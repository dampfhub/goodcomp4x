//! Screen-space UI: the top status bar, the bottom-left command tray (the
//! selected unit's stats and actions, or the open city's economy and build
//! options), tooltips for every button, and an info box for a hovered unit.
//!
//! Each frame the UI is laid out once into panels, text and buttons (a
//! `Layout`), which is then either drawn or hit-tested against a click, so
//! what's clickable always matches what's shown. UI geometry is in pixels
//! with the origin at the window's bottom-left and Y pointing up, the same
//! orientation as the world.

use glam::{Mat4, Vec2, Vec3};

use super::ability::Ability;
use super::city::{Build, Building, BuildUnit, LaborFocus, delivered_share};
use super::font::{self, Face};
use super::hex::Hex;
use super::orders::ClickMode;
use super::scenario::Scenario;
use super::unit::Unit;
use super::{GameState, mesh};
use crate::renderer::Vertex;

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
    ChangeBarracksSite,
    BarracksBuild(BuildUnit),
    OpenBarracks,
    OpenCity,
    CityQueueUp(usize),
    CityQueueDown(usize),
    CityQueueRemove(usize),
    BarracksQueueUp(usize),
    BarracksQueueDown(usize),
    BarracksQueueRemove(usize),
    Focus(LaborFocus),
    ConfirmBarracks,
    EndTurn,
    /// Debug panel: scenario pages, the savestate, playback pacing and fog.
    Scenario(Scenario),
    SaveState,
    LoadState,
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
    Panel { min: Vec2, max: Vec2, faded: bool },
    Text { origin: Vec2, px: u32, line: Line },
    Bar { min: Vec2, max: Vec2, fraction: f32 },
}

/// The UI for one frame. Panels also swallow clicks, so a click on the UI
/// never falls through to the map.
#[derive(Default)]
struct Layout {
    shapes: Vec<Shape>,
    buttons: Vec<Button>,
    panels: Vec<(Vec2, Vec2)>,
}

impl Layout {
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

/// Stack a map-hover card with the bottom-left trays without colliding with
/// the debug panel at the top of the window.
fn hover_panel_bottom(layout: &Layout, screen_size: Vec2) -> f32 {
    layout
        .panels
        .iter()
        .filter(|&&(min, max)| min.x <= MARGIN + 1.0 && max.y < screen_size.y * 0.6)
        .map(|&(_, max)| max.y + GAP)
        .fold(MARGIN, f32::max)
}

impl GameState {
    /// The UI as a triangle list in UI pixels. `cursor` is in window pixels
    /// with the origin at the top-left.
    pub fn build_ui(&self, screen_size: Vec2, cursor: Option<Vec2>) -> Vec<Vertex> {
        let mut layout = self.layout(screen_size);
        let point = cursor.map(|c| to_ui(c, screen_size));
        let hovered = point.and_then(|p| layout.button_at(p)).map(|b| b.target);

        // Describe a unit under the cursor, unless it's the selected one
        // (already in the tray) or the cursor is over the UI.
        let over_ui = point.is_some_and(|p| layout.covers(p));
        if let Some(idx) = cursor
            .filter(|_| !over_ui)
            .and_then(|c| self.unit_at_screen(c, screen_size))
            .filter(|&idx| Some(idx) != self.selected || self.selected_city.is_some())
            .filter(|_| !self.hovered_tile.is_some_and(|hex| self.cities.iter().any(|city| city.pos == hex || city.barracks == Some(hex))))
        {
            // Top-right, clear of the debug panel at the top-left.
            let mut panel = PanelBuilder::default();
            self.unit_info(idx, &mut panel);
            let top_left = Vec2::new(
                screen_size.x - MARGIN - panel.size().x,
                screen_size.y - TOP_BAR_HEIGHT - MARGIN,
            );
            panel.place_top_left(top_left, &mut layout);
        }
        if !over_ui && let Some(hex) = self.hovered_tile {
            if let Some(city) = self.cities.iter().position(|city| city.pos == hex) {
                let mut panel = PanelBuilder::default();
                self.structure_hover_panel(city, false, &mut panel);
                panel.place_bottom_left(Vec2::new(MARGIN, hover_panel_bottom(&layout, screen_size)), &mut layout);
            } else if let Some(city) = self.cities.iter().position(|city| city.barracks == Some(hex)) {
                let mut panel = PanelBuilder::default();
                self.structure_hover_panel(city, true, &mut panel);
                panel.place_bottom_left(Vec2::new(MARGIN, hover_panel_bottom(&layout, screen_size)), &mut layout);
            }
        }

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

    /// Handles a click on the UI, returning whether it hit anything (in which
    /// case it shouldn't also count as a click on the map). `cursor` is in
    /// window pixels with the origin at the top-left.
    pub(super) fn click_ui(&mut self, cursor: Vec2, screen_size: Vec2) -> bool {
        let point = to_ui(cursor, screen_size);
        let layout = self.layout(screen_size);
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
            Target::ChangeBarracksSite => self.change_selected_barracks_site(),
            Target::BarracksBuild(build) => self.queue_selected_barracks_unit(build),
            Target::OpenBarracks => if let Some(city) = self.selected_city { self.open_barracks(city) },
            Target::OpenCity => self.open_selected_city_from_barracks(),
            Target::CityQueueUp(index) => self.move_selected_city_queue_item(index, true),
            Target::CityQueueDown(index) => self.move_selected_city_queue_item(index, false),
            Target::CityQueueRemove(index) => self.remove_selected_city_queue_item(index),
            Target::BarracksQueueUp(index) => self.move_selected_barracks_queue_item(index, true),
            Target::BarracksQueueDown(index) => self.move_selected_barracks_queue_item(index, false),
            Target::BarracksQueueRemove(index) => self.remove_selected_barracks_queue_item(index),
            Target::Focus(focus) => self.set_selected_city_focus(focus),
            Target::ConfirmBarracks => self.confirm_barracks(),
            Target::EndTurn => self.end_planning(),
            Target::Scenario(scenario) => self.switch_scenario(scenario),
            Target::SaveState => self.save_state(),
            Target::LoadState => self.load_state(),
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
        let mut layout = Layout::default();
        self.top_bar(screen_size, &mut layout);
        self.debug_panel(screen_size, &mut layout);

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
            return layout;
        }
        let tray_size = tray.size();
        tray.place_bottom_left(Vec2::splat(MARGIN), &mut layout);
        if let Some(city) = self.selected_city {
            let mut queue = PanelBuilder::default();
            self.city_queue_panel(city, &mut queue);
            if !queue.rows.is_empty() {
                queue.place_bottom_left(
                    Vec2::new(MARGIN, MARGIN + tray_size.y + GAP),
                    &mut layout,
                );
            }
        } else if let Some(city) = self.selected_barracks {
            let mut queue = PanelBuilder::default();
            self.barracks_queue_panel(city, &mut queue);
            if !queue.rows.is_empty() {
                queue.place_bottom_left(
                    Vec2::new(MARGIN, MARGIN + tray_size.y + GAP),
                    &mut layout,
                );
            }
        }
        layout
    }

    /// Testing tools, top-left under the top bar and see-through so they
    /// don't read as game UI: the scenario pages (the current one gold;
    /// pressing it again restarts it) and the savestate.
    fn debug_panel(&self, size: Vec2, layout: &mut Layout) {
        let mut panel = PanelBuilder {
            faded: true,
            ..PanelBuilder::default()
        };
        panel.text(SMALL, vec![("DEBUG".into(), fade(LABEL_TEXT, true))]);
        let debug_button = |target, label: &str, hint: &str, state| ButtonSpec {
            target,
            label: label.into(),
            hint: hint.into(),
            state,
            armed: false,
        };
        panel.compact_buttons(
            Scenario::ALL
                .into_iter()
                .map(|scenario| {
                    let current = ButtonState::new(scenario == self.scenario, false);
                    debug_button(
                        Target::Scenario(scenario),
                        scenario.name(),
                        scenario.key(),
                        current,
                    )
                })
                .collect(),
        );
        if let Some(seed) = self.map_seed {
            panel.text(
                SMALL,
                vec![(format!("MAP SEED {seed}"), fade(DIM_TEXT, true))],
            );
        }
        panel.gap(GAP);
        panel.compact_buttons(vec![
            debug_button(
                Target::SaveState,
                "SAVE",
                "F6",
                ButtonState::new(false, self.is_resolving()),
            ),
            debug_button(
                Target::LoadState,
                "LOAD",
                "F7",
                ButtonState::new(false, self.savestate.is_none()),
            ),
        ]);
        if let Some(saved) = self.saved_summary() {
            panel.text(
                SMALL,
                vec![(format!("SAVED: {saved}"), fade(DIM_TEXT, true))],
            );
        }
        panel.gap(GAP);
        let playback = if self.instant_playback {
            "PLAYBACK: ALL AT ONCE"
        } else {
            "PLAYBACK: STEP BY STEP"
        };
        let fog = if self.fog_of_war {
            "FOG OF WAR: ON"
        } else {
            "FOG OF WAR: OFF"
        };
        panel.compact_buttons(vec![debug_button(
            Target::TogglePlayback,
            playback,
            "F8",
            ButtonState::Ready,
        )]);
        panel.compact_buttons(vec![debug_button(
            Target::ToggleFog,
            fog,
            "F9",
            ButtonState::Ready,
        )]);
        let top_left = Vec2::new(MARGIN, size.y - TOP_BAR_HEIGHT - MARGIN);
        panel.place_top_left(top_left, layout);
    }

    /// Turn number on the left, the latest notice in the middle, and on the
    /// right the End Turn button, which names whatever the turn is still
    /// waiting on (clicking it selects that).
    fn top_bar(&self, size: Vec2, layout: &mut Layout) {
        let min = Vec2::new(0.0, size.y - TOP_BAR_HEIGHT);
        layout.panel(min, size, false);
        let middle = min.y + TOP_BAR_HEIGHT / 2.0;

        let pending = self.pending();
        let turn = if self.is_resolving() {
            self.turn
        } else {
            self.turn + 1
        };
        let turn_text = format!("TURN {turn}");
        let left_end = MARGIN + font::ui(TITLE).width(&turn_text);
        push_text_row(
            layout,
            Vec2::new(MARGIN, middle),
            TITLE,
            vec![(turn_text, TEXT)],
        );

        let label = if self.is_resolving() {
            "RESOLVING".to_string()
        } else {
            end_turn_label(pending)
        };
        let hint = "SPACE".to_string();
        let width = single_line_button_width(&label, &hint);
        let button_min = Vec2::new(size.x - MARGIN - width, middle - END_TURN_HEIGHT / 2.0);
        let end_turn = Button {
            target: Target::EndTurn,
            label,
            hint,
            state: if self.is_resolving() {
                ButtonState::Disabled
            } else {
                ButtonState::new(pending == (0, 0), false)
            },
            armed: false,
            faded: false,
            min: button_min.round(),
            max: (button_min + Vec2::new(width, END_TURN_HEIGHT)).round(),
        };

        // The notice sits centered in the space left between the two.
        if !self.notice.is_empty() {
            let width = font::ui(BODY).width(&self.notice);
            let space = (left_end + 2.0 * GAP, end_turn.min.x - 2.0 * GAP);
            let left = ((space.0 + space.1 - width) / 2.0).max(space.0);
            if left + width <= space.1 {
                let line = vec![(self.notice.clone(), NOTICE_TEXT)];
                push_text_row(layout, Vec2::new(left, middle), BODY, line);
            }
        }
        layout.buttons.push(end_turn);
    }

    /// A unit's name, stats as they stand this turn, and anything notable
    /// about it. Stats boosted above their base value are green, reduced red.
    fn unit_info(&self, idx: usize, panel: &mut PanelBuilder) {
        let unit = &self.units[idx];
        let base = unit.unit_type.stats();
        let stats = unit.stats();
        let terrain = self.grid.tile(unit.pos);
        let defense = stats.defense * terrain.defense_multiplier();
        let (role, _) = self.unit_role(unit);

        panel.text(
            TITLE,
            vec![(
                format!("{:?} {role}", unit.team).to_uppercase(),
                unit.team.color(),
            )],
        );
        let hp_color = if unit.hp < unit.max_hp() * 0.5 {
            REDUCED_TEXT
        } else {
            TEXT
        };
        panel.text(
            BODY,
            stat_spans(&[
                (
                    "HP",
                    format!("{:.0}/{:.0}", unit.hp.ceil(), unit.max_hp()),
                    hp_color,
                ),
                (
                    "ATTACK",
                    format!("{:.0}", stats.attack),
                    compare(stats.attack, base.attack),
                ),
                (
                    "DEFENSE",
                    format!("{defense:.0}"),
                    compare(defense, base.defense),
                ),
            ]),
        );
        let (ability_name, _) = ability_text(unit);
        let (ability_status, ability_color) = if unit.ability_queued {
            ("QUEUED".to_string(), GOLD_TEXT)
        } else if unit.ability_cooldown > 0 {
            (
                format!("IN {}", turns_text(unit.ability_cooldown)),
                DIM_TEXT,
            )
        } else {
            ("READY".to_string(), TEXT)
        };
        let mut second = stat_spans(&[
            (
                "RANGE",
                stats.attack_range.to_string(),
                compare(stats.attack_range as f32, base.attack_range as f32),
            ),
            (
                "MOVE",
                stats.move_range.to_string(),
                compare(stats.move_range as f32, base.move_range as f32),
            ),
        ]);
        if !self.settlers.contains(&unit.id) && !self.workers.contains(&unit.id) {
            second.push((format!("   {ability_name} "), LABEL_TEXT));
            second.push((ability_status, ability_color));
        }
        panel.text(BODY, second);

        let mut notes = Vec::new();
        if terrain.defense_multiplier() != 1.0 {
            let bonus = (terrain.defense_multiplier() - 1.0) * 100.0;
            notes.push(format!("+{bonus:.0}% DEFENSE FROM TERRAIN"));
        }
        if unit.deployed {
            notes.push("DEPLOYED".to_string());
        }
        if unit.lookout {
            notes.push(format!("LOOKOUT: +{} SIGHT", super::fog::LOOKOUT_SIGHT));
        }
        if self.rival_of(idx).is_some() {
            notes.push("CONTESTED".to_string());
        }
        if self.is_player_controlled(idx) {
            let mut orders = Vec::new();
            if unit.planned_move.is_some() {
                orders.push("MOVE");
            }
            if unit.planned_attack.is_some() {
                orders.push("ATTACK");
            }
            if unit.holding {
                orders.push("HOLD");
            }
            if unit.guarding {
                orders.push("GUARD");
            }
            if !orders.is_empty() {
                notes.push(format!("ORDERS: {}", orders.join(", ")));
            }
        }
        for note in notes {
            panel.text(SMALL, vec![(note, DIM_TEXT)]);
        }
    }

    /// The selected unit's order buttons: what it can do depends on whether
    /// it's a fighter, a settler or a worker.
    fn unit_buttons(&self, idx: usize) -> Vec<ButtonSpec> {
        let unit = &self.units[idx];
        let can_move = unit.stats().move_range > 0;
        let locked = self.rival_of(idx).is_some();
        let swapping = self.swap_partner(idx).is_some();
        let worker = self.workers.contains(&unit.id);
        let settler = self.settlers.contains(&unit.id);
        let armed = |mode| self.ui_click_mode == Some(mode);

        let mut buttons = vec![ButtonSpec {
            target: Target::Unit(UnitAction::Move),
            label: "MOVE".into(),
            hint: "M".into(),
            state: ButtonState::new(unit.planned_move.is_some() && !swapping, !can_move),
            armed: armed(ClickMode::Move),
        }];
        if !worker {
            buttons.push(ButtonSpec {
                target: Target::Unit(UnitAction::Attack),
                label: "ATTACK".into(),
                hint: "X · SHIFT".into(),
                state: ButtonState::new(
                    unit.planned_attack.is_some(),
                    !unit.can_attack() || locked,
                ),
                armed: armed(ClickMode::Attack),
            });
        }
        buttons.push(ButtonSpec {
            target: Target::Unit(UnitAction::Swap),
            label: "SWAP".into(),
            hint: "CTRL".into(),
            state: ButtonState::new(swapping, !can_move || locked),
            armed: armed(ClickMode::Swap),
        });
        if settler {
            buttons.push(ButtonSpec::plain(UnitAction::Settle, "FOUND CITY", "F"));
        } else if worker {
            buttons.push(ButtonSpec::plain(UnitAction::Road, "BUILD ROAD", "R"));
            buttons.push(ButtonSpec::plain(UnitAction::Improve, "IMPROVE", "I"));
        } else {
            let (name, _) = ability_text(unit);
            let label = match unit.ability_cooldown {
                0 => name.to_string(),
                turns => format!("{name} ({turns})"),
            };
            buttons.push(ButtonSpec {
                target: Target::Unit(UnitAction::Ability),
                label,
                hint: "Q".into(),
                state: ButtonState::new(unit.ability_queued, unit.ability_cooldown > 0),
                armed: false,
            });
        }
        buttons.push(ButtonSpec {
            target: Target::Unit(UnitAction::Hold),
            label: "HOLD".into(),
            hint: "SPACE".into(),
            state: ButtonState::new(unit.holding, false),
            armed: false,
        });
        buttons.push(ButtonSpec {
            target: Target::Unit(UnitAction::Guard),
            label: "GUARD".into(),
            hint: "G".into(),
            state: ButtonState::new(unit.guarding, false),
            armed: false,
        });
        buttons
    }

    /// A group of selected units: how many of each, how to order them, and
    /// buttons for what every member can do at once.
    fn group_tray(&self, panel: &mut PanelBuilder) {
        let members: Vec<&str> = self
            .group
            .iter()
            .map(|&i| self.unit_role(&self.units[i]).0)
            .collect();
        let mut kinds: Vec<(&str, usize)> = Vec::new();
        for role in members {
            match kinds.iter_mut().find(|(kind, _)| *kind == role) {
                Some((_, count)) => *count += 1,
                None => kinds.push((role, 1)),
            }
        }
        let summary: Vec<String> = kinds
            .into_iter()
            .map(|(kind, count)| format!("{count} {kind}"))
            .collect();

        panel.text(
            TITLE,
            vec![(format!("{} UNITS SELECTED", self.group.len()), TEXT)],
        );
        panel.text(BODY, vec![(summary.join(", "), DIM_TEXT)]);
        for help in [
            "CLICK A HEX: EACH MOVES AS CLOSE TO IT AS IT CAN",
            "CLICK AN ENEMY: EVERY UNIT IN RANGE ATTACKS IT",
            "CLICK ONE UNIT TO SELECT JUST IT · ALT-CLICK ADDS OR REMOVES",
        ] {
            panel.text(SMALL, vec![(help.into(), LABEL_TEXT)]);
        }
        panel.gap(GAP);

        let armed = |mode| self.ui_click_mode == Some(mode);
        let all_guarding = self.group.iter().all(|&i| self.units[i].guarding);
        panel.buttons(vec![
            ButtonSpec {
                target: Target::Unit(UnitAction::Move),
                label: "MOVE".into(),
                hint: "M".into(),
                state: ButtonState::Ready,
                armed: armed(ClickMode::Move),
            },
            ButtonSpec {
                target: Target::Unit(UnitAction::Attack),
                label: "ATTACK".into(),
                hint: "X · SHIFT".into(),
                state: ButtonState::Ready,
                armed: armed(ClickMode::Attack),
            },
            ButtonSpec::plain(UnitAction::Hold, "HOLD", "SPACE"),
            ButtonSpec {
                target: Target::Unit(UnitAction::Guard),
                label: "GUARD".into(),
                hint: "G".into(),
                state: ButtonState::new(all_guarding, false),
                armed: false,
            },
        ]);
    }

    /// The open city: population, stores and income, growth, what it's
    /// building, and a card for each unit it can build.
    fn structure_hover_panel(&self, i: usize, barracks: bool, panel: &mut PanelBuilder) {
        let city = &self.cities[i];
        if barracks {
            let tile = city.barracks.unwrap();
            let active = city.worked.first() == Some(&tile);
            let production = active.then(|| (self.income(i).1 - 4).max(0)).unwrap_or(0);
            let queue = city.barracks_queue.first().map_or("EMPTY", |build| build.name());
            panel.text(TITLE, vec![(format!("CITY {} BARRACKS", city.id + 1), city.team.color())]);
            panel.text(SMALL, vec![(format!("HP {:.0}/{:.0} · +{production} PROD/T", city.barracks_hp, super::city::BARRACKS_MAX_HP), GOLD_TEXT)]);
            panel.text(SMALL, vec![(format!("QUEUE: {queue}"), DIM_TEXT)]);
            if let Some(build) = city.barracks_queue.first() {
                panel.bar((city.barracks_production as f32 / build.cost() as f32).clamp(0.0, 1.0));
            }
        } else {
            let (growth, _, _) = self.growth_status(i);
            let (_, production) = self.income(i);
            let queue = city.queue.first().map_or("EMPTY", |build| build.name());
            panel.text(TITLE, vec![(format!("CITY {}", city.id + 1), city.team.color())]);
            panel.text(SMALL, vec![(format!("HP {:.0}/{:.0} · POP {growth}% · +{production} PROD/T", city.hp, super::city::CITY_MAX_HP), GOLD_TEXT)]);
            panel.bar(growth as f32 / 100.0);
            panel.text(SMALL, vec![(format!("QUEUE: {queue}"), DIM_TEXT)]);
            if let Some(build) = city.queue.first() {
                panel.bar((city.production as f32 / build.cost() as f32).clamp(0.0, 1.0));
            }
        }
    }

    fn city_tray(&self, i: usize, panel: &mut PanelBuilder) {
        let city = &self.cities[i];
        let (food, production) = self.income(i);
        let net_food = food - city.population as i32 * 8;
        let (growth_percent, _, growth_label) = self.growth_status(i);

        panel.text(
            TITLE,
            vec![
                (format!("CITY {}", city.id + 1), city.team.color()),
                (
                    format!(
                        "   POPULATION {}/{}",
                        city.population,
                        super::city::MAX_CITY_POPULATION
                    ),
                    DIM_TEXT,
                ),
            ],
        );
        let net_color = match net_food.signum() {
            1 => BOOSTED_TEXT,
            -1 => REDUCED_TEXT,
            _ => TEXT,
        };
        panel.text(
            BODY,
            stat_spans(&[(
                "CITIZENS",
                format!(
                    "{} OF {} WORKING",
                    city.worked.len(),
                    city.population.min(super::city::MAX_CITY_POPULATION)
                ),
                TEXT,
            )]),
        );
        let mut food_line = stat_spans(&[("FOOD", quantity(city.food), TEXT)]);
        food_line.push((
            format!("   {} PER TURN", signed_quantity(net_food)),
            net_color,
        ));
        panel.text(BODY, food_line);
        let mut production_line = stat_spans(&[("PRODUCTION", quantity(city.production), TEXT)]);
        production_line.push((
            format!("   {} PER TURN", signed_quantity(production)),
            DIM_TEXT,
        ));
        panel.text(BODY, production_line);
        if let Some(build) = city.queue.first().copied() {
            panel.text(
                SMALL,
                vec![(
                    format!(
                        "{}: {} / {} PRODUCTION",
                        build.name(),
                        quantity(city.production.min(build.cost())),
                        quantity(build.cost())
                    ),
                    GOLD_TEXT,
                )],
            );
            panel.bar((city.production as f32 / build.cost() as f32).clamp(0.0, 1.0));
        }
        let building = match city.queue.first().copied() {
            Some(build) => (
                format!(
                    "{} ({} OF {})",
                    build.name(),
                    quantity(city.production.min(build.cost())),
                    quantity(build.cost())
                ),
                GOLD_TEXT,
            ),
            None => ("NOTHING - CHOOSE BELOW".to_string(), DIM_TEXT),
        };
        panel.text(BODY, stat_spans(&[("BUILDING", building.0, building.1)]));
        panel.text(SMALL, vec![("LABOR FOCUS".into(), LABEL_TEXT)]);
        panel.compact_buttons([LaborFocus::Food, LaborFocus::Production, LaborFocus::Balanced].into_iter().map(|focus| ButtonSpec {
            target: Target::Focus(focus), label: focus.name().into(), hint: "AUTO".into(),
            state: ButtonState::new(city.focus == focus, false), armed: false,
        }).collect());

        panel.gap(GAP);
        panel.text(SMALL, vec![(growth_label, DIM_TEXT)]);
        panel.bar(growth_percent as f32 / 100.0);

        if let Some(hex) = self.inspected_tile {
            let (tile_food, tile_production) = self.tile_yield(hex);
            let share = self
                .routes(i)
                .costs
                .get(&hex)
                .map_or(0, |cost| delivered_share(*cost));
            panel.gap(GAP);
            panel.text(
                SMALL,
                vec![(
                    format!(
                        "SELECTED TILE: {tile_food} FOOD, {tile_production} PRODUCTION, {}% DELIVERED",
                        share * 25
                    ),
                    DIM_TEXT,
                )],
            );
        }

        panel.gap(GAP);
        let builds = [
            BuildUnit::Melee,
            BuildUnit::Ranged,
            BuildUnit::Cavalry,
            BuildUnit::Siege,
        ];
        panel.buttons(
            builds
                .into_iter()
                .map(|build| ButtonSpec {
                    target: Target::Build(build),
                    label: build.name().into(),
                    hint: format!("{} · {} PROD", build.shortcut(), quantity(build.cost())),
                    state: ButtonState::new(city.queue.first() == Some(&Build::Unit(build)), false),
                    armed: false,
                })
                .collect(),
        );
        panel.gap(GAP);
        panel.buttons(vec![ButtonSpec {
            target: Target::ToggleYields,
            label: "YIELDS".into(),
            hint: "Y".into(),
            state: ButtonState::new(self.show_yields, false),
            armed: false,
        }]);
        panel.gap(GAP);
        panel.text(
            SMALL,
            vec![(
                "CLICK TILES TO ASSIGN CITIZENS · A AUTO-ASSIGN · CLICK A UNIT TO LEAVE".into(),
                LABEL_TEXT,
            )],
        );
        let buildings = [Building::Granary, Building::Barracks];
        panel.buttons(buildings.into_iter().filter(|&building| !(building == Building::Barracks && city.built.contains(&building))).map(|building| ButtonSpec {
            target: Target::Building(building), label: building.name().into(),
            hint: format!("{} · {} PROD", building.shortcut(), quantity(building.cost())),
            state: ButtonState::new(city.queue.first() == Some(&Build::Building(building)) || (building == Building::Barracks && self.placing_barracks == Some(i)), city.pending_building == Some(building) || city.queue.contains(&Build::Building(building))),
            armed: false,
        }).collect());
        if let Some(tile) = city.barracks {
            let active = city.worked.first() == Some(&tile);
            let status = if active { "MANAGER ACTIVE" } else { "MOVE MANAGER ONTO BARRACKS" };
            panel.gap(GAP);
            panel.text(SMALL, vec![(format!("BARRACKS: {status}"), if active { BOOSTED_TEXT } else { REDUCED_TEXT })]);
            panel.buttons(vec![ButtonSpec { target: Target::OpenBarracks, label: "SEE BARRACKS".into(), hint: "CLICK".into(), state: ButtonState::new(false, false), armed: false }]);
        } else if city.planned_barracks.is_some() {
            let site = city.planned_barracks.map(|h| format!("({}, {})", h.q, h.r)).unwrap_or_else(|| "NONE".into());
            let label = if city.pending_building == Some(Building::Barracks) { "BARRACKS READY" } else { "BARRACKS PLANNED" };
            panel.text(SMALL, vec![(format!("{label}: SITE {site}"), GOLD_TEXT)]);
            panel.buttons(vec![ButtonSpec {
                target: Target::ChangeBarracksSite, label: "CHANGE BARRACKS SITE".into(), hint: "CLICK".into(),
                state: ButtonState::new(self.placing_barracks == Some(i), false), armed: false,
            }]);
            if city.pending_building == Some(Building::Barracks) {
                panel.buttons(vec![ButtonSpec { target: Target::ConfirmBarracks, label: "CONFIRM BARRACKS".into(), hint: "CLICK".into(), state: ButtonState::new(false, false), armed: false }]);
            }
        }
    }

    fn barracks_tray(&self, i: usize, panel: &mut PanelBuilder) {
        let city = &self.cities[i];
        let Some(tile) = city.barracks else { return; };
        let active = city.worked.first() == Some(&tile);
        let (_, city_production) = self.income(i);
        let barracks_production = if active { (city_production - 4).max(0) } else { 0 };
        panel.text(TITLE, vec![(format!("CITY {} BARRACKS", city.id + 1), city.team.color())]);
        panel.text(BODY, stat_spans(&[("HP", format!("{:.0}/{:.0}", city.barracks_hp, super::city::BARRACKS_MAX_HP), GOLD_TEXT)]));
        panel.text(SMALL, vec![(format!("{} · +{barracks_production} PROD/T", if active { "MANAGER ACTIVE" } else { "NEEDS MANAGER" }), if active { BOOSTED_TEXT } else { REDUCED_TEXT })]);
        if let Some(build) = city.barracks_queue.first().copied() {
            panel.text(SMALL, vec![(format!("TRAINING {}: {} / {} PROD", build.name(), quantity(city.barracks_production.min(build.cost())), quantity(build.cost())), GOLD_TEXT)]);
            panel.bar((city.barracks_production as f32 / build.cost() as f32).clamp(0.0, 1.0));
        }
        let builds = [BuildUnit::Melee, BuildUnit::Ranged, BuildUnit::Cavalry, BuildUnit::Siege];
        panel.gap(GAP);
        panel.buttons(builds.into_iter().map(|build| ButtonSpec {
            target: Target::BarracksBuild(build), label: format!("TRAIN {}", build.name()),
            hint: format!("{} PROD", quantity(build.cost())),
            state: ButtonState::new(city.barracks_queue.first() == Some(&build), false), armed: false,
        }).collect());
        panel.buttons(vec![ButtonSpec { target: Target::OpenCity, label: "OPEN CITY".into(), hint: "CLICK".into(), state: ButtonState::new(false, false), armed: false }]);
    }

    fn barracks_queue_panel(&self, i: usize, panel: &mut PanelBuilder) {
        let city = &self.cities[i];
        if city.barracks_queue.is_empty() { return; }
        panel.text(SMALL, vec![("BARRACKS QUEUE".into(), LABEL_TEXT)]);
        for (index, build) in city.barracks_queue.iter().copied().enumerate() {
            let prefix = if index == 0 { "▶ " } else { "  " };
            panel.text(SMALL, vec![(format!("{prefix}{} · {} PROD", build.name(), quantity(build.cost())), if index == 0 { GOLD_TEXT } else { TEXT })]);
            panel.compact_buttons(vec![
                ButtonSpec { target: Target::BarracksQueueUp(index), label: "UP".into(), hint: "↑".into(), state: ButtonState::new(false, index == 0), armed: false },
                ButtonSpec { target: Target::BarracksQueueDown(index), label: "DOWN".into(), hint: "↓".into(), state: ButtonState::new(false, index + 1 == city.barracks_queue.len()), armed: false },
                ButtonSpec { target: Target::BarracksQueueRemove(index), label: "REMOVE".into(), hint: "X".into(), state: ButtonState::new(false, false), armed: false },
            ]);
        }
    }

    /// The production queues sit above the city tray so they remain usable
    /// when a city has many queued items.
    fn city_queue_panel(&self, i: usize, panel: &mut PanelBuilder) {
        let city = &self.cities[i];
        if city.queue.is_empty() {
            return;
        }
        if !city.queue.is_empty() {
            panel.text(SMALL, vec![("CITY QUEUE · BACKSPACE CANCEL · PGDN PROMOTE".into(), LABEL_TEXT)]);
            for (index, build) in city.queue.iter().copied().enumerate() {
                let prefix = if index == 0 { "▶ " } else { "  " };
                panel.text(SMALL, vec![(format!("{prefix}{} · {} PROD", build.name(), quantity(build.cost())), if index == 0 { GOLD_TEXT } else { TEXT })]);
                panel.compact_buttons(vec![
                    ButtonSpec { target: Target::CityQueueUp(index), label: "UP".into(), hint: "↑".into(), state: ButtonState::new(false, index == 0), armed: false },
                    ButtonSpec { target: Target::CityQueueDown(index), label: "DOWN".into(), hint: "↓".into(), state: ButtonState::new(false, index + 1 == city.queue.len()), armed: false },
                    ButtonSpec { target: Target::CityQueueRemove(index), label: "REMOVE".into(), hint: "X".into(), state: ButtonState::new(false, false), armed: false },
                ]);
            }
        }
    }

    /// Everything about a map hex: terrain, what it yields, and what's on it.
    fn tile_tooltip_lines(&self, hex: Hex) -> Vec<(u32, Line)> {
        if !self.is_explored(hex) {
            return vec![(BODY, vec![("UNEXPLORED".into(), DIM_TEXT)])];
        }
        let fog = self.fog();
        // Out of sight, other sides' cities and everything else that can
        // change are described as last seen.
        let seen_now = fog.sees(hex);
        let memory = if seen_now {
            None
        } else {
            self.remembered(hex)
        };
        let tile = self.grid.tile(hex);
        let terrain = tile.terrain;
        let city = self
            .cities
            .iter()
            .find(|c| c.pos == hex && (seen_now || c.team == super::PLAYER_TEAM));
        let barracks = self
            .cities
            .iter()
            .find(|c| c.barracks == Some(hex) && (seen_now || c.team == super::PLAYER_TEAM));
        let seen_city = memory.and_then(|m| m.city.filter(|c| c.team != super::PLAYER_TEAM));
        let seen_barracks = memory.and_then(|m| m.barracks.filter(|b| b.team != super::PLAYER_TEAM));
        let title = match (city, barracks, seen_city, seen_barracks) {
            (Some(city), ..) => (
                format!("{:?} CITY {}", city.team, city.id + 1).to_uppercase(),
                city.team.color(),
            ),
            (None, Some(city), ..) => (
                format!("{:?} BARRACKS", city.team).to_uppercase(),
                city.team.color(),
            ),
            (None, None, Some(seen), _) => (
                format!("{:?} CITY {}", seen.team, seen.id + 1).to_uppercase(),
                seen.team.color(),
            ),
            (None, None, None, Some(seen)) => (
                format!("{:?} BARRACKS", seen.team).to_uppercase(),
                seen.team.color(),
            ),
            (None, None, None, None) => (tile.name(), TEXT),
        };
        let mut lines = vec![(BODY, vec![title])];

        if !terrain.is_workable() {
            lines.push((SMALL, vec![("IMPASSABLE".into(), DIM_TEXT)]));
            return lines;
        }
        let (food, production) = self.raw_yield(hex);
        lines.push((
            SMALL,
            stat_spans(&[
                ("FOOD", food.to_string(), BOOSTED_TEXT),
                ("PRODUCTION", production.to_string(), GOLD_TEXT),
            ]),
        ));
        if let Some(city) = city {
            let city_index = self.cities.iter().position(|c| c.pos == hex).unwrap();
            let (growth, _, _) = self.growth_status(city_index);
            let (_, production_per_turn) = self.income(city_index);
            let queue = city.queue.first().map_or("NOTHING", |build| build.name());
            lines.push((SMALL, vec![(format!("HP {:.0}/{:.0} · GROWTH {growth}% · +{production_per_turn} PRODUCTION", city.hp, super::city::CITY_MAX_HP), GOLD_TEXT)]));
            lines.push((SMALL, vec![(format!("BUILDING {queue}"), DIM_TEXT)]));
        }
        if let Some(city) = barracks {
            let city_index = self.cities.iter().position(|c| c.barracks == Some(hex)).unwrap();
            let active = city.worked.first() == Some(&hex);
            let production_per_turn = active.then(|| (self.income(city_index).1 - 4).max(0)).unwrap_or(0);
            let queue = city.barracks_queue.first().map_or("NOTHING", |build| build.name());
            lines.push((SMALL, vec![(format!("HP {:.0}/{:.0} · +{production_per_turn} PRODUCTION", city.barracks_hp, super::city::BARRACKS_MAX_HP), GOLD_TEXT)]));
            lines.push((SMALL, vec![(format!("TRAINING {queue}"), DIM_TEXT)]));
        }

        let mut notes = Vec::new();
        if self.grid.has_fresh_water(hex) {
            notes.push("FRESH WATER: +1 FOOD".into());
        }
        if tile.defense_multiplier() != 1.0 {
            let bonus = (tile.defense_multiplier() - 1.0) * 100.0;
            notes.push(format!("+{bonus:.0}% DEFENSE"));
        }
        let (site, road) = match memory {
            Some(seen) => (seen.site, seen.road),
            None => (
                self.sites.get(&hex).map(|s| (s.label, s.team)),
                self.roads.contains(&hex),
            ),
        };
        if let Some((label, team)) = site {
            notes.push(format!("{team:?} {label}").to_uppercase());
        }
        if road {
            notes.push("ROAD: GOODS TRAVEL CHEAPER".into());
        }
        if let Some(worker) = self.cities.iter().find(|c| c.worked.contains(&hex)) {
            notes.push(format!("WORKED BY CITY {}", worker.id + 1));
        }
        if let Some(open) = self.selected_city
            && self.cities[open].pos != hex
        {
            let city = &self.cities[open];
            match self.routes(open).costs.get(&hex) {
                Some(&cost) => notes.push(format!(
                    "{}% REACHES CITY {}",
                    delivered_share(cost) * 25,
                    city.id + 1
                )),
                None => notes.push(format!("OUT OF CITY {}'S REACH", city.id + 1)),
            }
        }
        let describe = |unit: &Unit| format!("{:?} {}", unit.team, self.unit_role(unit).0).to_uppercase();
        let mut units: Vec<String> = self
            .units_at(hex)
            .filter(|&i| fog.shows(&self.units[i]))
            .map(|i| describe(&self.units[i]))
            .collect();
        if let Some(seen) = memory {
            units.extend(seen.units.iter().map(|(unit, _)| describe(unit)));
        }
        if !units.is_empty() {
            notes.push(units.join(", "));
        }
        if self.is_contested(hex) && seen_now {
            notes.push("CONTESTED".into());
        }
        lines.extend(
            notes
                .into_iter()
                .map(|note| (SMALL, vec![(note, DIM_TEXT)])),
        );
        lines
    }

    /// The tile tooltip, just below and right of the cursor (`point`, in UI
    /// pixels), kept inside the window.
    fn draw_tile_tooltip(&self, hex: Hex, point: Vec2, screen_size: Vec2, out: &mut Vec<Vertex>) {
        let mut panel = PanelBuilder::default();
        for (px, line) in self.tile_tooltip_lines(hex) {
            panel.text(px, line);
        }
        let size = panel.size();
        let top_left = point + TILE_TOOLTIP_OFFSET;
        let top_left = Vec2::new(
            top_left.x.min(screen_size.x - MARGIN - size.x),
            top_left.y.max(MARGIN + size.y),
        );
        let mut layout = Layout::default();
        panel.place_top_left(top_left, &mut layout);
        for shape in &layout.shapes {
            draw_shape(shape, out);
        }
    }

    /// A panel explaining what `button` does, above it (or below, for the
    /// top bar), kept inside the window.
    fn draw_tooltip(
        &self,
        button: &Button,
        layout: &Layout,
        screen_size: Vec2,
        out: &mut Vec<Vertex>,
    ) {
        let lines = self.tooltip_lines(button);
        let mut panel = PanelBuilder::default();
        for (px, line) in lines {
            panel.text(px, line);
        }
        let size = panel.size();
        let left = ((button.min.x + button.max.x - size.x) / 2.0)
            .clamp(MARGIN, (screen_size.x - MARGIN - size.x).max(MARGIN));
        // Clear of the panel the button sits in, so the tooltip doesn't hide
        // what the panel says: below panels at the top of the window (the top
        // bar, the debug panel), above those at the bottom (the tray).
        let center = (button.min + button.max) / 2.0;
        let (panel_min, panel_max) = layout
            .panels
            .iter()
            .copied()
            .find(|&(min, max)| contains(min, max, center))
            .unwrap_or((button.min, button.max));
        let near_top = (panel_min.y + panel_max.y) / 2.0 > screen_size.y / 2.0;
        let bottom = if near_top {
            panel_min.y - TOOLTIP_GAP - 2.0 * BORDER - size.y
        } else {
            panel_max.y + TOOLTIP_GAP + 2.0 * BORDER
        };
        let mut layout = Layout::default();
        panel.place_bottom_left(Vec2::new(left, bottom), &mut layout);
        for shape in &layout.shapes {
            draw_shape(shape, out);
        }
    }

    /// A tooltip's title (with the shortcut), description, and why the
    /// button is unavailable, if it is.
    fn tooltip_lines(&self, button: &Button) -> Vec<(u32, Line)> {
        let (title, shortcut, description, unavailable): (String, String, String, Option<String>) =
            match button.target {
                Target::Unit(action) => {
                    // A group's buttons are described for its first member.
                    let Some(idx) = self.selected.or(self.group.first().copied()) else {
                        return Vec::new();
                    };
                    let (title, shortcut, description, unavailable) =
                        self.unit_action_text(action, idx);
                    (title, shortcut.into(), description, unavailable)
                }
                Target::Build(build) => (
                    build.name().into(),
                    build.shortcut().to_string(),
                    format!("{}. {} PRODUCTION.", build.description(), quantity(build.cost())),
                    None,
                ),
                Target::Building(building) => (
                    building.name().into(),
                    building.shortcut().to_string(),
                    format!("{} {} PRODUCTION.", building.description(), quantity(building.cost())),
                    None,
                ),
                Target::ChangeBarracksSite => ("CHANGE SITE".into(), "CLICK".into(), String::new(), None),
                Target::BarracksBuild(build) => (
                    format!("TRAIN {}", build.name()),
                    "BARRACKS".into(),
                    format!("{}. {} PRODUCTION.", build.description(), quantity(build.cost())),
                    None,
                ),
                Target::OpenBarracks => ("OPEN BARRACKS".into(), "CLICK".into(), String::new(), None),
                Target::OpenCity => ("OPEN CITY".into(), "CLICK".into(), String::new(), None),
                Target::CityQueueUp(_) | Target::BarracksQueueUp(_) => ("MOVE UP".into(), "CLICK".into(), String::new(), None),
                Target::CityQueueDown(_) | Target::BarracksQueueDown(_) => ("MOVE DOWN".into(), "CLICK".into(), String::new(), None),
                Target::CityQueueRemove(_) | Target::BarracksQueueRemove(_) => (
                    "REMOVE".into(),
                    "CLICK".into(),
                    "REMOVING THE ACTIVE ITEM LOSES ITS PRODUCTION.".into(),
                    None,
                ),
                Target::Focus(focus) => (
                    format!("{} FOCUS", focus.name()),
                    "AUTO".into(),
                    "REASSIGNS CITIZENS TO FAVOR IT.".into(),
                    None,
                ),
                Target::ConfirmBarracks => ("CONFIRM SITE".into(), "CLICK".into(), String::new(), None),
                Target::Scenario(scenario) => (
                    scenario.name().into(),
                    scenario.key().into(),
                    match scenario {
                        Scenario::Combat => "FOUR UNITS A SIDE ACROSS A MOUNTAIN PASS.",
                        Scenario::Cities => "TWO ESTABLISHED CITIES WITH ARMIES.",
                        Scenario::Frontier => "A SETTLER, WORKER AND SCOUT EACH. BOTH SCOUTS ARE YOURS.",
                        Scenario::World => "A NEW RANDOM CONTINENT EVERY PRESS.",
                    }
                    .into(),
                    None,
                ),
                Target::SaveState => (
                    "SAVE".into(),
                    "F6".into(),
                    "SNAPSHOTS THE WHOLE GAME UNTIL IT CLOSES.".into(),
                    self.is_resolving()
                        .then(|| "NOT WHILE A TURN PLAYS OUT".to_string()),
                ),
                Target::LoadState => (
                    "LOAD".into(),
                    "F7".into(),
                    self.saved_summary().unwrap_or_default(),
                    self.savestate
                        .is_none()
                        .then(|| "NOTHING SAVED YET".to_string()),
                ),
                Target::TogglePlayback => (
                    "PLAYBACK".into(),
                    "F8".into(),
                    "STEP BY STEP OR ALL AT ONCE. THE OUTCOME IS THE SAME.".into(),
                    None,
                ),
                Target::ToggleFog => ("FOG OF WAR".into(), "F9".into(), String::new(), None),
                Target::ToggleYields => (
                    "YIELDS".into(),
                    "Y".into(),
                    "TILE YIELDS AROUND THIS CITY, AND THE SHARE THAT REACHES IT.".into(),
                    None,
                ),
                Target::EndTurn => (
                    "END TURN".into(),
                    "SPACE".into(),
                    if pending_text(self.pending()).is_some() {
                        "SELECTS WHAT STILL NEEDS ORDERS.".into()
                    } else {
                        "RESOLVES EVERYONE'S ORDERS.".into()
                    },
                    None,
                ),
            };

        let mut lines = vec![(
            BODY,
            vec![(title, TEXT), (format!("   {shortcut}"), LABEL_TEXT)],
        )];
        lines.extend(
            wrap(&description, TOOLTIP_WRAP)
                .into_iter()
                .map(|line| (SMALL, vec![(line, DIM_TEXT)])),
        );
        if let Some(reason) = unavailable {
            lines.push((SMALL, vec![(reason, REDUCED_TEXT)]));
        }
        lines
    }

    fn unit_action_text(
        &self,
        action: UnitAction,
        idx: usize,
    ) -> (String, &'static str, String, Option<String>) {
        let unit = &self.units[idx];
        let can_move = unit.stats().move_range > 0;
        let locked = self.rival_of(idx).is_some();
        let cannot_move = (!can_move).then(|| "CANNOT MOVE THIS TURN".to_string());
        match action {
            UnitAction::Move => (
                "MOVE".into(),
                "M",
                "NEXT CLICK ON A GREEN HEX MOVES THERE. CLICK IT AGAIN TO CANCEL.".into(),
                cannot_move,
            ),
            UnitAction::Attack => (
                "ATTACK".into(),
                "X OR SHIFT-CLICK",
                "NEXT CLICK ATTACKS A HEX IN RANGE, HITTING WHOEVER IS THERE WHEN IT LANDS. \
                 RANGE COUNTS FROM WHERE THE UNIT ENDS ITS MOVE."
                    .into(),
                if locked {
                    Some("LOCKED IN A CONTESTED HEX".into())
                } else if !unit.can_attack() {
                    Some("BUSY WITH ITS ABILITY THIS TURN".into())
                } else {
                    None
                },
            ),
            UnitAction::Swap => (
                "SWAP".into(),
                "CTRL-CLICK",
                "NEXT CLICK ON AN ADJACENT ALLY SWAPS PLACES WITH IT.".into(),
                if locked {
                    Some("LOCKED IN A CONTESTED HEX".into())
                } else {
                    cannot_move
                },
            ),
            UnitAction::Ability => {
                let (name, description) = ability_text(unit);
                let description = match unit.ability().cooldown() {
                    0 => format!("{description}."),
                    turns => format!("{description}. COOLDOWN {}.", turns_text(turns)),
                };
                let unavailable = (unit.ability_cooldown > 0)
                    .then(|| format!("READY IN {}", turns_text(unit.ability_cooldown)));
                (name.into(), "Q", description, unavailable)
            }
            UnitAction::Hold => (
                "HOLD".into(),
                "SPACE",
                "SKIPS THIS UNIT FOR THE TURN, KEEPING ANY QUEUED ORDERS.".into(),
                None,
            ),
            UnitAction::Guard => (
                "GUARD".into(),
                "G",
                "SKIPS THIS UNIT EVERY TURN UNTIL IT'S GIVEN AN ORDER.".into(),
                None,
            ),
            UnitAction::Settle => (
                "FOUND CITY".into(),
                "F",
                "AT LEAST 3 HEXES FROM ANY OTHER CITY.".into(),
                None,
            ),
            UnitAction::Road => (
                "BUILD ROAD".into(),
                "R",
                "GOODS TRAVEL MORE CHEAPLY ALONG ROADS.".into(),
                None,
            ),
            UnitAction::Improve => (
                "IMPROVE".into(),
                "I",
                "MINE ON HILLS (+2 PRODUCTION), LUMBER MILL IN FOREST OR JUNGLE (+1), \
                 FARM ELSEWHERE (+2 FOOD)."
                    .into(),
                None,
            ),
        }
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

/// A button before it's placed.
struct ButtonSpec {
    target: Target,
    label: String,
    hint: String,
    state: ButtonState,
    armed: bool,
}

impl ButtonSpec {
    fn plain(action: UnitAction, label: &str, hint: &str) -> Self {
        Self {
            target: Target::Unit(action),
            label: label.into(),
            hint: hint.into(),
            state: ButtonState::Ready,
            armed: false,
        }
    }
}

enum Row {
    Text(u32, Line),
    Gap(f32),
    Bar(f32),
    /// Buttons of equal width; compact ones are one line, label then hint.
    Buttons(Vec<ButtonSpec>, bool),
}

/// Stacks rows top to bottom in a panel sized to fit them.
#[derive(Default)]
struct PanelBuilder {
    rows: Vec<Row>,
    /// See-through, for the debug panel.
    faded: bool,
}

impl PanelBuilder {
    fn text(&mut self, px: u32, line: Line) {
        self.rows.push(Row::Text(px, line));
    }

    fn gap(&mut self, height: f32) {
        self.rows.push(Row::Gap(height));
    }

    /// A progress bar across the panel, `fraction` full.
    fn bar(&mut self, fraction: f32) {
        self.rows.push(Row::Bar(fraction));
    }

    /// A row of equally wide buttons.
    fn buttons(&mut self, buttons: Vec<ButtonSpec>) {
        self.space_button_rows();
        self.rows.push(Row::Buttons(buttons, false));
    }

    /// A row of equally wide one-line buttons.
    fn compact_buttons(&mut self, buttons: Vec<ButtonSpec>) {
        self.space_button_rows();
        self.rows.push(Row::Buttons(buttons, true));
    }

    /// Button borders are drawn just outside their rows, so a row of buttons
    /// right under another needs a gap to keep them from overlapping.
    fn space_button_rows(&mut self) {
        if matches!(self.rows.last(), Some(Row::Buttons(..))) {
            self.rows.push(Row::Gap(GAP));
        }
    }

    fn row_height(row: &Row) -> f32 {
        match row {
            Row::Text(px, _) => font::ui(*px).line_height + LINE_GAP,
            Row::Gap(height) => *height,
            Row::Bar(_) => GROWTH_BAR_HEIGHT,
            Row::Buttons(_, false) => BUTTON_HEIGHT,
            Row::Buttons(_, true) => END_TURN_HEIGHT,
        }
    }

    fn row_width(row: &Row) -> f32 {
        match row {
            Row::Text(px, line) => line_width(font::ui(*px), line),
            Row::Gap(_) | Row::Bar(_) => 0.0,
            Row::Buttons(buttons, compact) => {
                let width = button_width(buttons, *compact);
                buttons.len() as f32 * width + (buttons.len().saturating_sub(1)) as f32 * GAP
            }
        }
    }

    /// The panel's size, padding included.
    fn size(&self) -> Vec2 {
        let width = self.rows.iter().map(Self::row_width).fold(0.0, f32::max);
        let height: f32 = self.rows.iter().map(Self::row_height).sum();
        (Vec2::new(width, height) + 2.0 * PADDING).round()
    }

    fn place_bottom_left(self, min: Vec2, layout: &mut Layout) {
        let top = min.y + self.size().y;
        self.place_top_left(Vec2::new(min.x, top), layout);
    }

    fn place_top_left(self, top_left: Vec2, layout: &mut Layout) {
        let size = self.size();
        let min = Vec2::new(top_left.x, top_left.y - size.y).round();
        let max = min + size;
        layout.panel(min, max, self.faded);

        let left = min.x + PADDING;
        let inner_width = size.x - 2.0 * PADDING;
        let mut top = max.y - PADDING;
        for row in self.rows {
            let height = Self::row_height(&row);
            match row {
                Row::Text(px, line) => {
                    // Center capital letters in the line, ignoring the gap.
                    let middle = top - (height - LINE_GAP) / 2.0;
                    push_text_row(layout, Vec2::new(left, middle), px, line);
                }
                Row::Gap(_) => {}
                Row::Bar(fraction) => layout.shapes.push(Shape::Bar {
                    min: Vec2::new(left, top - height),
                    max: Vec2::new(left + inner_width, top),
                    fraction,
                }),
                Row::Buttons(buttons, compact) => {
                    let width = button_width(&buttons, compact);
                    for (i, spec) in buttons.into_iter().enumerate() {
                        let min = Vec2::new(left + i as f32 * (width + GAP), top - height);
                        layout.buttons.push(Button {
                            target: spec.target,
                            label: spec.label,
                            hint: spec.hint,
                            state: spec.state,
                            armed: spec.armed,
                            faded: self.faded,
                            min: min.round(),
                            max: (min + Vec2::new(width, height)).round(),
                        });
                    }
                }
            }
            top -= height;
        }
    }
}

/// Width every button in a row shares: enough for the widest one.
fn button_width(buttons: &[ButtonSpec], compact: bool) -> f32 {
    buttons
        .iter()
        .map(|b| {
            if compact {
                return single_line_button_width(&b.label, &b.hint);
            }
            let label = font::ui(BODY).width(&b.label);
            let hint = font::ui(SMALL).width(&b.hint);
            label.max(hint) + 2.0 * BUTTON_PADDING
        })
        .fold(if compact { 0.0 } else { BUTTON_MIN_WIDTH }, f32::max)
        .ceil()
}

fn single_line_button_width(label: &str, hint: &str) -> f32 {
    (font::ui(BODY).width(label) + GAP + font::ui(SMALL).width(hint) + 2.0 * BUTTON_PADDING).ceil()
}

/// Adds a line of text with its capital letters centered vertically on
/// `middle`, starting at `left`.
fn push_text_row(layout: &mut Layout, left_middle: Vec2, px: u32, line: Line) {
    let face = font::ui(px);
    let origin = Vec2::new(left_middle.x, left_middle.y - face.cap_height / 2.0);
    layout.shapes.push(Shape::Text { origin, px, line });
}

fn line_width(face: &Face, line: &Line) -> f32 {
    line.iter().map(|(span, _)| face.width(span)).sum()
}

fn contains(min: Vec2, max: Vec2, point: Vec2) -> bool {
    point.cmpge(min).all() && point.cmple(max).all()
}

/// Window pixels (origin top-left, Y down) to UI pixels (origin bottom-left, Y up).
fn to_ui(cursor: Vec2, screen_size: Vec2) -> Vec2 {
    Vec2::new(cursor.x, screen_size.y - cursor.y)
}

fn draw_shape(shape: &Shape, out: &mut Vec<Vertex>) {
    match shape {
        Shape::Panel { min, max, faded } => {
            let (bg, border) = (fade(PANEL_BG, *faded), fade(BORDER_COLOR, *faded));
            draw_box(*min, *max, bg, BORDER, border, out)
        }
        Shape::Text { origin, px, line } => {
            let face = font::ui(*px);
            let mut pen = origin.x;
            for (span, color) in line {
                face.push(Vec2::new(pen, origin.y), span, *color, out);
                pen += face.width(span);
            }
        }
        Shape::Bar { min, max, fraction } => {
            mesh::quad(*min, *max, BAR_BG, out);
            let filled = Vec2::new(min.x + (max.x - min.x) * fraction.clamp(0.0, 1.0), max.y);
            mesh::quad(*min, filled, GROWTH_COLOR, out);
        }
    }
}

fn draw_button(button: &Button, hovered: bool, out: &mut Vec<Vertex>) {
    let (bg, text_color, hint_color) = match (button.state, hovered) {
        (ButtonState::Ready, false) => (BUTTON_BG, TEXT, DIM_TEXT),
        (ButtonState::Ready, true) => (BUTTON_HOVER_BG, TEXT, DIM_TEXT),
        (ButtonState::Queued, false) => (QUEUED_BG, QUEUED_TEXT, QUEUED_HINT_TEXT),
        (ButtonState::Queued, true) => (QUEUED_HOVER_BG, QUEUED_TEXT, QUEUED_HINT_TEXT),
        (ButtonState::Disabled, _) => (DISABLED_BG, DISABLED_TEXT, DISABLED_TEXT),
    };
    let (border, border_color) = if button.armed {
        (ARMED_BORDER, ARMED_BORDER_COLOR)
    } else {
        (BORDER, BORDER_COLOR)
    };
    let [bg, border_color, text_color, hint_color] =
        [bg, border_color, text_color, hint_color].map(|color| fade(color, button.faded));
    draw_box(button.min, button.max, bg, border, border_color, out);

    let (label_face, hint_face) = (font::ui(BODY), font::ui(SMALL));
    let center = (button.min + button.max) / 2.0;
    let height = button.max.y - button.min.y;
    if height < BUTTON_HEIGHT {
        // One line: the label, then the hint beside it.
        let width = label_face.width(&button.label) + GAP + hint_face.width(&button.hint);
        let left = center.x - width / 2.0;
        let baseline = center.y - label_face.cap_height / 2.0;
        label_face.push(Vec2::new(left, baseline), &button.label, text_color, out);
        let hint_left = left + label_face.width(&button.label) + GAP;
        hint_face.push(
            Vec2::new(hint_left, baseline),
            &button.hint,
            hint_color,
            out,
        );
    } else {
        // Two lines: the label, with the hint under it.
        let gap = LINE_GAP + 2.0;
        let block = label_face.cap_height + gap + hint_face.cap_height;
        let label_baseline = center.y + block / 2.0 - label_face.cap_height;
        let hint_baseline = center.y - block / 2.0;
        let label_left = center.x - label_face.width(&button.label) / 2.0;
        let hint_left = center.x - hint_face.width(&button.hint) / 2.0;
        label_face.push(
            Vec2::new(label_left, label_baseline),
            &button.label,
            text_color,
            out,
        );
        hint_face.push(
            Vec2::new(hint_left, hint_baseline),
            &button.hint,
            hint_color,
            out,
        );
    }
}

/// `color` made see-through for the debug panel, or unchanged.
fn fade(color: Color, faded: bool) -> Color {
    let [r, g, b, a] = color;
    if faded {
        [r, g, b, a * DEBUG_ALPHA]
    } else {
        color
    }
}

/// A filled box with a border drawn around the outside of `min`..`max`.
fn draw_box(
    min: Vec2,
    max: Vec2,
    bg: Color,
    border: f32,
    border_color: Color,
    out: &mut Vec<Vertex>,
) {
    mesh::quad(min - border, max + border, border_color, out);
    mesh::quad(min, max, bg, out);
}

/// "LABEL value" pairs on one line, labels dim and values in their own color.
fn stat_spans(stats: &[(&str, String, Color)]) -> Line {
    let mut line = Vec::new();
    for (i, (label, value, color)) in stats.iter().enumerate() {
        let separator = if i == 0 { "" } else { "   " };
        line.push((format!("{separator}{label} "), LABEL_TEXT));
        line.push((value.clone(), *color));
    }
    line
}

/// Green if a stat is above its base value, red if below.
fn compare(value: f32, base: f32) -> Color {
    if value > base {
        BOOSTED_TEXT
    } else if value < base {
        REDUCED_TEXT
    } else {
        TEXT
    }
}

/// A city amount in quarter units, without decimals when it's whole.
fn quantity(quarters: i32) -> String {
    if quarters % 4 == 0 {
        (quarters / 4).to_string()
    } else {
        format!("{:.2}", quarters as f32 / 4.0)
    }
}

fn signed_quantity(quarters: i32) -> String {
    let sign = if quarters >= 0 { "+" } else { "" };
    format!("{sign}{}", quantity(quarters))
}

/// The End Turn button's label: the next thing the turn is waiting on (units
/// first, as that's what clicking selects first), or "END TURN" once nothing is.
fn end_turn_label((units, cities): (usize, usize)) -> String {
    match (units, cities) {
        (1, _) => "UNIT NEEDS ORDERS".into(),
        (0, 0) => "END TURN".into(),
        (0, 1) => "CHOOSE PRODUCTION".into(),
        (0, cities) => format!("{cities} CITIES NEED PRODUCTION"),
        (units, _) => format!("{units} UNITS NEED ORDERS"),
    }
}

/// What the turn is waiting on, like "2 UNITS AND 1 CITY NEED ORDERS", from
/// `GameState::pending`; `None` once nothing is.
fn pending_text((units, cities): (usize, usize)) -> Option<String> {
    let plural = |count: usize, one: &str, many: &str| {
        let word = if count == 1 { one } else { many };
        format!("{count} {word}")
    };
    let mut parts = Vec::new();
    if units > 0 {
        parts.push(plural(units, "UNIT", "UNITS"));
    }
    if cities > 0 {
        parts.push(plural(cities, "CITY", "CITIES"));
    }
    let verb = if units + cities == 1 { "NEEDS" } else { "NEED" };
    (!parts.is_empty()).then(|| format!("{} {verb} ORDERS", parts.join(" AND ")))
}

fn turns_text(turns: u32) -> String {
    match turns {
        1 => "1 TURN".to_string(),
        _ => format!("{turns} TURNS"),
    }
}

/// Splits `text` into lines of at most `max_chars`, breaking between words.
fn wrap(text: &str, max_chars: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        if !line.is_empty() && line.len() + 1 + word.len() > max_chars {
            lines.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

/// The ability's name and a short description of what it does.
fn ability_text(unit: &Unit) -> (&'static str, &'static str) {
    match unit.ability() {
        Ability::ShieldWall => ("SHIELD WALL", "+50% DEFENSE THIS TURN, NO MOVING"),
        Ability::Volley => ("VOLLEY", "ALSO HITS ENEMIES NEXT TO THE TARGET, ALL AT 60%"),
        Ability::Charge => ("CHARGE", "+1 MOVE AND +50% ATTACK THIS TURN"),
        Ability::Deploy if unit.deployed => ("PACK UP", "A TURN PACKING UP, THEN IT CAN MOVE"),
        Ability::Deploy => ("DEPLOY", "A TURN SETTING UP, THEN +1 RANGE BUT NO MOVING"),
        Ability::Lookout => ("LOOKOUT", "NO MOVING THIS TURN, +2 SIGHT NEXT TURN"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::game::unit::{Team, UnitType};

    const SCREEN: Vec2 = Vec2::new(1600.0, 900.0);

    #[test]
    fn hovering_a_button_shows_its_tooltip() {
        let mut game = GameState::city_scenario();
        game.select_city();
        let card = button_cursor(&game, Target::Build(BuildUnit::Siege));
        let plain = game.build_ui(SCREEN, None).len();
        assert!(game.build_ui(SCREEN, Some(card)).len() > plain);
    }

    #[test]
    fn hovering_an_enemy_describes_it() {
        let mut game = GameState::new();
        game.fog_of_war = false;
        let enemy = game.units.iter().find(|u| u.team == Team::Red).unwrap();
        let empty = hex_cursor(&game, Hex::new(0, 1));
        let plain = game.build_ui(SCREEN, Some(empty)).len();
        let hovered = game.build_ui(SCREEN, Some(hex_cursor(&game, enemy.pos)));
        assert!(hovered.len() > plain);
    }

    #[test]
    fn hovering_an_enemy_out_of_sight_shows_nothing() {
        let game = GameState::new();
        let enemy = game.units.iter().find(|u| u.team == Team::Red).unwrap();
        assert!(!game.fog().sees(enemy.pos), "the combat map starts with Red unseen");
        let empty = hex_cursor(&game, Hex::new(0, 1));
        let plain = game.build_ui(SCREEN, Some(empty)).len();
        let hovered = game.build_ui(SCREEN, Some(hex_cursor(&game, enemy.pos)));
        assert_eq!(hovered.len(), plain);
    }

    #[test]
    fn ui_projection_puts_the_origin_at_the_bottom_left() {
        let clip = |p: Vec2| {
            ui_projection(SCREEN)
                .project_point3(p.extend(0.0))
                .truncate()
        };
        // Vulkan clip space has Y = +1 at the bottom of the window.
        assert!(clip(Vec2::ZERO).abs_diff_eq(Vec2::new(-1.0, 1.0), 1e-5));
        assert!(clip(SCREEN).abs_diff_eq(Vec2::new(1.0, -1.0), 1e-5));
    }

    /// Where to click, in window pixels, to press the button for `target`.
    fn button_cursor(game: &GameState, target: Target) -> Vec2 {
        let layout = game.layout(SCREEN);
        let button = layout
            .buttons
            .iter()
            .find(|b| b.target == target)
            .expect("button shown");
        to_ui((button.min + button.max) / 2.0, SCREEN)
    }

    /// Where `hex` is drawn, in window pixels.
    fn hex_cursor(game: &GameState, hex: Hex) -> Vec2 {
        let camera = &game.camera;
        let offset = (hex.to_world() - camera.center) / camera.half_height;
        let ndc = Vec2::new(offset.x * SCREEN.y / SCREEN.x, -offset.y);
        (ndc + 1.0) / 2.0 * SCREEN
    }

    fn find(game: &GameState, unit_type: UnitType) -> usize {
        game.units
            .iter()
            .position(|u| u.team == Team::Blue && u.unit_type == unit_type)
            .unwrap()
    }

    #[test]
    fn attack_button_arms_a_square_attack_for_the_next_click() {
        let mut game = GameState::new();
        let melee = find(&game, UnitType::Melee);
        assert_eq!(game.selected, Some(melee));
        // An empty hex the melee could otherwise move to.
        let target = Hex::new(-1, 0);

        let attack = Target::Unit(UnitAction::Attack);
        game.handle_click(button_cursor(&game, attack), SCREEN, ClickMode::Normal);
        assert_eq!(game.ui_click_mode, Some(ClickMode::Attack));
        game.handle_click(hex_cursor(&game, target), SCREEN, ClickMode::Normal);
        assert_eq!(game.units[melee].planned_attack, Some(target));
        assert_eq!(game.units[melee].planned_move, None);
        assert_eq!(game.ui_click_mode, None, "armed for one click only");
    }

    #[test]
    fn pressing_an_armed_button_again_disarms_it() {
        let mut game = GameState::new();
        let swap = Target::Unit(UnitAction::Swap);
        game.handle_click(button_cursor(&game, swap), SCREEN, ClickMode::Normal);
        assert_eq!(game.ui_click_mode, Some(ClickMode::Swap));
        game.handle_click(button_cursor(&game, swap), SCREEN, ClickMode::Normal);
        assert_eq!(game.ui_click_mode, None);
    }

    #[test]
    fn hold_button_holds_the_selected_unit() {
        let mut game = GameState::new();
        let first = game.selected.unwrap();
        let hold = Target::Unit(UnitAction::Hold);
        game.handle_click(button_cursor(&game, hold), SCREEN, ClickMode::Normal);
        assert!(game.units[first].holding);
        assert_ne!(game.selected, Some(first));
    }

    #[test]
    fn debug_panel_saves_loads_and_switches_scenarios() {
        let mut game = GameState::new();
        let unit = game.selected.unwrap();
        let start = game.units[unit].pos;
        let load = Target::LoadState;
        let load_state = |game: &GameState| {
            let layout = game.layout(SCREEN);
            layout
                .buttons
                .iter()
                .find(|b| b.target == load)
                .unwrap()
                .state
        };
        assert_eq!(
            load_state(&game),
            ButtonState::Disabled,
            "nothing saved yet"
        );

        game.handle_click(
            button_cursor(&game, Target::SaveState),
            SCREEN,
            ClickMode::Normal,
        );
        assert_eq!(load_state(&game), ButtonState::Ready);
        game.units[unit].pos = Hex::new(0, 1);
        game.handle_click(button_cursor(&game, load), SCREEN, ClickMode::Normal);
        assert_eq!(game.units[unit].pos, start);

        let cities = Target::Scenario(Scenario::Cities);
        game.handle_click(button_cursor(&game, cities), SCREEN, ClickMode::Normal);
        assert_eq!(game.scenario, Scenario::Cities);
    }

    #[test]
    fn build_card_queues_its_unit() {
        let mut game = GameState::city_scenario();
        game.select_city();
        let cavalry = Target::Build(BuildUnit::Cavalry);
        game.handle_click(button_cursor(&game, cavalry), SCREEN, ClickMode::Normal);
        let city = game.selected_city.unwrap();
        assert_eq!(game.cities[city].queue, vec![Build::Unit(BuildUnit::Cavalry)]);
    }

    /// The city scenario with the player's city open and the camera settled on it.
    fn city_view() -> GameState {
        let mut game = GameState::city_scenario();
        game.select_city();
        game.update(10.0);
        game
    }

    #[test]
    fn barracks_map_click_locks_site_and_exits_placement() {
        let mut game = city_view();
        game.units.clear();
        let city = game.selected_city.unwrap();
        let button = button_cursor(&game, Target::Building(Building::Barracks));
        game.handle_click(button, SCREEN, ClickMode::Normal);
        assert_eq!(game.placing_barracks, Some(city));

        // Use visible map tiles so this exercises UI hit testing as well as
        // city placement, rather than calling city_click directly.
        let sites: Vec<_> = (-8..=8).flat_map(|q| (-8..=8).map(move |r| Hex::new(q, r)))
            .filter(|&hex| {
                let cursor = hex_cursor(&game, hex);
                game.grid.is_passable(hex)
                    && !game.cities.iter().any(|c| c.pos == hex)
                    && (0.0..SCREEN.x).contains(&cursor.x)
                    && (0.0..SCREEN.y).contains(&cursor.y)
                    && !game.layout(SCREEN).covers(to_ui(cursor, SCREEN))
            }).take(2).collect();
        assert_eq!(sites.len(), 2);
        game.handle_click(hex_cursor(&game, sites[0]), SCREEN, ClickMode::Normal);
        assert_eq!(game.placing_barracks, None);
        assert_eq!(game.cities[city].planned_barracks, Some(sites[0]));
        game.update_hover(Some(hex_cursor(&game, sites[1])), SCREEN, 0.1);
        game.handle_click(hex_cursor(&game, sites[1]), SCREEN, ClickMode::Normal);
        assert_eq!(game.cities[city].planned_barracks, Some(sites[0]));
        assert_eq!(game.placing_barracks, None);
    }

    #[test]
    fn clicking_a_unit_leaves_the_city_view() {
        let mut game = city_view();
        // One of the player's units drawn clear of the top bar and the tray.
        let unit = (0..game.units.len())
            .find(|&i| {
                let cursor = hex_cursor(&game, game.units[i].pos);
                game.units[i].team == Team::Blue
                    && (100.0..500.0).contains(&cursor.y)
                    && (0.0..SCREEN.x).contains(&cursor.x)
            })
            .expect("a unit in view");
        game.handle_click(
            hex_cursor(&game, game.units[unit].pos),
            SCREEN,
            ClickMode::Normal,
        );
        assert_eq!(game.selected_city, None);
        assert_eq!(game.selected, Some(unit));
    }

    #[test]
    fn clicking_the_open_city_again_closes_it() {
        let mut game = city_view();
        let city = game.cities[game.selected_city.unwrap()].pos;
        game.handle_click(hex_cursor(&game, city), SCREEN, ClickMode::Normal);
        assert_eq!(game.selected_city, None);
    }

    #[test]
    fn yields_show_only_for_the_open_city_and_toggle() {
        let mut game = city_view();
        let shown = game.build_vertices().len();
        game.handle_click(
            button_cursor(&game, Target::ToggleYields),
            SCREEN,
            ClickMode::Normal,
        );
        assert!(!game.show_yields);
        assert!(game.build_vertices().len() < shown, "badges hidden");
        game.toggle_yields();
        assert_eq!(game.build_vertices().len(), shown);

        // Hovering a city without opening it no longer shows its yields.
        let city = game.cities[game.selected_city.unwrap()].pos;
        game.close_city();
        game.update_hover(Some(hex_cursor(&game, city)), SCREEN, 0.0);
        assert!(game.hovered_city.is_some());
        assert_eq!(game.yields_city(), None);
        assert!(game.build_vertices().len() < shown);
    }

    #[test]
    fn resting_on_a_tile_shows_its_tooltip_after_a_delay() {
        let mut game = GameState::new();
        let cursor = hex_cursor(&game, Hex::new(0, 1));
        game.update_hover(Some(cursor), SCREEN, 0.0);
        let before = game.build_ui(SCREEN, Some(cursor)).len();
        game.update_hover(Some(cursor), SCREEN, TILE_TOOLTIP_DELAY);
        assert_eq!(game.hovered_tile, Some(Hex::new(0, 1)));
        assert!(game.build_ui(SCREEN, Some(cursor)).len() > before);

        // Moving to another hex starts the wait over.
        let elsewhere = hex_cursor(&game, Hex::new(1, 1));
        game.update_hover(Some(elsewhere), SCREEN, 0.1);
        assert_eq!(game.hover_seconds, 0.0);
    }

    #[test]
    fn clicks_on_a_panel_do_not_reach_the_map() {
        let mut game = GameState::new();
        let selected = game.selected;
        // The top bar's left end, away from any button.
        game.handle_click(Vec2::new(4.0, 4.0), SCREEN, ClickMode::Normal);
        assert_eq!(game.selected, selected);
    }

    #[test]
    fn end_turn_button_names_what_is_waiting() {
        assert_eq!(end_turn_label((3, 1)), "3 UNITS NEED ORDERS");
        assert_eq!(end_turn_label((1, 1)), "UNIT NEEDS ORDERS");
        assert_eq!(end_turn_label((0, 1)), "CHOOSE PRODUCTION");
        assert_eq!(end_turn_label((0, 2)), "2 CITIES NEED PRODUCTION");
        assert_eq!(end_turn_label((0, 0)), "END TURN");

        let game = GameState::new();
        let layout = game.layout(SCREEN);
        let button = layout.buttons.iter().find(|b| b.target == Target::EndTurn);
        assert_eq!(button.unwrap().label, "4 UNITS NEED ORDERS");
    }

    #[test]
    fn wrap_breaks_between_words() {
        assert_eq!(wrap("AB CD EF", 5), vec!["AB CD", "EF"]);
        assert_eq!(wrap("ABCDEFG HI", 5), vec!["ABCDEFG", "HI"]);
    }
}
