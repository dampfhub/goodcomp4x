//! Builds each frame's geometry from the game state.

use std::collections::HashSet;
use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, TAU};

use glam::Vec2;

use super::hex::{HEX_SIZE, Hex};
use super::orders::ClickMode;
use super::terrain::Terrain;
use super::turn::{Phase, step_rank};
use super::unit::{Team, Unit, UnitStats};
use super::{GameState, font, mesh};
use crate::renderer::Vertex;

type Color = [f32; 4];

const BORDER_COLOR: Color = [0.10, 0.10, 0.13, 1.0];
const PLAINS_COLOR: Color = [0.22, 0.22, 0.27, 1.0];
const HILLS_COLOR: Color = [0.28, 0.27, 0.23, 1.0];
const HILL_PEAK_COLOR: Color = [0.50, 0.46, 0.32, 1.0];
const MOUNTAIN_COLOR: Color = [0.13, 0.12, 0.12, 1.0];
const MOUNTAIN_PEAK_COLOR: Color = [0.44, 0.42, 0.42, 1.0];
const SNOW_COLOR: Color = [0.90, 0.92, 0.95, 1.0];
const SELECTED_COLOR: Color = [0.80, 0.78, 0.30, 1.0];
const CONTESTED_COLOR: Color = [0.55, 0.32, 0.10, 1.0];
const MOVE_RANGE_COLOR: Color = [0.24, 0.42, 0.26, 1.0];
const ATTACK_RANGE_COLOR: Color = [0.45, 0.22, 0.22, 1.0];
const ATTACK_RANGE_EMPTY_COLOR: Color = [0.36, 0.24, 0.20, 1.0];
const LABEL_COLOR: Color = [0.05, 0.05, 0.05, 1.0];
const HEALTH_BAR_BG_COLOR: Color = [0.08, 0.08, 0.08, 1.0];

/// Sizes below are for a unit drawn at full scale, alone in its hex.
const UNIT_ICON_RADIUS: f32 = HEX_SIZE * 0.42;
const LABEL_HEIGHT: f32 = UNIT_ICON_RADIUS * 0.9;
/// Icon growth while a unit is highlighted for having just acted.
const ACTED_SCALE: f32 = 1.35;

/// Rings drawn behind a unit's icon: gold while its ability is queued, steel
/// while a siege engine is deployed.
const ABILITY_RING_COLOR: Color = [0.95, 0.78, 0.25, 1.0];
const ABILITY_RING_RADIUS: f32 = UNIT_ICON_RADIUS * 1.32;
const DEPLOYED_RING_COLOR: Color = [0.62, 0.66, 0.72, 1.0];
const DEPLOYED_RING_RADIUS: f32 = UNIT_ICON_RADIUS * 1.18;

/// The two units sharing a contested hex are drawn at this scale, stacked
/// vertically with Blue on top.
const CONTESTED_SCALE: f32 = 0.5;
const CONTESTED_OFFSET_Y: f32 = 0.38;

/// Turn-order badges beside each unit: move order on the left, attack order on the right.
const BADGE_OFFSET_X: f32 = 0.62;
const BADGE_RADIUS: f32 = 0.17;
const BADGE_DIGIT_HEIGHT: f32 = 0.2;
const BADGE_BG_COLOR: Color = [0.06, 0.06, 0.08, 0.9];
const MOVE_ORDER_COLOR: Color = [0.45, 0.70, 1.00, 1.0];
const ATTACK_ORDER_COLOR: Color = [1.00, 0.40, 0.35, 1.0];

const HEALTH_BAR_WIDTH: f32 = 0.9;
const HEALTH_BAR_HEIGHT: f32 = 0.14;
const HEALTH_BAR_OFFSET_Y: f32 = 0.78;

/// Outline around each tile the hovered or selected city works.
const WORKED_OUTLINE_RADIUS: f32 = HEX_SIZE * 0.84;
const WORKED_OUTLINE_WIDTH: f32 = 0.06;
const WORKED_OUTLINE_RIM_WIDTH: f32 = 0.10;
const WORKED_OUTLINE_RIM_COLOR: Color = [0.02, 0.05, 0.03, 1.0];

/// A queued move is drawn as a faded copy of the unit at its destination.
const GHOST_ALPHA: f32 = 0.4;
const GHOST_FAN_RADIUS: f32 = HEX_SIZE * 0.3;

/// A queued swap is drawn as a link between the two allies.
const SWAP_LINK_WIDTH: f32 = 0.1;
const SWAP_LINK_ALPHA: f32 = 0.8;

const ATTACK_MARKER_RADIUS: f32 = HEX_SIZE * 0.16;
const ATTACK_MARKER_OFFSET: Vec2 = Vec2::new(0.5, 0.5);
const ATTACK_MARKER_FAN_RADIUS: f32 = HEX_SIZE * 0.18;

/// What the selected unit can do this turn, computed once per frame.
struct Selection {
    pos: Hex,
    planned_pos: Hex,
    team: Team,
    stats: UnitStats,
    reachable: HashSet<Hex>,
    /// Locked in a contested hex, so it can't attack anything else.
    locked: bool,
    /// Swap is armed, so adjacent allies are the hexes to highlight.
    swapping: bool,
}

impl GameState {
    /// The whole scene as a triangle list, back to front: hex grid, queued
    /// order markers, then units.
    pub fn build_vertices(&self) -> Vec<Vertex> {
        let mut out = Vec::new();

        let selection = self.selected.map(|idx| {
            let unit = &self.units[idx];
            let stats = unit.stats();
            // A green hex means clicking moves there, which isn't true while
            // an attack or swap is armed.
            let shows_moves = matches!(self.ui_click_mode, None | Some(ClickMode::Move));
            Selection {
                pos: unit.pos,
                planned_pos: unit.planned_pos(),
                team: unit.team,
                stats,
                reachable: if shows_moves {
                    self.reachable_hexes(unit.pos, stats.move_range)
                } else {
                    HashSet::new()
                },
                locked: self.rival_of(idx).is_some(),
                swapping: self.ui_click_mode == Some(ClickMode::Swap),
            }
        });

        for hex in self.grid.all_hexes() {
            let center = hex.to_world();
            let fill = self.hex_fill(hex, selection.as_ref());
            mesh::regular_polygon(center, HEX_SIZE, 6, 0.0, BORDER_COLOR, &mut out);
            mesh::regular_polygon(center, HEX_SIZE * 0.92, 6, 0.0, fill, &mut out);
            push_terrain_symbol(center, self.grid.terrain(hex), &mut out);
        }

        self.push_city_map(&mut out);
        self.push_order_markers(&mut out);

        for (idx, unit) in self.units.iter().enumerate() {
            let (center, scale) = self.unit_layout(idx);
            let (icon_scale, color) = if self.recent_actors.contains(&unit.id) {
                (scale * ACTED_SCALE, brighten(unit.team.color()))
            } else {
                (scale, unit.team.color())
            };
            push_status_rings(center, unit, scale, &mut out);
            let letter = self.unit_letter(unit);
            push_unit_icon(center, unit, letter, icon_scale, color, &mut out);
            push_order_badges(center, unit, scale, &mut out);
            push_health_bar(center, unit.hp / unit.max_hp(), scale, &mut out);
        }

        self.push_tile_yields(&mut out);
        out
    }

    /// Ghosts at queued move destinations, links between allies queued to
    /// swap, and diamonds on attacked hexes. Markers for units sharing a target
    /// hex are fanned out so each order stays visible.
    fn push_order_markers(&self, out: &mut Vec<Vertex>) {
        let swapping: HashSet<u32> = (0..self.units.len())
            .filter(|&i| self.swap_partner(i).is_some())
            .map(|i| self.units[i].id)
            .collect();

        let plain_moves = group_by_target(&self.units, |u| {
            u.planned_move.filter(|_| !swapping.contains(&u.id))
        });
        for (hex, movers) in plain_moves {
            for (i, unit) in movers.iter().enumerate() {
                let pos = fan_position(hex.to_world(), i, movers.len(), GHOST_FAN_RADIUS);
                let color = with_alpha(unit.team.color(), GHOST_ALPHA);
                push_unit_icon(pos, unit, self.unit_letter(unit), 1.0, color, out);
            }
        }

        for (idx, unit) in self.units.iter().enumerate() {
            if let Some(partner) = self.swap_partner(idx).filter(|&p| p > idx) {
                let color = with_alpha(unit.team.color(), SWAP_LINK_ALPHA);
                let (a, b) = (unit.pos.to_world(), self.units[partner].pos.to_world());
                mesh::segment(a, b, SWAP_LINK_WIDTH, color, out);
            }
        }

        for (hex, attackers) in group_by_target(&self.units, |u| u.planned_attack) {
            let base = hex.to_world() + ATTACK_MARKER_OFFSET;
            for (i, unit) in attackers.iter().enumerate() {
                let pos = fan_position(base, i, attackers.len(), ATTACK_MARKER_FAN_RADIUS);
                let color = unit.team.color();
                mesh::regular_polygon(pos, ATTACK_MARKER_RADIUS, 4, FRAC_PI_4, color, out);
            }
        }
    }

    /// Where to draw a unit and at what scale: full size in the middle of its
    /// hex, or half size and offset when it shares a contested hex.
    pub(super) fn unit_layout(&self, idx: usize) -> (Vec2, f32) {
        let unit = &self.units[idx];
        let center = unit.pos.to_world();
        if self.rival_of(idx).is_none() {
            return (center, 1.0);
        }
        let offset_y = if unit.team == Team::Blue {
            CONTESTED_OFFSET_Y
        } else {
            -CONTESTED_OFFSET_Y
        };
        (center + Vec2::new(0.0, offset_y), CONTESTED_SCALE)
    }

    /// The terrain's color, unless the hex is selected, contested, or in the
    /// selected unit's move or attack range. Mountains are never highlighted
    /// since nothing can move to or stand on them.
    fn hex_fill(&self, hex: Hex, selection: Option<&Selection>) -> Color {
        let terrain = self.grid.terrain(hex);
        let base = terrain_color(terrain);
        if !terrain.is_passable() {
            return base;
        }
        if selection.is_some_and(|sel| sel.pos == hex) {
            return SELECTED_COLOR;
        }
        if self.is_contested(hex) {
            return CONTESTED_COLOR;
        }
        let Some(sel) = selection else { return base };
        if sel.swapping {
            let swappable = sel.pos.distance(hex) == 1 && self.controlled_unit_at(hex).is_some();
            return if swappable { MOVE_RANGE_COLOR } else { base };
        }

        let in_attack_range =
            !sel.locked && sel.planned_pos.distance(hex) <= sel.stats.attack_range;
        let has_enemy = self.enemy_of_team_at(hex, sel.team).is_some();
        if self.is_occupied(hex) {
            return if has_enemy && in_attack_range {
                ATTACK_RANGE_COLOR
            } else {
                base
            };
        }
        if sel.reachable.contains(&hex) {
            MOVE_RANGE_COLOR
        } else if in_attack_range {
            ATTACK_RANGE_EMPTY_COLOR
        } else {
            base
        }
    }
}

impl GameState {
    fn push_city_map(&self, out: &mut Vec<Vertex>) {
        for &h in &self.roads {
            mesh::regular_polygon(h.to_world(), 0.12, 8, 0.0, [0.65, 0.45, 0.24, 1.0], out);
            for n in h.neighbors() {
                if (h.q, h.r) < (n.q, n.r) && self.is_road_hex(n) {
                    mesh::segment(
                        h.to_world(),
                        n.to_world(),
                        0.09,
                        [0.65, 0.45, 0.24, 1.0],
                        out,
                    );
                }
            }
        }
        // Road segments reaching a city are drawn even though the city center
        // itself is represented by its larger city marker.
        for city in &self.cities {
            for neighbor in city.pos.neighbors() {
                if self.roads.contains(&neighbor) {
                    mesh::segment(
                        city.pos.to_world(),
                        neighbor.to_world(),
                        0.09,
                        [0.65, 0.45, 0.24, 1.0],
                        out,
                    );
                }
            }
        }
        if let Some(i) = self.hovered_city.or(self.selected_city) {
            let routes = self.routes(i);
            for (h, cost) in &routes.costs {
                if self.hovered_city.is_none() {
                    continue;
                }
                font::push_text(
                    h.to_world() + Vec2::new(-0.3, 0.52),
                    0.18,
                    &format!("{}%", super::city::delivered_share(*cost) * 25),
                    [0.65, 0.85, 0.65, 1.0],
                    out,
                );
            }
            let manager_is_moving = self.moving_manager == Some(i);
            for (worker_index, h) in self.cities[i].worked.iter().enumerate() {
                if manager_is_moving && worker_index == 0 { continue; }
                let color = if routes.costs.contains_key(h) {
                    if worker_index == 0 { [1.0, 0.78, 0.20, 1.0] } else { [0.25, 1.0, 0.4, 1.0] }
                } else {
                    [1.0, 0.25, 0.2, 1.0]
                };
                // A colored ring on a slightly wider dark one, just inside the
                // hex's fill so it doesn't blur into the grid lines.
                let center = h.to_world();
                let radius = WORKED_OUTLINE_RADIUS;
                let rim = WORKED_OUTLINE_RIM_COLOR;
                mesh::polygon_outline(center, radius, WORKED_OUTLINE_RIM_WIDTH, 6, 0.0, rim, out);
                mesh::polygon_outline(center, radius, WORKED_OUTLINE_WIDTH, 6, 0.0, color, out);
                if worker_index == 0 {
                    font::push_glyph(center, 0.34, 'M', LABEL_COLOR, out);
                }
            }
            if !manager_is_moving && let Some(manager) = self.cities[i].worked.first() {
                for worker in self.cities[i].worked.iter().skip(1) {
                    push_dotted_segment(manager.to_world(), worker.to_world(), 0.045, [0.78, 0.88, 0.62, 0.9], out);
                }
            }
        }
        for (h, site) in &self.sites {
            font::push_glyph(
                h.to_world() + Vec2::new(-0.55, 0.3),
                0.28,
                site.label.chars().next().unwrap(),
                site.team.color(),
                out,
            );
        }
        for city in &self.cities {
            if let Some(hex) = city.barracks {
                mesh::regular_polygon(hex.to_world(), 0.31, 4, FRAC_PI_4, [0.72, 0.35, 0.18, 1.0], out);
                font::push_glyph(hex.to_world(), 0.30, 'B', LABEL_COLOR, out);
            }
        }
        for c in &self.cities {
            let pos = c.pos.to_world();
            mesh::quad(
                pos - Vec2::splat(0.48),
                pos + Vec2::splat(0.48),
                c.team.color(),
                out,
            );
            font::push_glyph(pos, 0.5, 'H', LABEL_COLOR, out);
        }
    }
}

/// Short dashes communicate a labor relationship without looking like a road.
fn push_dotted_segment(a: Vec2, b: Vec2, width: f32, color: Color, out: &mut Vec<Vertex>) {
    const DASHES: usize = 7;
    for index in 0..DASHES {
        let start = (index as f32 + 0.18) / DASHES as f32;
        let end = (index as f32 + 0.62) / DASHES as f32;
        mesh::segment(a.lerp(b, start), a.lerp(b, end), width, color, out);
    }
}

impl GameState {
    /// Show yields only while hovering a city, limited to its economic reach.
    fn push_tile_yields(&self, out: &mut Vec<Vertex>) {
        let Some(city) = self.hovered_city else {
            return;
        };
        let routes = self.routes(city);
        for hex in self.grid.all_hexes().filter(|h| self.grid.is_passable(*h)) {
            if !routes.costs.contains_key(&hex) && !self.cities[city].worked.contains(&hex) {
                continue;
            }
            let (food, production) = if self.cities.iter().any(|c| c.pos == hex) {
                (2, 1)
            } else {
                self.tile_yield(hex)
            };
            let center = hex.to_world() + Vec2::new(0.0, -0.49);
            mesh::quad(
                center - Vec2::new(0.52, 0.18),
                center + Vec2::new(0.52, 0.18),
                [0.035, 0.045, 0.045, 0.94],
                out,
            );
            for (is_food, count, offset, color) in [
                (true, food, -0.37, [0.42, 0.96, 0.32, 1.0]),
                (false, production, 0.14, [1.0, 0.69, 0.22, 1.0]),
            ] {
                let p = center + Vec2::new(offset, 0.0);
                if is_food {
                    // Grain stalk with paired kernels.
                    mesh::segment(
                        p + Vec2::new(0.0, -0.12),
                        p + Vec2::new(0.0, 0.12),
                        0.025,
                        color,
                        out,
                    );
                    for y in [-0.04, 0.04] {
                        for x in [-0.05, 0.05] {
                            mesh::regular_polygon(p + Vec2::new(x, y), 0.045, 4, 0.0, color, out);
                        }
                    }
                } else {
                    // Hammer: broad head and narrow handle.
                    mesh::quad(
                        p + Vec2::new(-0.02, -0.12),
                        p + Vec2::new(0.025, 0.06),
                        color,
                        out,
                    );
                    mesh::quad(
                        p + Vec2::new(-0.09, 0.04),
                        p + Vec2::new(0.09, 0.12),
                        color,
                        out,
                    );
                }
                font::push_text(
                    p + Vec2::new(0.11, -0.105),
                    0.21,
                    &count.to_string(),
                    color,
                    out,
                );
            }
        }
    }
}

fn terrain_color(terrain: Terrain) -> Color {
    match terrain {
        Terrain::Plains => PLAINS_COLOR,
        Terrain::Hills => HILLS_COLOR,
        Terrain::Mountains => MOUNTAIN_COLOR,
    }
}

/// Peak symbols drawn over the hex fill, so terrain stays recognizable under
/// selection highlights. Hills get two small peaks tucked below where a unit
/// icon sits; mountains get one large snow-capped peak.
fn push_terrain_symbol(center: Vec2, terrain: Terrain, out: &mut Vec<Vertex>) {
    // A triangle rotated a quarter turn points straight up.
    let mut peak = |offset: Vec2, radius: f32, color: Color| {
        mesh::regular_polygon(center + offset, radius, 3, FRAC_PI_2, color, out);
    };
    match terrain {
        Terrain::Plains => {}
        Terrain::Hills => {
            peak(Vec2::new(-0.3, -0.5), 0.2, HILL_PEAK_COLOR);
            peak(Vec2::new(0.25, -0.47), 0.17, HILL_PEAK_COLOR);
        }
        Terrain::Mountains => {
            let (base, radius, cap_radius) = (Vec2::new(0.0, -0.05), 0.55, 0.2);
            peak(base, radius, MOUNTAIN_PEAK_COLOR);
            // Same shape scaled down so it shares the big peak's apex.
            peak(
                base + Vec2::new(0.0, radius - cap_radius),
                cap_radius,
                SNOW_COLOR,
            );
        }
    }
}

/// Groups units by the hex `target` picks out, in first-seen order so marker
/// layout stays stable from frame to frame.
fn group_by_target(
    units: &[Unit],
    target: impl Fn(&Unit) -> Option<Hex>,
) -> Vec<(Hex, Vec<&Unit>)> {
    let mut groups: Vec<(Hex, Vec<&Unit>)> = Vec::new();
    for unit in units {
        let Some(hex) = target(unit) else { continue };
        match groups.iter_mut().find(|(h, _)| *h == hex) {
            Some((_, members)) => members.push(unit),
            None => groups.push((hex, vec![unit])),
        }
    }
    groups
}

/// Spreads `total` markers evenly around a small circle at `base`, or returns
/// `base` itself when there's only one.
fn fan_position(base: Vec2, index: usize, total: usize, radius: f32) -> Vec2 {
    if total <= 1 {
        return base;
    }
    base + Vec2::from_angle(TAU * index as f32 / total as f32) * radius
}

/// A polygon whose side count encodes the unit type, with `letter` on top.
fn push_unit_icon(
    center: Vec2,
    unit: &Unit,
    letter: char,
    scale: f32,
    color: Color,
    out: &mut Vec<Vertex>,
) {
    let sides = unit.unit_type.icon_sides();
    // A quarter turn so odd-sided shapes (the melee triangle) point up.
    mesh::regular_polygon(
        center,
        UNIT_ICON_RADIUS * scale,
        sides,
        FRAC_PI_2,
        color,
        out,
    );
    let label_color = with_alpha(LABEL_COLOR, color[3]);
    font::push_glyph(center, LABEL_HEIGHT * scale, letter, label_color, out);
}

/// Status rings behind the icon. They're filled discs, so only the rim shows
/// once the icon is drawn on top; the larger one goes first so both stay visible.
fn push_status_rings(center: Vec2, unit: &Unit, scale: f32, out: &mut Vec<Vertex>) {
    if unit.ability_queued {
        mesh::regular_polygon(
            center,
            ABILITY_RING_RADIUS * scale,
            24,
            0.0,
            ABILITY_RING_COLOR,
            out,
        );
    }
    if unit.deployed {
        mesh::regular_polygon(
            center,
            DEPLOYED_RING_RADIUS * scale,
            24,
            0.0,
            DEPLOYED_RING_COLOR,
            out,
        );
    }
}

/// Small numbered badges showing where the unit's type moves and attacks in
/// the turn order.
fn push_order_badges(center: Vec2, unit: &Unit, scale: f32, out: &mut Vec<Vertex>) {
    let badges = [
        (-BADGE_OFFSET_X, Phase::Move, MOVE_ORDER_COLOR),
        (BADGE_OFFSET_X, Phase::Attack, ATTACK_ORDER_COLOR),
    ];
    for (offset_x, phase, color) in badges {
        let pos = center + Vec2::new(offset_x * scale, 0.0);
        let digit = char::from_digit(step_rank(unit.unit_type, phase), 10).unwrap();
        mesh::regular_polygon(pos, BADGE_RADIUS * scale, 12, 0.0, BADGE_BG_COLOR, out);
        font::push_glyph(pos, BADGE_DIGIT_HEIGHT * scale, digit, color, out);
    }
}

fn push_health_bar(unit_center: Vec2, hp_fraction: f32, scale: f32, out: &mut Vec<Vertex>) {
    let hp = hp_fraction.clamp(0.0, 1.0);
    let size = Vec2::new(HEALTH_BAR_WIDTH, HEALTH_BAR_HEIGHT) * scale;
    let min = unit_center + Vec2::new(0.0, HEALTH_BAR_OFFSET_Y * scale) - size / 2.0;

    mesh::quad(min, min + size, HEALTH_BAR_BG_COLOR, out);
    mesh::quad(
        min,
        min + size * Vec2::new(hp, 1.0),
        [1.0 - hp, hp, 0.08, 1.0],
        out,
    );
}

fn brighten([r, g, b, a]: Color) -> Color {
    [
        (r + 0.35).min(1.0),
        (g + 0.35).min(1.0),
        (b + 0.35).min(1.0),
        a,
    ]
}

fn with_alpha([r, g, b, _]: Color, a: f32) -> Color {
    [r, g, b, a]
}
