//! Screen-space UI: the selected unit's ability button.
//!
//! UI geometry is laid out in pixels with the origin at the window's
//! bottom-left and Y pointing up, the same orientation as the world, so the
//! shared shape and text helpers draw the same way in both.

use glam::{Mat4, Vec2, Vec3};

use super::ability::Ability;
use super::city::BuildUnit;
use super::orders::ClickMode;
use super::unit::Unit;
use super::{GameState, font, mesh};
use crate::renderer::Vertex;

type Color = [f32; 4];

/// Shared command-tray position. Cities and units use this same bottom-left area.
const TRAY_MIN: Vec2 = Vec2::new(16.0, 48.0);
const TRAY_MAX: Vec2 = Vec2::new(660.0, 282.0);
const READY_BG: Color = [0.16, 0.18, 0.24, 0.95];
const READY_TEXT: Color = [0.95, 0.95, 0.95, 1.0];
const DESCRIPTION_TEXT: Color = [0.80, 0.80, 0.84, 1.0];
const BUILD_BG: Color = [0.10, 0.13, 0.18, 0.98];
const BUILD_HOVER_BG: Color = [0.27, 0.36, 0.47, 1.0];
const BUILD_QUEUED_BG: Color = [0.52, 0.40, 0.15, 1.0];
const BUILD_BORDER: Color = [0.42, 0.49, 0.57, 1.0];

#[derive(Clone, Copy)]
enum UnitAction {
    Move,
    Attack,
    Ability,
    Settle,
    Road,
    Improve,
}

/// Maps UI pixels (origin bottom-left, Y up) to clip space.
pub fn ui_projection(screen_size: Vec2) -> Mat4 {
    Mat4::from_translation(Vec3::new(-1.0, 1.0, 0.0))
        * Mat4::from_scale(Vec3::new(2.0 / screen_size.x, -2.0 / screen_size.y, 1.0))
}

impl GameState {
    /// The UI as a triangle list in UI pixels: the ability button for the
    /// selected unit, if any.
    pub fn build_ui(&self, screen_size: Vec2) -> Vec<Vertex> {
        let mut out = Vec::new();
        self.city_ui(screen_size, &mut out);
        let Some(idx) = self.selected else { return out };
        // Cities own the tray while open; unit actions return after closing it.
        if self.selected_city.is_some() {
            return out;
        }
        self.unit_ui(idx, screen_size, &mut out);
        out
    }

    /// Handles a click on the UI, returning whether it hit anything (in which
    /// case it shouldn't also count as a click on the map). `cursor` is in
    /// window pixels with the origin at the top-left.
    pub(super) fn click_ui(&mut self, cursor: Vec2, screen_size: Vec2) -> bool {
        if cursor.y < 42.0 {
            self.end_planning();
            return true;
        }
        let point = Vec2::new(cursor.x, screen_size.y - cursor.y);
        if self.selected_city.is_some()
            && let Some(build) = build_option_at(point, screen_size)
        {
            self.queue_selected_city_unit(build);
            return true;
        }
        if self.selected_city.is_some()
            && point.cmpge(TRAY_MIN).all()
            && point.cmple(TRAY_MAX).all()
        {
            return true;
        }
        if self.selected.is_none() {
            return false;
        }
        if let Some(action) = unit_action_at(point, screen_size) {
            match action {
                UnitAction::Move => self.choose_move_action(),
                UnitAction::Attack => self.choose_attack_action(),
                UnitAction::Ability => self.toggle_selected_ability(),
                UnitAction::Settle => self.found_city_selected(),
                UnitAction::Road => self.build_worker_road_selected(),
                UnitAction::Improve => self.improve_worker_tile_selected(),
            }
            return true;
        }
        false
    }

    pub fn update_ui_hover(&mut self, cursor: Option<Vec2>, screen_size: Vec2) {
        self.hovered_build = self.selected_city.and_then(|_| {
            cursor.and_then(|p| build_option_at(Vec2::new(p.x, screen_size.y - p.y), screen_size))
        });
    }
}

impl GameState {
    fn unit_ui(&self, idx: usize, size: Vec2, out: &mut Vec<Vertex>) {
        let unit = &self.units[idx];
        mesh::quad(
            TRAY_MIN - Vec2::splat(2.0),
            TRAY_MAX + Vec2::splat(2.0),
            BUILD_BORDER,
            out,
        );
        mesh::quad(TRAY_MIN, TRAY_MAX, READY_BG, out);
        font::push_text(
            Vec2::new(30.0, 254.0),
            14.0,
            &format!("{:?} UNIT", unit.unit_type),
            READY_TEXT,
            out,
        );
        font::push_text(
            Vec2::new(30.0, 230.0),
            12.0,
            "CHOOSE AN ACTION, THEN CLICK THE MAP",
            DESCRIPTION_TEXT,
            out,
        );
        let mut actions = vec![UnitAction::Move, UnitAction::Attack];
        if self.settlers.contains(&unit.id) {
            actions.push(UnitAction::Settle);
        } else if self.workers.contains(&unit.id) {
            actions.extend([UnitAction::Road, UnitAction::Improve]);
        } else {
            actions.push(UnitAction::Ability);
        }
        for (i, action) in actions.into_iter().enumerate() {
            let (min, max) = unit_action_rect(i, size);
            let (label, sub, active) = match action {
                UnitAction::Move => (
                    "[M] MOVE",
                    "SELECT DESTINATION".to_string(),
                    self.ui_click_mode == Some(ClickMode::Move),
                ),
                UnitAction::Attack => (
                    "[X] ATTACK",
                    "SELECT TARGET".to_string(),
                    self.ui_click_mode == Some(ClickMode::Attack),
                ),
                UnitAction::Ability => {
                    let (name, description) = ability_text(unit);
                    (
                        "[Q] ABILITY",
                        format!("{}: {}", name, description),
                        unit.ability_queued,
                    )
                }
                UnitAction::Settle => ("[F] FOUND CITY", "CONSUMES SETTLER".to_string(), false),
                UnitAction::Road => ("[R] BUILD ROAD", "ON THIS TILE".to_string(), false),
                UnitAction::Improve => ("[I] IMPROVE", "FARM OR MINE".to_string(), false),
            };
            let bg = if active { BUILD_QUEUED_BG } else { BUILD_BG };
            mesh::quad(
                min - Vec2::splat(2.0),
                max + Vec2::splat(2.0),
                BUILD_BORDER,
                out,
            );
            mesh::quad(min, max, bg, out);
            font::push_text(min + Vec2::new(8.0, 34.0), 13.0, label, READY_TEXT, out);
            font::push_text(min + Vec2::new(8.0, 14.0), 9.0, &sub, DESCRIPTION_TEXT, out);
        }
    }
}

impl GameState {
    fn city_ui(&self, size: Vec2, out: &mut Vec<Vertex>) {
        use super::city::{amount, delivered_share};
        mesh::quad(Vec2::new(0.0, size.y - 42.0), size, READY_BG, out);
        let pending = (0..self.units.len())
            .filter(|&i| self.is_player_controlled(i) && self.needs_orders(i))
            .count();
        let header = if self.is_resolving() {
            format!("TURN {} - RESOLVING", self.turn)
        } else {
            format!(
                "TURN {} - {} UNITS NEED ORDERS - ENTER END PLANNING",
                self.turn + 1,
                pending
            )
        };
        font::push_text(
            Vec2::new(16.0, size.y - 28.0),
            14.0,
            &header,
            READY_TEXT,
            out,
        );
        font::push_text(
            Vec2::new(700.0, size.y - 28.0),
            12.0,
            &self.notice,
            READY_TEXT,
            out,
        );
        let Some(i) = self.selected_city else { return };
        let c = &self.cities[i];
        let (food, production) = self.income(i);
        let (growth_percent, _growth_needed, growth_label) = self.growth_status(i);
        mesh::quad(
            TRAY_MIN - Vec2::splat(2.0),
            TRAY_MAX + Vec2::splat(2.0),
            BUILD_BORDER,
            out,
        );
        mesh::quad(TRAY_MIN, TRAY_MAX, READY_BG, out);
        let mut lines = vec![
            format!("CITY {}  |  POP {}", c.id + 1, c.population),
            format!("WORKING {} OF {} CITIZENS", c.worked.len(), c.population),
            format!(
                "FOOD {}  NET {}",
                amount(c.food),
                amount(food - c.population as i32 * 8)
            ),
            format!("PROD {}  +{}", amount(c.production), amount(production)),
            c.queue.map_or("BUILD: CHOOSE A UNIT AT RIGHT".into(), |b| {
                format!(
                    "BUILDING {}: {} / {}",
                    b.name(),
                    amount(c.production),
                    amount(b.cost())
                )
            }),
            "A AUTO ASSIGN  |  ESC CLOSE".into(),
        ];
        if let Some(h) = self.inspected_tile {
            let (f, p) = self.tile_yield(h);
            let routes = self.routes(i);
            let share = routes.costs.get(&h).map_or(0, |c| delivered_share(*c));
            lines.push(format!("TILE F{} P{}  DELIVERS {}%", f, p, share * 25));
        }
        for (row, line) in lines.iter().enumerate() {
            font::push_text(
                Vec2::new(30.0, 254.0 - row as f32 * 26.0),
                12.0,
                line,
                READY_TEXT,
                out,
            );
        }
        let growth_min = Vec2::new(30.0, 82.0);
        let growth_size = Vec2::new(274.0, 15.0);
        mesh::quad(growth_min, growth_min + growth_size, [0.04, 0.06, 0.07, 1.0], out);
        mesh::quad(growth_min, growth_min + growth_size * Vec2::new(growth_percent as f32 / 100.0, 1.0), [0.30, 0.88, 0.35, 1.0], out);
        font::push_text(Vec2::new(30.0, 106.0), 11.0, &growth_label, READY_TEXT, out);
        for build in [
            BuildUnit::Melee,
            BuildUnit::Ranged,
            BuildUnit::Cavalry,
            BuildUnit::Siege,
        ] {
            let (min, max) = build_option_rect(build, size);
            let bg = if c.queue == Some(build) {
                BUILD_QUEUED_BG
            } else if self.hovered_build == Some(build) {
                BUILD_HOVER_BG
            } else {
                BUILD_BG
            };
            mesh::quad(
                min - Vec2::splat(2.0),
                max + Vec2::splat(2.0),
                BUILD_BORDER,
                out,
            );
            mesh::quad(min, max, bg, out);
            font::push_text(
                min + Vec2::new(8.0, 34.0),
                13.0,
                &format!("[{}] {}", build.shortcut(), build.name()),
                READY_TEXT,
                out,
            );
            font::push_text(
                min + Vec2::new(8.0, 14.0),
                11.0,
                &format!("{} PROD", super::city::amount(build.cost())),
                DESCRIPTION_TEXT,
                out,
            );
        }
        if let Some(build) = self.hovered_build {
            let tip = format!(
                "{}: {}  |  CLICK OR PRESS [{}] TO QUEUE",
                build.name(),
                build.description(),
                build.shortcut()
            );
            font::push_text(Vec2::new(334.0, 62.0), 12.0, &tip, DESCRIPTION_TEXT, out);
        }
    }
}

fn build_option_rect(build: BuildUnit, _size: Vec2) -> (Vec2, Vec2) {
    let index = match build {
        BuildUnit::Melee => 0,
        BuildUnit::Ranged => 1,
        BuildUnit::Cavalry => 2,
        BuildUnit::Siege => 3,
    };
    let min = Vec2::new(
        334.0 + (index % 2) as f32 * 156.0,
        142.0 - (index / 2) as f32 * 76.0,
    );
    (min, min + Vec2::new(154.0, 62.0))
}

fn build_option_at(point: Vec2, size: Vec2) -> Option<BuildUnit> {
    [
        BuildUnit::Melee,
        BuildUnit::Ranged,
        BuildUnit::Cavalry,
        BuildUnit::Siege,
    ]
    .into_iter()
    .find(|build| {
        let (min, max) = build_option_rect(*build, size);
        point.cmpge(min).all() && point.cmple(max).all()
    })
}

fn unit_action_rect(index: usize, _size: Vec2) -> (Vec2, Vec2) {
    let min = Vec2::new(
        30.0 + (index % 2) as f32 * 306.0,
        132.0 - (index / 2) as f32 * 76.0,
    );
    (min, min + Vec2::new(290.0, 62.0))
}

fn unit_action_at(point: Vec2, size: Vec2) -> Option<UnitAction> {
    [
        UnitAction::Move,
        UnitAction::Attack,
        UnitAction::Ability,
        UnitAction::Settle,
        UnitAction::Road,
        UnitAction::Improve,
    ]
    .into_iter()
    .find(|action| {
        let index = match action {
            UnitAction::Move => 0,
            UnitAction::Attack => 1,
            UnitAction::Ability | UnitAction::Settle | UnitAction::Road => 2,
            UnitAction::Improve => 3,
        };
        let (min, max) = unit_action_rect(index, size);
        point.cmpge(min).all() && point.cmple(max).all()
    })
}

/// The ability's button label and a one-line description of what it does.
fn ability_text(unit: &Unit) -> (&'static str, &'static str) {
    match unit.ability() {
        Ability::ShieldWall => ("SHIELD WALL", "+50% DEFENSE THIS TURN - CANNOT MOVE"),
        Ability::Volley => (
            "VOLLEY",
            "ATTACK ALSO HITS ENEMIES NEXT TO THE TARGET - ALL HITS DEAL 60%",
        ),
        Ability::Charge => ("CHARGE", "+1 MOVE AND +50% ATTACK THIS TURN"),
        Ability::Deploy if unit.deployed => ("PACK UP", "SPEND A TURN PACKING UP TO MOVE AGAIN"),
        Ability::Deploy => (
            "DEPLOY",
            "SPEND A TURN SETTING UP - THEN +1 RANGE BUT CANNOT MOVE",
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ui_projection_puts_the_origin_at_the_bottom_left() {
        let size = Vec2::new(1280.0, 720.0);
        let clip = |p: Vec2| ui_projection(size).project_point3(p.extend(0.0)).truncate();
        // Vulkan clip space has Y = +1 at the bottom of the window.
        assert!(clip(Vec2::ZERO).abs_diff_eq(Vec2::new(-1.0, 1.0), 1e-5));
        assert!(clip(size).abs_diff_eq(Vec2::new(1.0, -1.0), 1e-5));
    }

    #[test]
    fn build_choice_hit_testing_matches_its_button() {
        let size = Vec2::new(1280.0, 720.0);
        let (min, max) = build_option_rect(BuildUnit::Cavalry, size);
        assert_eq!(
            build_option_at((min + max) / 2.0, size),
            Some(BuildUnit::Cavalry)
        );
    }
}
