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
use super::city::{BuildUnit, delivered_share};
use super::font::{self, Face};
use super::orders::ClickMode;
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

/// Maps UI pixels (origin bottom-left, Y up) to clip space.
pub fn ui_projection(screen_size: Vec2) -> Mat4 {
    Mat4::from_translation(Vec3::new(-1.0, 1.0, 0.0))
        * Mat4::from_scale(Vec3::new(2.0 / screen_size.x, -2.0 / screen_size.y, 1.0))
}

/// Something a button does.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Target {
    Unit(UnitAction),
    Build(BuildUnit),
    EndTurn,
}

/// An order for the selected unit.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum UnitAction {
    Move,
    Attack,
    Swap,
    Ability,
    Hold,
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
    min: Vec2,
    max: Vec2,
}

impl Button {
    fn contains(&self, point: Vec2) -> bool {
        contains(self.min, self.max, point)
    }
}

enum Shape {
    Panel { min: Vec2, max: Vec2 },
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

    fn panel(&mut self, min: Vec2, max: Vec2) {
        self.shapes.push(Shape::Panel { min, max });
        self.panels.push((min, max));
    }
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
        {
            let mut panel = PanelBuilder::default();
            self.unit_info(idx, &mut panel);
            let top_left = Vec2::new(MARGIN, screen_size.y - TOP_BAR_HEIGHT - MARGIN);
            panel.place_top_left(top_left, &mut layout);
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
                UnitAction::Settle => self.found_city_selected(),
                UnitAction::Road => self.build_worker_road_selected(),
                UnitAction::Improve => self.improve_worker_tile_selected(),
            },
            Target::Build(build) => self.queue_selected_city_unit(build),
            Target::EndTurn => self.end_planning(),
        }
        true
    }

    fn layout(&self, screen_size: Vec2) -> Layout {
        let mut layout = Layout::default();
        self.top_bar(screen_size, &mut layout);

        let mut tray = PanelBuilder::default();
        if let Some(city) = self.selected_city {
            self.city_tray(city, &mut tray);
        } else if let Some(idx) = self.selected {
            self.unit_info(idx, &mut tray);
            tray.gap(GAP);
            tray.buttons(self.unit_buttons(idx));
        } else {
            return layout;
        }
        tray.place_bottom_left(Vec2::splat(MARGIN), &mut layout);
        layout
    }

    /// Turn number and status on the left, the latest notice in the middle,
    /// and the End Turn button on the right.
    fn top_bar(&self, size: Vec2, layout: &mut Layout) {
        let min = Vec2::new(0.0, size.y - TOP_BAR_HEIGHT);
        layout.panel(min, size);
        let middle = min.y + TOP_BAR_HEIGHT / 2.0;

        let pending = (0..self.units.len())
            .filter(|&i| self.is_player_controlled(i) && self.needs_orders(i))
            .count();
        let (turn, status) = if self.is_resolving() {
            (self.turn, "RESOLVING ORDERS".to_string())
        } else if pending > 0 {
            let units = if pending == 1 {
                "UNIT NEEDS"
            } else {
                "UNITS NEED"
            };
            (self.turn + 1, format!("{pending} {units} ORDERS"))
        } else {
            (self.turn + 1, "READY TO END THE TURN".to_string())
        };
        let turn_text = format!("TURN {turn}");
        let turn_width = font::ui(TITLE).width(&turn_text);
        let mut left = MARGIN;
        push_text_row(
            layout,
            Vec2::new(left, middle),
            TITLE,
            vec![(turn_text, TEXT)],
        );
        left += turn_width + 2.0 * GAP;
        push_text_row(
            layout,
            Vec2::new(left, middle),
            BODY,
            vec![(status.clone(), DIM_TEXT)],
        );
        let left_end = left + font::ui(BODY).width(&status);

        let label = "END TURN".to_string();
        let hint = "ENTER".to_string();
        let width = single_line_button_width(&label, &hint);
        let button_min = Vec2::new(size.x - MARGIN - width, middle - END_TURN_HEIGHT / 2.0);
        let end_turn = Button {
            target: Target::EndTurn,
            label,
            hint,
            state: if self.is_resolving() {
                ButtonState::Disabled
            } else {
                ButtonState::new(pending == 0, false)
            },
            armed: false,
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
        let terrain = self.grid.terrain(unit.pos);
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
            notes.push(format!("ON {terrain:?}: +{bonus:.0}% DEFENSE").to_uppercase());
        }
        if unit.deployed {
            notes.push("DEPLOYED: +1 RANGE, CANNOT MOVE".to_string());
        }
        if self.rival_of(idx).is_some() {
            notes.push("CONTESTED: FIGHTING FOR THIS HEX".to_string());
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
        buttons
    }

    /// The open city: population, stores and income, growth, what it's
    /// building, and a card for each unit it can build.
    fn city_tray(&self, i: usize, panel: &mut PanelBuilder) {
        let city = &self.cities[i];
        let (food, production) = self.income(i);
        let net_food = food - city.population as i32 * 8;
        let (growth_percent, _, growth_label) = self.growth_status(i);

        panel.text(
            TITLE,
            vec![
                (format!("CITY {}", city.id + 1), city.team.color()),
                (format!("   POPULATION {}", city.population), DIM_TEXT),
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
                format!("{} OF {} WORKING", city.worked.len(), city.population),
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
        let building = match city.queue {
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
                    state: ButtonState::new(city.queue == Some(build), false),
                    armed: false,
                })
                .collect(),
        );
        panel.gap(GAP);
        panel.text(
            SMALL,
            vec![(
                "CLICK TILES TO ASSIGN CITIZENS · A AUTO-ASSIGN · ESC CLOSE".into(),
                LABEL_TEXT,
            )],
        );
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
        // what the panel says: below the top bar, above the tray.
        let center = (button.min + button.max) / 2.0;
        let (panel_min, panel_max) = layout
            .panels
            .iter()
            .copied()
            .find(|&(min, max)| contains(min, max, center))
            .unwrap_or((button.min, button.max));
        let bottom = if button.target == Target::EndTurn {
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
                    let Some(idx) = self.selected else {
                        return Vec::new();
                    };
                    let (title, shortcut, description, unavailable) =
                        self.unit_action_text(action, idx);
                    (title, shortcut.into(), description, unavailable)
                }
                Target::Build(build) => (
                    build.name().into(),
                    build.shortcut().to_string(),
                    format!(
                        "{}. COSTS {} PRODUCTION; THE CITY SPENDS ITS STORED PRODUCTION \
                         AND PLACES THE UNIT NEXT TO ITSELF ONCE THERE'S ENOUGH.",
                        build.description(),
                        quantity(build.cost())
                    ),
                    None,
                ),
                Target::EndTurn => {
                    let pending = (0..self.units.len())
                        .filter(|&i| self.is_player_controlled(i) && self.needs_orders(i))
                        .count();
                    (
                        "END TURN".into(),
                        "ENTER".into(),
                        "RESOLVES EVERYONE'S ORDERS AND COLLECTS CITY INCOME. EVERY UNIT \
                         NEEDS ORDERS FIRST; HOLD ONE (SPACE) TO LEAVE IT AS IT IS."
                            .into(),
                        (pending > 0)
                            .then(|| format!("{pending} STILL NEED ORDERS: CLICKING SELECTS ONE")),
                    )
                }
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
                "THE NEXT CLICK ON A GREEN HEX MOVES THERE. WITHOUT PICKING AN ACTION, \
                 CLICKING A GREEN HEX MOVES AND CLICKING AN ENEMY ATTACKS. CLICK A QUEUED \
                 ORDER AGAIN TO CANCEL IT."
                    .into(),
                cannot_move,
            ),
            UnitAction::Attack => (
                "ATTACK".into(),
                "X OR SHIFT-CLICK",
                "THE NEXT CLICK ATTACKS ANY HEX IN RANGE, OCCUPIED OR NOT: WHOEVER STANDS \
                 THERE WHEN THE ATTACK LANDS IS HIT. RANGE COUNTS FROM WHERE THE UNIT WILL \
                 BE AFTER ITS MOVE."
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
                "THE NEXT CLICK ON AN ADJACENT ALLY SWAPS PLACES WITH IT. BOTH UNITS MUST \
                 BE ABLE TO MOVE."
                    .into(),
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
                    turns => format!("{description}. COOLDOWN: {}.", turns_text(turns)),
                };
                let unavailable = (unit.ability_cooldown > 0)
                    .then(|| format!("READY IN {}", turns_text(unit.ability_cooldown)));
                (name.into(), "Q", description, unavailable)
            }
            UnitAction::Hold => (
                "HOLD".into(),
                "SPACE",
                "KEEP ANY ORDERS ALREADY QUEUED AND SKIP THE REST OF THIS UNIT'S TURN. \
                 CTRL-RIGHT-CLICK CLEARS ITS ORDERS AND THE HOLD."
                    .into(),
                None,
            ),
            UnitAction::Settle => (
                "FOUND CITY".into(),
                "F",
                "TURNS THIS SETTLER INTO A CITY ON ITS HEX. CITIES MUST BE SPACED APART.".into(),
                None,
            ),
            UnitAction::Road => (
                "BUILD ROAD".into(),
                "R",
                "BUILDS A DIRT ROAD ON THIS WORKER'S HEX, LOWERING THE COST OF MOVING \
                 GOODS THROUGH IT."
                    .into(),
                None,
            ),
            UnitAction::Improve => (
                "IMPROVE".into(),
                "I",
                "BUILDS A FARM OR MINE ON THIS WORKER'S HEX, RAISING WHAT IT YIELDS.".into(),
                None,
            ),
        }
    }

    /// The unit drawn under `cursor` (window pixels, origin top-left). In a
    /// contested hex, whichever of the two is nearer the cursor.
    fn unit_at_screen(&self, cursor: Vec2, screen_size: Vec2) -> Option<usize> {
        let hex = self.hex_at_screen(cursor, screen_size)?;
        let point = self.camera.screen_to_world(cursor, screen_size);
        let distance = |idx: usize| self.unit_layout(idx).0.distance(point);
        self.units_at(hex)
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
    Buttons(Vec<ButtonSpec>),
}

/// Stacks rows top to bottom in a panel sized to fit them.
#[derive(Default)]
struct PanelBuilder {
    rows: Vec<Row>,
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
        self.rows.push(Row::Buttons(buttons));
    }

    fn row_height(row: &Row) -> f32 {
        match row {
            Row::Text(px, _) => font::ui(*px).line_height + LINE_GAP,
            Row::Gap(height) => *height,
            Row::Bar(_) => GROWTH_BAR_HEIGHT,
            Row::Buttons(_) => BUTTON_HEIGHT,
        }
    }

    fn row_width(row: &Row) -> f32 {
        match row {
            Row::Text(px, line) => line_width(font::ui(*px), line),
            Row::Gap(_) | Row::Bar(_) => 0.0,
            Row::Buttons(buttons) => {
                let width = button_width(buttons);
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
        layout.panel(min, max);

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
                Row::Buttons(buttons) => {
                    let width = button_width(&buttons);
                    for (i, spec) in buttons.into_iter().enumerate() {
                        let min = Vec2::new(left + i as f32 * (width + GAP), top - height);
                        layout.buttons.push(Button {
                            target: spec.target,
                            label: spec.label,
                            hint: spec.hint,
                            state: spec.state,
                            armed: spec.armed,
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
fn button_width(buttons: &[ButtonSpec]) -> f32 {
    buttons
        .iter()
        .map(|b| {
            let label = font::ui(BODY).width(&b.label);
            let hint = font::ui(SMALL).width(&b.hint);
            label.max(hint) + 2.0 * BUTTON_PADDING
        })
        .fold(BUTTON_MIN_WIDTH, f32::max)
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
        Shape::Panel { min, max } => draw_box(*min, *max, PANEL_BG, BORDER, BORDER_COLOR, out),
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
        Ability::ShieldWall => ("SHIELD WALL", "+50% DEFENSE THIS TURN, BUT CANNOT MOVE"),
        Ability::Volley => (
            "VOLLEY",
            "THE ATTACK ALSO HITS ENEMIES NEXT TO THE TARGET, BUT EVERY HIT DEALS 60%",
        ),
        Ability::Charge => ("CHARGE", "+1 MOVE AND +50% ATTACK THIS TURN"),
        Ability::Deploy if unit.deployed => ("PACK UP", "SPEND A TURN PACKING UP TO MOVE AGAIN"),
        Ability::Deploy => (
            "DEPLOY",
            "SPEND A TURN SETTING UP, THEN +1 RANGE BUT CANNOT MOVE UNTIL PACKED UP",
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::hex::Hex;
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
        let game = GameState::new();
        let enemy = game.units.iter().find(|u| u.team == Team::Red).unwrap();
        let empty = hex_cursor(&game, Hex::new(0, 1));
        let plain = game.build_ui(SCREEN, Some(empty)).len();
        let hovered = game.build_ui(SCREEN, Some(hex_cursor(&game, enemy.pos)));
        assert!(hovered.len() > plain);
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
    fn build_card_queues_its_unit() {
        let mut game = GameState::city_scenario();
        game.select_city();
        let cavalry = Target::Build(BuildUnit::Cavalry);
        game.handle_click(button_cursor(&game, cavalry), SCREEN, ClickMode::Normal);
        let city = game.selected_city.unwrap();
        assert_eq!(game.cities[city].queue, Some(BuildUnit::Cavalry));
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
    fn wrap_breaks_between_words() {
        assert_eq!(wrap("AB CD EF", 5), vec!["AB CD", "EF"]);
        assert_eq!(wrap("ABCDEFG HI", 5), vec!["ABCDEFG", "HI"]);
    }
}
