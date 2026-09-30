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
//! `roster.rs` (the unit strip), `settings_menu.rs` (the settings menu),
//! `network_menu.rs` (its Multiplayer section),
//! `tooltips.rs`, `text.rs` (number and text
//! formatting), `tests.rs`.

mod action_icons;
mod builder;
mod dock;
mod imgui;
mod network_menu;
pub(in crate::game) use network_menu::NetField;
pub use network_menu::{NetMenu, NetRequest};
mod paint;
mod panels;
mod queue;
mod roster;
pub(in crate::game) use roster::RosterKey;
mod settings_menu;
mod text;
mod tooltips;
mod trays;

use glam::{Mat4, Vec2, Vec3};

use super::city::{BuildUnit, Building, Good};
use super::draw::UnitLook;
use super::hex::Hex;
use super::orders::ClickMode;
use super::scenario::Scenario;
use super::settings::Setting;
use super::workers::JobKind;
use super::{GameState, mesh};
use crate::renderer::Vertex;
use builder::PanelBuilder;
use dock::{Dock, Rect, Zone};
pub use imgui::{ImGuiLayoutState, style_imgui};

/// The least window size, in logical pixels, the game lets the window be
/// resized to: the ImGui status bar has room for all of its controls
/// (`imgui_status_bar_keeps_its_parts_apart_at_every_width`).
pub const MIN_WINDOW_SIZE: [f32; 2] = [640.0, 480.0];
use paint::{draw_button, draw_chip_hover, draw_shape};

const BUILDING_LIST_VISIBLE: usize = 5;
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
/// A title with a button at its right end (a queue's title and its Clear
/// button): the button's height, and the row's with the gap under it.
const TITLE_BUTTON_HEIGHT: f32 = 30.0;
const TITLE_ROW_HEIGHT: f32 = TITLE_BUTTON_HEIGHT + QUEUE_ITEM_GAP;
/// A unit's square in the unit strip, and the space between squares.
const ROSTER_CHIP: f32 = 44.0;
const ROSTER_CHIP_GAP: f32 = 6.0;
/// Most units in one row of the strip before it wraps.
const ROSTER_PER_ROW: usize = 12;
/// How much of its square a unit's token fills, across.
const ROSTER_TOKEN_SHARE: f32 = 0.8;
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
/// A build card the stockpile can't pay for yet (`ButtonSpec::short`): a
/// muted red rim, a faintly red ground and its price in `SHORT_PRICE`, bright
/// enough to read and press, unlike a disabled card's grey.
const SHORT_BORDER_COLOR: Color = [0.58, 0.28, 0.24, 1.0];
const SHORT_BG: Color = [0.06, 0.038, 0.036, 0.95];
const SHORT_HOVER_BG: Color = [0.10, 0.065, 0.06, 0.95];
const SHORT_PRICE: Color = [0.88, 0.48, 0.42, 1.0];
/// A short card that's also queued keeps its gold ground: its price in a
/// dark red that reads on gold.
const SHORT_QUEUED_PRICE: Color = [0.42, 0.14, 0.10, 1.0];
/// The stockpile's resources, wherever they're named.
const FOOD_TEXT: Color = [0.62, 0.90, 0.40, 1.0];
const WOOD_TEXT: Color = [0.85, 0.62, 0.36, 1.0];
const METAL_TEXT: Color = [0.62, 0.74, 0.92, 1.0];
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
    OpenSettings,
    Building(Building),
    BarracksBuild(BuildUnit),
    OpenBarracks,
    OpenCity,
    OpenInterior,
    InteriorClear,
    CityQueueRemove(usize),
    BarracksQueueRemove(usize),
    /// A queue panel's Clear button: every item off, each refunded.
    ClearCityQueue,
    ClearBarracksQueue,
    /// A worker for the open city's pool.
    BuildWorker,
    /// A scout from the open city's own queue.
    BuildScout,
    /// A settler from the open city's own queue.
    BuildSettler,
    /// One more citizen for the open city, bought with food.
    Grow,
    /// The open city spends a turn gathering.
    Gather,
    /// A card in the open city's production list for something its
    /// workers build (a road, improvement or structure): arms it for
    /// placing on the map.
    WorkerJob(JobKind),
    /// Shown while the open city is placing something: stops placing it
    /// (like Escape or a right-click on the map), with nothing placed.
    CancelPlacing,
    /// The X on one of the open city's worker jobs.
    WorkerJobRemove(usize),
    /// Sends the worker with this id straight home, to stay until released.
    RecallWorker(u32),
    /// Lets one of the open city's held (recalled) workers take jobs again.
    ReleaseWorker,
    /// A worker's row in the city panel: the camera goes to it.
    ShowWorker(u32),
    /// A click on a queue row (not a drag): for a worker job, the camera
    /// goes to it.
    QueueItem(QueueKind, usize),
    /// The unit strip, by unit id: click selects that unit and moves the
    /// camera to it, Shift-click adds it to the selection, and Ctrl-click
    /// takes it out.
    RosterSelect(RosterKey),
    RosterAdd(RosterKey),
    RosterRemove(RosterKey),
    /// A chip of the open city's priority order: a click puts its good
    /// first (dragging one onto another reorders them, `QueueKind::Priority`).
    Priority(Good),
    EndTurn,
    /// Debug panel: scenario pages, the savestate, playback pacing and fog.
    Scenario(Scenario),
    SaveState,
    LoadState,
    CompleteProduction,
    TogglePlayback,
    ToggleFog,
    /// Debug panel: production speeds builds, or builds take fixed time.
    ToggleProductionSpeedup,
    /// Debug panel: the Cavalry and Armored cap counts those alive, or every
    /// one ever trained.
    ToggleLifetimeCap,
    /// Settings menu: set a setting to a value (the nearer end of its range
    /// if outside it), from its checkbox, slider or choice.
    SetSetting(Setting, i32),
    CloseSettings,
    /// Settings menu: close the game.
    Quit,
    /// Settings menu: the Multiplayer page, and back.
    OpenMultiplayer,
    CloseMultiplayer,
    /// Multiplayer: how many people to host for.
    NetPlayers(usize),
    /// Multiplayer, classic: type into this field (again: stop).
    EditNetField(NetField),
    HostGame,
    JoinGame,
    /// Multiplayer: leave the network game.
    LeaveGame,
    /// Multiplayer, hosting: put the join code, or this machine's address
    /// on the local network, on the clipboard.
    CopyJoinCode,
    CopyHostAddress,
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
    /// Stay put and attack enemies that come in range (`toggle_alert`).
    Alert,
    Settle,
    Disband,
    /// Drops every order, queue, hold, guard and alert (Ctrl-right-click).
    ClearOrders,
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
    /// Queued or chosen: gold (`ButtonSpec::queued`).
    queued: bool,
    /// Why it can't be pressed, if it can't: disabled
    /// (`ButtonSpec::unavailable`).
    unavailable: Option<String>,
    /// This button's action is what the next map click will do.
    armed: bool,
    /// A build the stockpile can't pay for yet (`ButtonSpec::short`).
    short: bool,
    /// Part of the debug panel, drawn see-through so it doesn't read as game UI.
    faded: bool,
    min: Vec2,
    max: Vec2,
}

impl Button {
    /// How it's drawn: disabled when unavailable, else gold when queued.
    fn state(&self) -> ButtonState {
        ButtonState::new(self.queued, self.unavailable.is_some())
    }

    /// Drawn short: a red rim and price (`ButtonSpec::drawn_short`).
    fn drawn_short(&self) -> bool {
        self.short && self.unavailable.is_none()
    }

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
        waiting: bool,
        dragging: bool,
        drop_target: bool,
    },
    /// A unit's token in the unit strip, framed while it's selected.
    UnitChip {
        min: Vec2,
        max: Vec2,
        chip: RosterChip,
    },
}

/// A chip in the turn strip: what it stands for, its picture, whether it's
/// selected (its units, or its city open), and how many units or workers it
/// counts (shown when more than one).
#[derive(Clone, Copy)]
pub(super) struct RosterChip {
    pub(super) key: RosterKey,
    pub(super) icon: ChipIcon,
    pub(super) color: Color,
    pub(super) selected: bool,
    pub(super) count: usize,
}

/// A turn strip chip's picture.
#[derive(Clone, Copy)]
pub(super) enum ChipIcon {
    /// A unit's (or worker's) token, as on the map.
    Unit(UnitLook),
    /// A city's tower.
    City,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum QueueKind {
    City,
    Barracks,
    /// The open city's worker jobs, listed in its tray.
    Workers,
    /// The open city's priority order: its food, wood and metal chips
    /// (`Row::Reorder`), which never scroll or come off.
    Priority,
}

impl QueueKind {
    /// What a row's X button does.
    fn remove_target(self, index: usize) -> Target {
        match self {
            QueueKind::City => Target::CityQueueRemove(index),
            QueueKind::Barracks => Target::BarracksQueueRemove(index),
            QueueKind::Workers => Target::WorkerJobRemove(index),
            QueueKind::Priority => unreachable!("priority chips have no X"),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct QueueDrag {
    kind: QueueKind,
    source: usize,
    target: Option<usize>,
}

#[derive(Clone)]
struct QueueItemSpec {
    kind: QueueKind,
    index: usize,
    label: String,
    active: bool,
    /// Its item waits for the stockpile (`waiting_items`): tinted.
    waiting: bool,
    dragging: bool,
    drop_target: bool,
}

struct QueueItemRegion {
    kind: QueueKind,
    index: usize,
    min: Vec2,
    max: Vec2,
    body_max_x: f32,
    /// Can't be dragged: a priority chip whose button is off.
    locked: bool,
}

struct BuildingScrollRegion {
    city: usize,
    min: Vec2,
    max: Vec2,
    track_min: Vec2,
    track_max: Vec2,
    thumb_height: f32,
    max_offset: usize,
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
    building_scrollbars: Vec<BuildingScrollRegion>,
    queue_items: Vec<QueueItemRegion>,
    /// The unit strip's tokens and the unit id each one stands for.
    roster_chips: Vec<(Vec2, Vec2, RosterKey)>,
    dock: Option<Dock>,
    /// Where the settings menu's shapes and buttons start, while it's open:
    /// `build_ui` draws them after everything before them, buttons
    /// included, so no other panel's buttons show through it.
    overlay: Option<(usize, usize)>,
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
    /// The button under `point`: the last placed, which draws on top, if
    /// panels overlap (only the centered settings menu does).
    fn button_at(&self, point: Vec2) -> Option<&Button> {
        self.buttons.iter().rev().find(|b| b.contains(point))
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

    /// The unit whose token in the unit strip is under `point`, if any.
    fn roster_chip_at(&self, point: Vec2) -> Option<RosterKey> {
        self.roster_chips
            .iter()
            .find(|&&(min, max, _)| contains(min, max, point))
            .map(|&(_, _, id)| id)
    }
}

/// What a click on unit `id` in the unit strip does, by its modifiers.
fn roster_target(key: RosterKey, mode: ClickMode) -> Target {
    match mode {
        ClickMode::QueueMove => Target::RosterAdd(key),
        ClickMode::Swap => Target::RosterRemove(key),
        _ => Target::RosterSelect(key),
    }
}

impl GameState {
    /// The UI as a triangle list in UI pixels. `cursor` is in window pixels
    /// with the origin at the top-left.
    #[cfg(test)]
    pub fn build_ui(&self, screen_size: Vec2, cursor: Option<Vec2>) -> Vec<Vertex> {
        let mut out = Vec::new();
        self.build_ui_into(screen_size, cursor, &mut out);
        out
    }

    /// `build_ui` into `out`, cleared first, reusing its memory from frame
    /// to frame.
    pub fn build_ui_into(&self, screen_size: Vec2, cursor: Option<Vec2>, out: &mut Vec<Vertex>) {
        out.clear();
        let (layout, over_ui) = self.layout_with_hover(screen_size, cursor);
        let point = cursor.map(|c| to_ui(c, screen_size));
        let hovered = point.and_then(|p| layout.button_at(p)).map(|b| b.target);

        // The settings menu (and anything placed after it) is a layer of
        // its own over the rest.
        let (shapes_split, buttons_split) = layout
            .overlay
            .unwrap_or((layout.shapes.len(), layout.buttons.len()));
        let (under_shapes, over_shapes) = layout.shapes.split_at(shapes_split);
        let (under_buttons, over_buttons) = layout.buttons.split_at(buttons_split);
        for shape in under_shapes {
            draw_shape(shape, out);
        }
        if let Some(&(min, max, _)) = point.and_then(|p| {
            layout
                .roster_chips
                .iter()
                .find(|&&(min, max, _)| contains(min, max, p))
        }) {
            draw_chip_hover(min, max, out);
        }
        for button in under_buttons {
            draw_button(button, hovered == Some(button.target), out);
        }
        for shape in over_shapes {
            draw_shape(shape, out);
        }
        for button in over_buttons {
            draw_button(button, hovered == Some(button.target), out);
        }
        if let Some((lines, rect)) = point.and_then(|p| self.classic_tooltip_at(&layout, p)) {
            self.draw_tooltip(lines, rect, &layout, screen_size, out);
        }
        if let (Some(point), Some(hex)) = (point, self.hovered_tile)
            && self.hover_seconds >= TILE_TOOLTIP_DELAY
            && !over_ui
        {
            self.draw_tile_tooltip(hex, point, screen_size, out);
        }
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
        if !over_ui
            && let Some(hex) = self.hovered_tile
            && let Some(panel) = self.structure_inspect_panel(hex)
        {
            layout.dock_panel(panel, Zone::BottomLeft);
        }

        (layout, over_ui)
    }

    /// Shared visibility-filtered structure inspection for both UI presentations.
    fn structure_inspect_panel(&self, hex: Hex) -> Option<PanelBuilder> {
        let fog = self.fog();
        let known = |city: &super::city::City| city.team == self.local_team || fog.sees(hex);
        let (city, barracks) = self.cities.iter().enumerate().find_map(|(idx, city)| {
            (known(city) && city.pos == hex)
                .then_some((idx, false))
                .or_else(|| (known(city) && city.barracks == Some(hex)).then_some((idx, true)))
        })?;
        let mut panel = PanelBuilder::default();
        self.structure_hover_panel(city, barracks, &mut panel);
        Some(panel)
    }

    /// Handles a click on the UI, returning whether it hit anything (in which
    /// case it shouldn't also count as a click on the map). `cursor` is in
    /// window pixels with the origin at the top-left.
    /// `mode` carries the click's modifiers: Shift-clicking the unit strip
    /// adds to the selection and Ctrl-clicking takes out.
    pub(super) fn click_ui(&mut self, cursor: Vec2, screen_size: Vec2, mode: ClickMode) -> bool {
        let point = to_ui(cursor, screen_size);
        let layout = self.layout_with_hover(screen_size, Some(cursor)).0;
        // A button (the centered settings menu's, drawn over the panels)
        // takes the click before a scrollbar or a chip under it.
        if layout.button_at(point).is_none()
            && (self.drag_queue_scrollbar_at(cursor, screen_size, false)
                || self.drag_building_scrollbar_at(cursor, screen_size, false))
        {
            return true;
        }
        if layout.button_at(point).is_none()
            && let Some(key) = layout.roster_chip_at(point)
        {
            self.activate_target(roster_target(key, mode));
            return true;
        }
        let Some(button) = layout.button_at(point) else {
            return layout.covers(point);
        };
        if button.unavailable.is_some() {
            return true;
        }
        self.activate_target(button.target);
        true
    }

    fn activate_target(&mut self, target: Target) {
        // Any other button ends typing into a field.
        if !matches!(target, Target::EditNetField(_)) {
            self.stop_typing();
        }
        match target {
            Target::Unit(action) => match action {
                UnitAction::Move => self.choose_move_action(),
                UnitAction::Attack => self.choose_attack_action(),
                UnitAction::Swap => self.choose_swap_action(),
                UnitAction::Ability => self.toggle_selected_ability(),
                UnitAction::Hold => self.hold_selected_unit(),
                UnitAction::Guard => self.toggle_guard(),
                UnitAction::Alert => self.toggle_alert(),
                UnitAction::Settle => self.found_city_selected(),
                UnitAction::Disband => self.disband_selected(),
                UnitAction::ClearOrders => self.handle_right_click(),
            },
            Target::RosterSelect(id) => self.roster_select(id),
            Target::RosterAdd(id) => self.roster_add(id),
            Target::RosterRemove(id) => self.roster_remove(id),
            Target::BuildWorker => self.queue_selected_city_worker(),
            Target::BuildScout => self.queue_selected_city_scout(),
            Target::BuildSettler => self.queue_selected_city_settler(),
            Target::Grow => self.queue_selected_city_growth(),
            Target::Gather => self.queue_selected_city_gather(),
            Target::WorkerJob(kind) => self.arm_worker_job(kind),
            Target::CancelPlacing => {
                self.stop_placing();
            }
            Target::WorkerJobRemove(index) => self.remove_worker_job(index),
            Target::RecallWorker(id) => self.recall_worker(id),
            Target::ReleaseWorker => self.release_worker(),
            Target::ShowWorker(id) => self.show_worker(id),
            Target::QueueItem(kind, index) => self.queue_item_clicked(kind, index),
            Target::Build(build) => self.queue_selected_city_unit(build),
            Target::ToggleYields => self.toggle_yields(),
            Target::OpenSettings => self.settings_open = true,
            Target::Building(building) => self.queue_selected_city_building(building),
            Target::BarracksBuild(build) => self.queue_selected_barracks_unit(build),
            Target::OpenBarracks => {
                if let Some(city) = self.selected_city {
                    self.open_barracks(city)
                }
            }
            Target::OpenCity => self.open_selected_city_from_barracks(),
            Target::OpenInterior => self.toggle_city_interior(),
            Target::InteriorClear => self.clear_selected_interior_orders(),
            Target::CityQueueRemove(index) => self.remove_selected_city_queue_item(index),
            Target::BarracksQueueRemove(index) => self.remove_selected_barracks_queue_item(index),
            Target::ClearCityQueue => self.clear_selected_city_queue(),
            Target::ClearBarracksQueue => self.clear_selected_barracks_queue(),
            Target::Priority(good) => self.prioritize_selected_city(good),
            // While a network game waits for the others' plans, End Turn
            // takes this side's back.
            Target::EndTurn if self.waiting_for_peers() => self.take_back_turn(),
            Target::EndTurn => self.end_planning(),
            Target::Scenario(scenario) => self.switch_scenario(scenario),
            Target::SaveState => self.save_state(),
            Target::LoadState => self.load_state(),
            Target::CompleteProduction => self.debug_complete_current_production(),
            Target::TogglePlayback => self.toggle_instant_playback(),
            Target::ToggleFog => self.toggle_fog(),
            Target::ToggleProductionSpeedup => self.toggle_production_speedup(),
            Target::ToggleLifetimeCap => self.toggle_lifetime_special_cap(),
            Target::SetSetting(setting, value) => self.set_setting(setting, value),
            Target::CloseSettings => self.close_settings(),
            Target::Quit => self.quit_requested = true,
            Target::OpenMultiplayer
            | Target::CloseMultiplayer
            | Target::NetPlayers(_)
            | Target::EditNetField(_)
            | Target::HostGame
            | Target::JoinGame
            | Target::LeaveGame
            | Target::CopyJoinCode
            | Target::CopyHostAddress => self.activate_network_target(target),
        }
    }

    /// Tracks which map hex the cursor is over (ignoring the UI) and how long
    /// it has rested there, for city hover outlines and the tile tooltip.
    /// `cursor` is in window pixels with the origin at the top-left.
    pub fn update_hover(&mut self, cursor: Option<Vec2>, screen_size: Vec2, dt: f32) {
        let layout = self.layout(screen_size);
        let hex = cursor
            .filter(|&c| !layout.covers(to_ui(c, screen_size)))
            .and_then(|c| self.hex_at_screen(c, screen_size));
        if self.hover_interior(hex) {
            return;
        }
        if hex == self.hovered_tile {
            self.hover_seconds += dt;
        } else {
            self.hovered_tile = hex;
            self.hover_seconds = 0.0;
        }
        self.hovered_city = hex.and_then(|h| self.cities.iter().position(|c| c.pos == h));
        self.hover_edge(hex.and(cursor), screen_size);
    }

    pub fn update_hover_imgui(&mut self, cursor: Option<Vec2>, screen_size: Vec2, dt: f32) {
        let hex = cursor.and_then(|c| self.hex_at_screen(c, screen_size));
        if self.hover_interior(hex) {
            return;
        }
        if hex == self.hovered_tile {
            self.hover_seconds += dt;
        } else {
            self.hovered_tile = hex;
            self.hover_seconds = 0.0;
        }
        self.hovered_city = hex.and_then(|h| self.cities.iter().position(|c| c.pos == h));
        self.hover_edge(hex.and(cursor), screen_size);
    }

    /// In a city interior, `hex` (an interior tile) is the hovered one, for
    /// the attack preview, and there's no map hover; says whether it is.
    fn hover_interior(&mut self, hex: Option<Hex>) -> bool {
        let inside = self.interior_view.is_some();
        self.hovered_interior = hex.filter(|_| inside);
        if inside {
            self.hovered_tile = None;
            self.hovered_city = None;
            self.hover_seconds = 0.0;
        }
        inside
    }

    /// With a wall or gate armed, the hex edge under `cursor` (over the map,
    /// not the UI), for its highlight.
    fn hover_edge(&mut self, cursor: Option<Vec2>, screen_size: Vec2) {
        self.hovered_job =
            cursor.and_then(|c| self.job_target_at(self.camera.screen_to_world(c, screen_size)));
    }

    pub fn set_ui_notice(&mut self, notice: &str) {
        self.notice = notice.into();
    }

    fn layout(&self, screen_size: Vec2) -> Layout {
        let mut layout = Layout::for_screen(screen_size);
        self.top_bar(screen_size, &mut layout);

        let mut tray = PanelBuilder::default();
        if let Some(city) = self.interior_view {
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
        } else {
            self.debug_panel(&mut layout);
            self.dock_roster(&mut layout);
            self.place_settings(screen_size, &mut layout);
            return layout;
        }
        // A tray taller than the screen wouldn't dock at all: what scrolls
        // in it (a city's workers and jobs, its catalogue) shows less.
        tray.fit_height(layout.remaining_height(Zone::BottomLeft, tray.size().x));
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
        self.dock_roster(&mut layout);
        self.place_settings(screen_size, &mut layout);
        layout
    }

    /// Whether `cursor` (window pixels, origin top-left) is over a classic
    /// panel, where a left drag isn't a selection box.
    pub fn ui_covers(&self, cursor: Vec2, screen_size: Vec2) -> bool {
        let point = to_ui(cursor, screen_size);
        self.layout_with_hover(screen_size, Some(cursor))
            .0
            .covers(point)
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
