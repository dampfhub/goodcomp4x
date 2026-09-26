//! Builds each frame's geometry from the game state.

use std::collections::HashSet;
use std::f32::consts::{FRAC_PI_4, TAU};

use glam::Vec2;

use super::hex::{HEX_SIZE, Hex};
use super::terrain::Terrain;
use super::turn::{Phase, step_rank};
use super::unit::{Team, Unit, UnitStats, UnitType};
use super::{GameState, font, mesh};
use crate::renderer::Vertex;

type Color = [f32; 4];

const BORDER_COLOR: Color = [0.52, 0.48, 0.34, 0.28];
const PLAINS_COLOR: Color = [0.22, 0.22, 0.27, 1.0];
const HILLS_COLOR: Color = [0.28, 0.27, 0.23, 1.0];

const MOUNTAIN_COLOR: Color = [0.13, 0.12, 0.12, 1.0];

const SELECTED_COLOR: Color = [0.80, 0.78, 0.30, 1.0];
const CONTESTED_COLOR: Color = [0.55, 0.32, 0.10, 1.0];
const MOVE_RANGE_COLOR: Color = [0.24, 0.42, 0.26, 1.0];
const ATTACK_RANGE_COLOR: Color = [0.45, 0.22, 0.22, 1.0];
const ATTACK_RANGE_EMPTY_COLOR: Color = [0.36, 0.24, 0.20, 1.0];
const LABEL_COLOR: Color = [0.90, 0.87, 0.73, 1.0];
const HEALTH_BAR_BG_COLOR: Color = [0.08, 0.08, 0.08, 1.0];

/// Sizes below are for a unit drawn at full scale, alone in its hex.
const UNIT_ICON_RADIUS: f32 = HEX_SIZE * 0.42;
const LABEL_HEIGHT: f32 = UNIT_ICON_RADIUS * 0.38;
/// Icon growth while a unit is highlighted for having just acted.
const ACTED_SCALE: f32 = 1.12;

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
const HEALTH_BAR_HEIGHT: f32 = 0.055;
const HEALTH_BAR_OFFSET_Y: f32 = -0.69;

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
}

impl GameState {
    /// The whole scene as a triangle list, back to front: hex grid, queued
    /// order markers, then units.
    pub fn build_vertices(&self) -> Vec<Vertex> {
        let mut out = Vec::with_capacity(12_000);
        // A continuous, darker landscape extends beyond the playable hexes.
        mesh::quad(Vec2::splat(-100.0), Vec2::splat(100.0), [1.0; 4], &mut out);
        mesh::material(&mut out, Vec2::ZERO, 1.0, 1.0, 3.0);

        let selection = self.selected.map(|idx| {
            let unit = &self.units[idx];
            let stats = unit.stats();
            Selection {
                pos: unit.pos,
                planned_pos: unit.planned_pos(),
                team: unit.team,
                stats,
                reachable: self.reachable_hexes(unit.pos, stats.move_range),
                locked: self.rival_of(idx).is_some(),
            }
        });

        for hex in self.grid.all_hexes() {
            let center = hex.to_world();
            let fill = self.hex_fill(hex, selection.as_ref());
            let terrain = self.grid.terrain(hex);
            let start = out.len();
            mesh::regular_polygon(center, HEX_SIZE, 6, 0.0, [1.0; 4], &mut out);
            let variant = match terrain {
                Terrain::Plains => 0.0,
                Terrain::Hills => 1.0,
                Terrain::Mountains => 2.0,
            };
            mesh::material(&mut out[start..], center, HEX_SIZE, 1.0, variant);
            mesh::ring(center, HEX_SIZE * 0.99, 6, 0.014, BORDER_COLOR, &mut out);
            if fill != terrain_color(terrain) {
                mesh::ring(center, HEX_SIZE * 0.93, 6, 0.032, fill, &mut out);
                mesh::regular_polygon(
                    center,
                    HEX_SIZE * 0.92,
                    6,
                    0.0,
                    with_alpha(fill, 0.08),
                    &mut out,
                );
            }
        }

        // Deterministic small groves sit at tile margins, leaving movement and
        // unit centers unobstructed. No random state is consumed by rendering.
        for hex in self.grid.all_hexes() {
            if self.grid.terrain(hex) != Terrain::Plains {
                continue;
            }
            let center = hex.to_world();
            let seed = (center.x * 17.31 + center.y * 41.73).sin().abs();
            if seed < 0.48 {
                continue;
            }
            for i in 0..3 {
                let pos = center + Vec2::new(-0.43 + i as f32 * 0.22, 0.47 + (i % 2) as f32 * 0.09);
                let radius = 0.23 + seed * 0.045;
                let start = out.len();
                mesh::quad(
                    pos - Vec2::splat(radius),
                    pos + Vec2::splat(radius),
                    [1.0; 4],
                    &mut out,
                );
                mesh::material(&mut out[start..], pos, radius, 2.0, 4.0);
            }
        }

        self.push_order_markers(&mut out);

        for (idx, unit) in self.units.iter().enumerate() {
            let (center, scale) = self.unit_layout(idx);
            let (icon_scale, color) = if self.recent_actors.contains(&unit.id) {
                (scale * ACTED_SCALE, brighten(unit.team.color()))
            } else {
                (scale, unit.team.color())
            };
            push_status_rings(center, unit, scale, &mut out);
            push_unit_icon(center, unit, icon_scale, color, &mut out);
            push_order_badges(center, unit, scale, &mut out);
            push_health_bar(center, unit.hp / unit.max_hp(), scale, &mut out);
        }

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
                push_unit_icon(pos, unit, 1.0, color, out);
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
    fn unit_layout(&self, idx: usize) -> (Vec2, f32) {
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

fn terrain_color(terrain: Terrain) -> Color {
    match terrain {
        Terrain::Plains => PLAINS_COLOR,
        Terrain::Hills => HILLS_COLOR,
        Terrain::Mountains => MOUNTAIN_COLOR,
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

/// Lit, volumetric miniatures with small class labels for tactical readability.
fn push_unit_icon(center: Vec2, unit: &Unit, scale: f32, color: Color, out: &mut Vec<Vertex>) {
    let radius = HEX_SIZE * 0.77 * scale;
    let variant = match unit.unit_type {
        UnitType::Melee => 0.0,
        UnitType::Ranged => 1.0,
        UnitType::Cavalry => 2.0,
        UnitType::Siege => 3.0,
    };
    let start = out.len();
    mesh::quad(
        center - Vec2::splat(radius),
        center + Vec2::splat(radius),
        color,
        out,
    );
    mesh::material(&mut out[start..], center, radius, 2.0, variant);
    let label = center + Vec2::new(0.0, -0.49 * scale);
    mesh::regular_polygon(
        label,
        0.13 * scale,
        16,
        0.0,
        with_alpha(BADGE_BG_COLOR, color[3]),
        out,
    );
    font::push_glyph(
        label,
        LABEL_HEIGHT * scale,
        unit.unit_type.letter(),
        with_alpha(LABEL_COLOR, color[3]),
        out,
    );
}
/// Status rings behind the miniature. Thin outlines keep the ground visible.
/// The larger one goes first so both stay visible.
fn push_status_rings(center: Vec2, unit: &Unit, scale: f32, out: &mut Vec<Vertex>) {
    if unit.ability_queued {
        mesh::ring(
            center,
            ABILITY_RING_RADIUS * scale,
            48,
            0.025,
            ABILITY_RING_COLOR,
            out,
        );
    }
    if unit.deployed {
        mesh::ring(
            center,
            DEPLOYED_RING_RADIUS * scale,
            48,
            0.02,
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
