//! Builds each frame's geometry from the game state.

use std::collections::{HashMap, HashSet};
use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, TAU};

use glam::Vec2;

use super::fog::{Fog, SeenBuilding};
use super::hex::{HEX_SIZE, Hex, HexGrid, edge, edge_corners};
use super::map_icons::{self, IMPROVEMENT_SPOT, MapIcon, RESOURCE_SPOT};
use super::orders::ClickMode;
use super::terrain::{Feature, Terrain, Tile};
use super::turn::{Phase, step_rank};
use super::unit::{Team, Unit, UnitStats};
use super::unit_icons::{self, UnitIcon};
use super::workers::{Structure, StructureKind};
use super::{GameState, PLAYER_TEAM, font, mesh};
use crate::renderer::Vertex;

type Color = [f32; 4];

const BORDER_COLOR: Color = [0.10, 0.10, 0.13, 1.0];
/// Each hex's fill as a share of its size; the rest is the border between hexes.
const HEX_FILL_SCALE: f32 = 0.92;
/// How far an explored hex's border reaches: past its own edge by as much as
/// a neighbor's border reaches in, so facing a never-seen hex or the map's
/// edge it is as wide as the whole gap between two hexes.
const OUTER_BORDER_RADIUS: f32 = HEX_SIZE * (2.0 - HEX_FILL_SCALE);
/// The grey veil over remembered hexes out of sight, and the line where they
/// meet hexes in sight: a light, cool grey like the cloud, so it can't be
/// mistaken for the dark gaps between ordinary hexes.
const FOG_EDGE_COLOR: Color = [0.24, 0.25, 0.28, 1.0];
/// The fog edge fills the whole gap between two hexes' fills: each fill stops
/// short of its hex's edge by (1 - HEX_FILL_SCALE) of the apothem, sqrt(3) / 2.
const FOG_EDGE_WIDTH: f32 = HEX_SIZE * (1.0 - HEX_FILL_SCALE) * 1.732_050_8;
/// Along a river the fog edge widens to cover it whole, since a river is wider
/// than the gap; otherwise a sliver of it would show on the side in sight.
const FOG_RIVER_EDGE_WIDTH: f32 = RIVER_WIDTH + 0.02;
const REMEMBERED_TINT: Color = [0.0, 0.0, 0.0, 0.58];
const CLOUD_SPACING: f32 = 4.6;
const PLAINS_COLOR: Color = [0.26, 0.24, 0.12, 1.0];
const GRASSLAND_COLOR: Color = [0.12, 0.20, 0.08, 1.0];
const DESERT_COLOR: Color = [0.45, 0.36, 0.17, 1.0];
const DUNE_COLOR: Color = [0.30, 0.22, 0.09, 1.0];
const TUNDRA_COLOR: Color = [0.20, 0.21, 0.19, 1.0];
const TUFT_COLOR: Color = [0.36, 0.38, 0.33, 1.0];
const SNOWFIELD_COLOR: Color = [0.55, 0.60, 0.66, 1.0];
const FOREST_COLOR: Color = [0.03, 0.10, 0.03, 1.0];
const MARSH_COLOR: Color = [0.09, 0.15, 0.11, 1.0];
const REED_COLOR: Color = [0.22, 0.30, 0.15, 1.0];
const JUNGLE_COLOR: Color = [0.02, 0.12, 0.05, 1.0];
const CANOPY_COLOR: Color = [0.01, 0.06, 0.02, 1.0];
const TREE_COLOR: Color = [0.015, 0.045, 0.015, 1.0];
const COAST_COLOR: Color = [0.06, 0.16, 0.30, 1.0];
const OCEAN_COLOR: Color = [0.02, 0.06, 0.16, 1.0];
const LAKE_COLOR: Color = [0.07, 0.19, 0.30, 1.0];
const WAVE_COLOR: Color = [0.16, 0.32, 0.52, 1.0];
const DEEP_WAVE_COLOR: Color = [0.05, 0.13, 0.28, 1.0];
const RIVER_COLOR: Color = [0.12, 0.34, 0.70, 1.0];
/// Rivers are a little wider than the dark gap between hexes they run along.
const RIVER_WIDTH: f32 = 0.16;
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

/// Sizes below are for a unit drawn at full scale, alone in its hex. Small
/// enough to leave the hex's top corners to resource and improvement icons.
const UNIT_ICON_RADIUS: f32 = HEX_SIZE * 0.36;
/// Sides of the disc military units stand on.
const TOKEN_SIDES: u32 = 32;
/// Civilian hexagons, relative to the military disc.
const CIVILIAN_TOKEN_SIZE: f32 = 0.95;
/// The dark edge around unit icons and map markers, so they stand out on any
/// terrain.
const ICON_OUTLINE_COLOR: Color = [0.03, 0.03, 0.04, 1.0];
const ICON_OUTLINE_WIDTH: f32 = 0.06;
/// Tile yields: below the unit spot, food then production as die-face pips
/// on a dark see-through pill.
const YIELD_ROW_OFFSET: Vec2 = Vec2::new(0.0, -0.56);
/// Distance between neighboring pips, across and up.
const YIELD_PIP_PITCH: Vec2 = Vec2::new(0.11, 0.15);
const YIELD_ICON_SCALE: f32 = 0.8;
/// How far a pip icon, at that scale, reaches from its center.
const YIELD_ICON_HALF: Vec2 = Vec2::new(0.05, 0.076);
/// Space between the food group and the production group.
const YIELD_GROUP_GAP: f32 = 0.08;
const YIELD_ROW_PADDING: Vec2 = Vec2::new(0.06, 0.035);
const YIELD_PIP_CORNER: f32 = 0.1;
const YIELD_ROW_COLOR: Color = [0.005, 0.006, 0.007, 0.85];
/// Past six, a yield is one icon and its amount.
const YIELD_DIGIT_HEIGHT: f32 = 0.12;
const YIELD_DIGIT_COLOR: Color = [0.95, 0.95, 0.95, 1.0];
const YIELD_NUMBER_GAP: f32 = 0.03;
/// Workers: tokens out on the map (smaller in a corner when a unit shares
/// their hex), the player's queued jobs as faded named rings, and the tag on
/// each of the player's cities counting the workers at home.
const WORKER_SCALE: f32 = 0.7;
const WORKER_BESIDE_SCALE: f32 = 0.45;
const WORKER_BESIDE_UNIT: Vec2 = Vec2::new(0.45, -0.32);
const PLANNED_JOB_COLOR: Color = [0.95, 0.78, 0.42, 0.75];
const PLANNED_JOB_COLOR_SOLID: Color = [0.95, 0.78, 0.42, 1.0];
const PLANNED_JOB_LABEL_OFFSET: Vec2 = Vec2::new(0.0, 0.6);
const PLANNED_JOB_LABEL_HEIGHT: f32 = 0.12;
const WORKER_TAG_MIN: Vec2 = Vec2::new(-0.78, -0.58);
const WORKER_TAG_MAX: Vec2 = Vec2::new(-0.3, -0.32);
const WORKER_TAG_COLOR: Color = [0.03, 0.03, 0.04, 0.92];
/// Structures workers build.
const STONE_COLOR: Color = [0.24, 0.23, 0.21, 1.0];
/// How thick a wall or gate is along its hex edge.
const BARRIER_WIDTH: f32 = 0.16;
const MORTAR_COLOR: Color = [0.09, 0.085, 0.08, 1.0];
const WOOD_COLOR: Color = [0.40, 0.20, 0.07, 1.0];
/// The granary marker beside a city.
const GRANARY_COLOR: Color = [0.95, 0.72, 0.22, 1.0];
/// Icon growth while a unit is highlighted for having just acted.
const ACTED_SCALE: f32 = 1.35;

/// Rings drawn behind a unit's icon: gold while its ability is queued, steel
/// while a siege engine is deployed.
const ABILITY_RING_COLOR: Color = [0.95, 0.78, 0.25, 1.0];
const ABILITY_RING_RADIUS: f32 = UNIT_ICON_RADIUS * 1.32;
const DEPLOYED_RING_COLOR: Color = [0.62, 0.66, 0.72, 1.0];
const DEPLOYED_RING_RADIUS: f32 = UNIT_ICON_RADIUS * 1.18;
/// A white hex outline around a guarding unit, outside any status rings.
const GUARD_OUTLINE_COLOR: Color = [0.92, 0.94, 0.98, 1.0];
const GUARD_OUTLINE_RADIUS: f32 = UNIT_ICON_RADIUS * 1.5;
const GUARD_OUTLINE_WIDTH: f32 = 0.05;

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
/// The dotted line along the open city's manager's delivery route, shown
/// while the cursor is on the manager.
const MANAGER_ROUTE_WIDTH: f32 = 0.06;
const MANAGER_ROUTE_COLOR: Color = [0.35, 0.82, 1.0, 0.9];

/// A queued move is drawn as a faded copy of the unit at its destination.
const GHOST_ALPHA: f32 = 0.4;
const GHOST_FAN_RADIUS: f32 = HEX_SIZE * 0.3;

/// A unit following a queue (Shift-click) shows its plan instead of a ghost,
/// while selected or hovered: a line along its moves, each turn's number on
/// the hex it moves to (and on each attack's arrow), all in pills rimmed in
/// team color (orange for attacks). A small tag beside the unit says a queue
/// is there, with the number of turns it has left.
const QUEUE_LINE_WIDTH: f32 = 0.07;
const QUEUE_LINE_ALPHA: f32 = 0.7;
const QUEUE_DIGIT_HEIGHT: f32 = 0.2;
const QUEUE_BADGE_HALF_HEIGHT: f32 = 0.18;
const QUEUE_BADGE_PADDING: f32 = 0.09;
const QUEUE_BADGE_RIM: f32 = 0.04;
const QUEUE_TEXT_COLOR: Color = [0.95, 0.95, 0.95, 1.0];
const QUEUE_TAG_OFFSET: Vec2 = Vec2::new(-0.44, -0.4);
const QUEUE_TAG_SCALE: f32 = 0.7;

/// A queued swap is drawn as a link between the two allies.
const SWAP_LINK_WIDTH: f32 = 0.1;
const SWAP_LINK_ALPHA: f32 = 0.8;

/// A queued attack is drawn as a curved arrow from the attacker (or its ghost,
/// if it's moving first) to the hex it's attacking.
const ATTACK_ARC_COLOR: Color = [1.00, 0.45, 0.30, 1.0];
const ATTACK_ARC_OUTLINE_COLOR: Color = [0.08, 0.03, 0.02, 1.0];
const ATTACK_ARC_WIDTH: f32 = 0.07;
const ATTACK_ARC_OUTLINE_WIDTH: f32 = 0.13;
/// How far the arc bows sideways, as a share of its length.
const ATTACK_ARC_BOW: f32 = 0.25;
/// The arc starts at the edge of the attacker's icon and stops short of the
/// target's center, so a unit drawn there doesn't hide the arrowhead.
const ATTACK_ARC_START: f32 = UNIT_ICON_RADIUS;
const ATTACK_ARC_STOP: f32 = HEX_SIZE * 0.55;
const ATTACK_ARC_SEGMENTS: usize = 20;
const ATTACK_ARROW_LENGTH: f32 = 0.28;
const ATTACK_ARROW_WIDTH: f32 = 0.26;

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
    /// The whole scene as a triangle list, back to front: hex grid, cities,
    /// fog, queued order markers, then units.
    pub fn build_vertices(&self) -> Vec<Vertex> {
        if let Some(city) = self.interior_view {
            return self.build_interior_vertices(city);
        }
        let mut out = Vec::new();
        let fog = self.fog();

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
                    self.known_reachable_hexes(unit.pos, stats.move_range, unit.team, &fog)
                } else {
                    HashSet::new()
                },
                locked: self.rival_of(idx).is_some(),
                swapping: self.ui_click_mode == Some(ClickMode::Swap),
            }
        });

        // The fixed cloud field sits behind the map. Known terrain painted
        // afterward hides it without clipping or rebuilding around sight.
        if self.fog_of_war {
            push_cloud_banks(&self.grid, &mut out);
        }

        // Never-seen hexes aren't drawn at all: the background shows there.
        let explored: Vec<Hex> = self
            .grid
            .all_hexes()
            .filter(|&h| self.is_explored(h))
            .collect();
        // Every border first, each reaching across the whole gap: the fills
        // drawn next cover what lies inside an explored neighbor, leaving a
        // full-width border facing never-seen hexes and the map's edge.
        for &hex in &explored {
            mesh::regular_polygon(
                hex.to_world(),
                OUTER_BORDER_RADIUS,
                6,
                0.0,
                BORDER_COLOR,
                &mut out,
            );
        }
        for &hex in &explored {
            let center = hex.to_world();
            let fill = self.hex_fill(hex, selection.as_ref(), &fog);
            mesh::regular_polygon(center, HEX_SIZE * HEX_FILL_SCALE, 6, 0.0, fill, &mut out);
            push_tile_symbols(center, self.grid.tile(hex), &mut out);
            if let Some(resource) = self.grid.resource(hex) {
                let icon = MapIcon::resource(resource);
                map_icons::push_map_icon(center + RESOURCE_SPOT, icon, &mut out);
            }
        }
        push_rivers(&self.grid, |h| self.is_explored(h), &mut out);

        self.push_city_map(&fog, &mut out);
        self.push_fog(&fog, &mut out);
        self.push_order_markers(&fog, &mut out);

        for (idx, unit) in self.units.iter().enumerate() {
            if !fog.shows(unit) {
                continue;
            }
            let (center, scale) = self.unit_layout(idx);
            let (icon_scale, color) = if self.recent_actors.contains(&unit.id) {
                (scale * ACTED_SCALE, brighten(unit.team.color()))
            } else {
                (scale, unit.team.color())
            };
            push_status_rings(center, unit, scale, &mut out);
            let look = self.unit_look(unit);
            push_unit_icon(center, look, icon_scale, color, &mut out);
            if self.show_details {
                push_order_badges(center, unit, look, scale, &mut out);
            }
            push_health_bar(center, unit.hp / unit.max_hp(), scale, &mut out);
            if unit.has_queue() && self.is_player_controlled(idx) {
                let text = format!(">{}", unit.plan_len());
                let at = center + QUEUE_TAG_OFFSET * scale;
                push_turn_badge(
                    at,
                    &text,
                    QUEUE_TAG_SCALE * scale,
                    unit.team.color(),
                    &mut out,
                );
            }
        }
        self.push_field_workers(&fog, &mut out);

        self.push_tile_yields(&fog, &mut out);
        self.push_effects(&mut out);
        out
    }

    /// The city interior is a separate world map, drawn with the normal hex
    /// and unit primitives rather than as buttons in a screen-space panel.
    fn build_interior_vertices(&self, city: usize) -> Vec<Vertex> {
        let mut out = Vec::new();
        let city = &self.cities[city];
        let center_hex = Hex::new(0, 0);
        let selected = self.interior_selected.and_then(|source| {
            city.interior
                .fighters
                .iter()
                .find(|f| f.source_id == source)
        });
        for q in -2..=2 {
            for r in -2..=2 {
                let hex = Hex::new(q, r);
                if hex.distance(center_hex) > 2 {
                    continue;
                }
                let at = hex.to_world();
                mesh::regular_polygon(at, OUTER_BORDER_RADIUS, 6, 0.0, BORDER_COLOR, &mut out);
                let fill = if hex == center_hex {
                    if city.interior.core_hp > 0.0 {
                        [0.30, 0.24, 0.14, 1.0]
                    } else {
                        [0.19, 0.10, 0.09, 1.0]
                    }
                } else if hex.distance(center_hex) == 2 {
                    [0.21, 0.24, 0.27, 1.0]
                } else {
                    [0.26, 0.29, 0.31, 1.0]
                };
                mesh::regular_polygon(at, HEX_SIZE * HEX_FILL_SCALE, 6, 0.0, fill, &mut out);
                if let Some(fighter) = selected {
                    let from = fighter.planned_move.unwrap_or(fighter.pos);
                    let occupied = city.interior.fighters.iter().find(|f| f.pos == hex);
                    let can_move = occupied.is_none()
                        && (hex != center_hex || city.interior.core_hp <= 0.0)
                        && from.distance(hex) <= fighter.unit_type.stats().move_range.max(1);
                    let can_attack = (occupied.is_some_and(|other| other.team != fighter.team)
                        || (hex == center_hex
                            && city.team != fighter.team
                            && city.interior.core_hp > 0.0))
                        && from.distance(hex) <= fighter.unit_type.stats().attack_range;
                    if can_move || can_attack {
                        mesh::polygon_outline(
                            at,
                            HEX_SIZE * 0.82,
                            0.065,
                            6,
                            0.0,
                            if can_attack {
                                ATTACK_RANGE_COLOR
                            } else {
                                MOVE_RANGE_COLOR
                            },
                            &mut out,
                        );
                    }
                }
            }
        }

        let post = center_hex.to_world();
        let breached = city.interior.core_hp <= 0.0;
        mesh::regular_polygon(post, 0.56, 6, 0.0, ICON_OUTLINE_COLOR, &mut out);
        mesh::regular_polygon(
            post,
            0.48,
            6,
            0.0,
            if breached {
                [0.22, 0.16, 0.14, 1.0]
            } else {
                city.team.color()
            },
            &mut out,
        );
        if breached {
            mesh::polygon_outline(post, 0.59, 0.07, 6, 0.0, SELECTED_COLOR, &mut out);
        }
        if breached {
            font::push_text_centered(
                post + Vec2::new(0.0, 0.13),
                0.16,
                "0 HP",
                SELECTED_COLOR,
                &mut out,
            );
            font::push_text_centered(
                post + Vec2::new(0.0, -0.14),
                if city.team == PLAYER_TEAM { 0.17 } else { 0.23 },
                if city.team == PLAYER_TEAM {
                    "DEFEND"
                } else {
                    "TAKE"
                },
                SELECTED_COLOR,
                &mut out,
            );
        } else {
            font::push_text_centered(
                post + Vec2::new(0.0, -0.07),
                0.23,
                "POST",
                LABEL_COLOR,
                &mut out,
            );
        }
        push_health_bar(
            post,
            city.interior.core_hp / super::city::CORE_HP,
            1.1,
            &mut out,
        );

        for fighter in &city.interior.fighters {
            let at = fighter.pos.to_world();
            if self.interior_selected == Some(fighter.source_id) {
                mesh::polygon_outline(at, 0.72, 0.09, 6, 0.0, SELECTED_COLOR, &mut out);
            }
            if let Some(dest) = fighter.planned_move {
                push_unit_icon(
                    dest.to_world(),
                    UnitLook {
                        icon: UnitIcon::of(fighter.unit_type),
                        civilian: false,
                    },
                    0.85,
                    with_alpha(fighter.team.color(), GHOST_ALPHA),
                    &mut out,
                );
            }
            if let Some(target) = fighter.planned_attack {
                push_attack_arc(
                    fighter.planned_move.unwrap_or(fighter.pos).to_world(),
                    target.to_world(),
                    &mut out,
                );
            }
        }
        for fighter in &city.interior.fighters {
            let at = fighter.pos.to_world();
            push_unit_icon(
                at,
                UnitLook {
                    icon: UnitIcon::of(fighter.unit_type),
                    civilian: false,
                },
                1.0,
                fighter.team.color(),
                &mut out,
            );
            push_health_bar(
                at,
                fighter.hp / fighter.unit_type.stats().max_hp,
                1.0,
                &mut out,
            );
        }
        out
    }

    /// Darkens remembered terrain and covers unexplored areas with clouds.
    /// Drawn over the map and cities but under units and orders.
    fn push_fog(&self, fog: &Fog, out: &mut Vec<Vertex>) {
        if !self.fog_of_war {
            return;
        }
        let remembered: Vec<Hex> = self
            .grid
            .all_hexes()
            .filter(|h| self.is_explored(*h) && !fog.sees(*h))
            .collect();
        let blank = |h: Hex| !self.grid.contains(h) || !self.is_explored(h);
        for &hex in &remembered {
            let center = hex.to_world();
            mesh::regular_polygon(center, HEX_SIZE, 6, 0.0, REMEMBERED_TINT, out);
            // Facing blank, the border reaches past the hex (see
            // OUTER_BORDER_RADIUS); veil that outer half too, so the band
            // is one shade.
            for n in hex.neighbors().into_iter().filter(|&n| blank(n)) {
                let (a, b) = edge_corners(hex, n);
                let grow = |p: Vec2| center + (p - center) * (OUTER_BORDER_RADIUS / HEX_SIZE);
                mesh::polygon(&[a, b, grow(b), grow(a)], REMEMBERED_TINT, out);
            }
        }
        for &hex in &remembered {
            for n in hex.neighbors() {
                if self.grid.contains(n) && fog.sees(n) {
                    let (a, b) = edge_corners(hex, n);
                    let width = if self.grid.has_river(hex, n) {
                        FOG_RIVER_EDGE_WIDTH
                    } else {
                        FOG_EDGE_WIDTH
                    };
                    mesh::segment(a, b, width, FOG_EDGE_COLOR, out);
                    // Round the joints where edges meet at a corner.
                    for p in [a, b] {
                        mesh::regular_polygon(p, width / 2.0, 12, 0.0, FOG_EDGE_COLOR, out);
                    }
                }
            }
        }
    }
}

/// Stable pseudo-random number for one cell of the cloud lattice.
fn cloud_hash(x: i32, y: i32, salt: u32) -> f32 {
    let mut bits =
        (x as u32).wrapping_mul(0x9E37_79B1) ^ (y as u32).wrapping_mul(0x85EB_CA77) ^ salt;
    bits ^= bits >> 16;
    bits = bits.wrapping_mul(0x7FEB_352D);
    bits ^= bits >> 15;
    bits as f32 / u32::MAX as f32
}

/// Draws one shaded octagon per world-space lattice point. This keeps
/// the cloud pattern continuous across hexes while doing constant, cheap work
/// per puff: no recursive subdivision and no noise sampling per vertex.
fn push_cloud_banks(grid: &HexGrid, out: &mut Vec<Vertex>) {
    let Some((min, max)) =
        grid.all_hexes()
            .map(Hex::to_world)
            .fold(None, |bounds: Option<(Vec2, Vec2)>, p| {
                Some(match bounds {
                    None => (p, p),
                    Some((min, max)) => (min.min(p), max.max(p)),
                })
            })
    else {
        return;
    };
    let min_x = (min.x / CLOUD_SPACING).floor() as i32 - 1;
    let max_x = (max.x / CLOUD_SPACING).ceil() as i32 + 1;
    let min_y = (min.y / CLOUD_SPACING).floor() as i32 - 1;
    let max_y = (max.y / CLOUD_SPACING).ceil() as i32 + 1;
    let cells = ((max_x - min_x + 1) * (max_y - min_y + 1)) as usize;
    out.reserve(cells * 3 * 8 * 3);
    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let bank = Vec2::new(x as f32, y as f32) * CLOUD_SPACING;
            for puff in 0..3_u32 {
                let salt = puff.wrapping_mul(0x9E37_79B9);
                let angle = cloud_hash(x, y, 0xA341_316C ^ salt) * TAU;
                let offset = 0.45 + cloud_hash(x, y, 0xC801_3EA4 ^ salt) * 1.45;
                let center = bank + Vec2::from_angle(angle) * offset;
                if !grid.contains(Hex::from_world(center)) {
                    continue;
                }
                let radius = 1.9 + cloud_hash(x, y, 0xAD90_777D ^ salt) * 0.9;
                let rotation = cloud_hash(x, y, 0x7E95_761E ^ salt) * TAU;
                push_cloud_puff(center, radius, rotation, out);
            }
        }
    }
}

fn push_cloud_puff(center: Vec2, radius: f32, rotation: f32, out: &mut Vec<Vertex>) {
    const SIDES: u32 = 8;
    let vertex = |p: Vec2, color: Color| Vertex {
        pos: [p.x, p.y, 0.0],
        color,
        uv: crate::renderer::SOLID_UV,
    };
    let middle = vertex(center, [0.19, 0.20, 0.23, 0.64]);
    for i in 0..SIDES {
        let corner =
            |i| center + Vec2::from_angle(rotation + TAU * i as f32 / SIDES as f32) * radius;
        out.extend([
            middle,
            vertex(corner(i), [0.10, 0.11, 0.14, 0.38]),
            vertex(corner(i + 1), [0.10, 0.11, 0.14, 0.38]),
        ]);
    }
}

impl GameState {
    /// Ghosts at queued move destinations, links between allies queued to
    /// swap, and diamonds on attacked hexes. Markers for units sharing a target
    /// hex are fanned out so each order stays visible. Units following a
    /// queue show their numbered plan instead, and only while selected or
    /// hovered.
    fn push_order_markers(&self, fog: &Fog, out: &mut Vec<Vertex>) {
        let swapping: HashSet<u32> = (0..self.units.len())
            .filter(|&i| self.swap_partner(i).is_some())
            .map(|i| self.units[i].id)
            .collect();

        // Where each moving unit's ghost is drawn, for its attack arc.
        let mut ghosts: HashMap<u32, Vec2> = HashMap::new();
        let plain_moves = group_by_target(&self.units, |u| {
            u.planned_move
                .filter(|_| !swapping.contains(&u.id) && !u.has_queue() && fog.shows(u))
        });
        for (hex, movers) in plain_moves {
            for (i, unit) in movers.iter().enumerate() {
                let pos = fan_position(hex.to_world(), i, movers.len(), GHOST_FAN_RADIUS);
                let color = with_alpha(unit.team.color(), GHOST_ALPHA);
                push_unit_icon(pos, self.unit_look(unit), 1.0, color, out);
                ghosts.insert(unit.id, pos);
            }
        }

        for (idx, unit) in self.units.iter().enumerate() {
            if let Some(partner) = self.swap_partner(idx).filter(|&p| p > idx) {
                let color = with_alpha(unit.team.color(), SWAP_LINK_ALPHA);
                let (a, b) = (unit.pos.to_world(), self.units[partner].pos.to_world());
                mesh::segment(a, b, SWAP_LINK_WIDTH, color, out);
            }
        }

        // Only the player's own attacks: the AI's plans aren't theirs to see.
        for (idx, unit) in self.units.iter().enumerate() {
            let Some(target) = unit
                .planned_attack
                .filter(|_| self.is_player_controlled(idx) && !unit.has_queue())
            else {
                continue;
            };
            // Swapping units attack from their partner's hex.
            let from = match ghosts.get(&unit.id) {
                Some(&ghost) => ghost,
                None if unit.planned_move.is_some() => unit.planned_pos().to_world(),
                None => self.unit_layout(idx).0,
            };
            push_attack_arc(from, target.to_world(), out);
        }

        let selection = self.selection();
        let hovered = self
            .hovered_tile
            .and_then(|hex| self.controlled_unit_at(hex));
        let plans: Vec<&Unit> = (0..self.units.len())
            .filter(|&i| selection.contains(&i) || hovered == Some(i))
            .filter(|&i| self.units[i].has_queue() && self.is_player_controlled(i))
            .map(|i| &self.units[i])
            .collect();
        push_queue_plans(&plans, out);
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
    fn hex_fill(&self, hex: Hex, selection: Option<&Selection>, fog: &Fog) -> Color {
        let tile = self.grid.tile(hex);
        let base = tile_color(tile);
        if !tile.terrain.is_passable() {
            return base;
        }
        let in_group = self.group.iter().any(|&i| self.units[i].pos == hex);
        if in_group || selection.is_some_and(|sel| sel.pos == hex) {
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
        let has_enemy = self.known_enemy_target_at(hex, sel.team, fog);
        if self.known_occupied(hex, fog) || has_enemy {
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
    /// Roads, improvements, cities and barracks, plus the open city's labor.
    /// Hexes in sight show what's there now; remembered hexes out of sight
    /// show what was there when last seen.
    fn push_city_map(&self, fog: &Fog, out: &mut Vec<Vertex>) {
        let view = self.map_view(fog);
        for &h in &view.roads {
            mesh::regular_polygon(h.to_world(), 0.12, 8, 0.0, [0.65, 0.45, 0.24, 1.0], out);
        }
        // Road segments join roads to each other and to cities, even though a
        // city center is drawn as its larger marker.
        let joins = |h: Hex| view.roads.contains(&h) || view.cities.iter().any(|c| c.0 == h);
        for &h in &view.roads {
            for n in h.neighbors() {
                let city = !view.roads.contains(&n);
                if joins(n) && (city || (h.q, h.r) < (n.q, n.r)) {
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
        if let Some(i) = self
            .hovered_city
            .or(self.selected_city)
            .filter(|&i| self.cities[i].team == PLAYER_TEAM || fog.sees(self.cities[i].pos))
        {
            // Delivery labels show routes as the player knows them; the
            // worked-tile rings below show whether goods really arrive.
            let routes = self.routes(i);
            let known_routes = self.known_routes(i, fog);
            for (h, cost) in &known_routes.costs {
                if self.yields_city() != Some(i) {
                    continue;
                }
                let food_share = self.mill_food_share(i, *h, *cost);
                let production_share = super::city::delivered_share(*cost);
                let label = if food_share == production_share {
                    format!("{}%", food_share * 25)
                } else {
                    format!("F{} P{}", food_share * 25, production_share * 25)
                };
                font::push_text(
                    h.to_world() + Vec2::new(-0.3, 0.52),
                    0.18,
                    &label,
                    [0.65, 0.85, 0.65, 1.0],
                    out,
                );
            }
            let manager_is_moving = self.moving_manager == Some(i);
            for (worker_index, h) in self.cities[i].worked.iter().enumerate() {
                if manager_is_moving && worker_index == 0 {
                    continue;
                }
                let color = if routes.costs.contains_key(h) {
                    if worker_index == 0 {
                        [1.0, 0.78, 0.20, 1.0]
                    } else {
                        [0.25, 1.0, 0.4, 1.0]
                    }
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
            if let Some(&manager) = self.cities[i].worked.first()
                && !manager_is_moving
                && self.hovered_tile == Some(manager)
            {
                // Hovering the manager traces the way its goods travel to the
                // city center, as the player knows the board.
                for leg in known_routes.path_from(manager).windows(2) {
                    let (from, to) = (leg[0].to_world(), leg[1].to_world());
                    push_dotted_segment(from, to, MANAGER_ROUTE_WIDTH, MANAGER_ROUTE_COLOR, out);
                }
            }
        }
        if let Some(i) = self.selected_barracks
            && let Some(barracks) = self.cities[i].barracks
        {
            let routes = self.known_routes_from(self.cities[i].team, barracks, fog);
            for (hex, cost) in &routes.costs {
                font::push_text(
                    hex.to_world() + Vec2::new(-0.3, 0.52),
                    0.18,
                    &format!("{}%", super::city::delivered_share(*cost) * 25),
                    [1.0, 0.70, 0.30, 1.0],
                    out,
                );
            }
            for source in &self.cities[i].worked {
                push_dotted_segment(
                    source.to_world(),
                    barracks.to_world(),
                    0.06,
                    [1.0, 0.62, 0.20, 0.9],
                    out,
                );
            }
        }
        for &(h, structure) in &view.structures {
            push_structure(h.to_world(), structure.kind, structure.team.color(), out);
        }
        self.push_planned_jobs(out);
        for (&(a, b), barrier) in &view.barriers {
            push_barrier(a, b, barrier.kind, barrier.team.color(), out);
        }
        if let Some((a, b)) = self.hovered_edge {
            let (start, end) = edge_corners(a, b);
            mesh::segment(start, end, BARRIER_WIDTH, PLANNED_JOB_COLOR_SOLID, out);
        }
        for &(h, label) in &view.sites {
            let center = h.to_world() + IMPROVEMENT_SPOT;
            match MapIcon::improvement(label) {
                Some(icon) => map_icons::push_map_icon(center, icon, out),
                None => {
                    let letter = label.chars().next().unwrap_or('?');
                    font::push_glyph(center, 0.2, letter, ICON_OUTLINE_COLOR, out);
                }
            }
        }
        for (hex, barracks) in &view.barracks {
            push_barracks_marker(hex.to_world(), barracks.team.color(), out);
            push_health_bar(hex.to_world(), barracks.health, 0.7, out);
        }
        for city in &self.cities {
            for (building, hex) in [
                (super::city::Building::Mill, city.mill),
                (super::city::Building::Workshop, city.workshop),
            ] {
                let Some(hex) = hex else {
                    continue;
                };
                if city.team != PLAYER_TEAM && !fog.sees(hex) {
                    continue;
                }
                let (badge, color) = building_badge(building);
                mesh::regular_polygon(hex.to_world(), 0.31, 4, FRAC_PI_4, color, out);
                font::push_glyph(hex.to_world(), 0.30, badge, LABEL_COLOR, out);
            }
        }
        // Planned sites stay visible until confirmation. An active placement
        // also follows the map hover before a site has been selected.
        for (i, city) in self.cities.iter().enumerate() {
            if city.team != PLAYER_TEAM {
                continue;
            }
            for building in [
                super::city::Building::Barracks,
                super::city::Building::Mill,
                super::city::Building::Workshop,
            ] {
                let planned = city.planned_sites.get(&building).copied();
                let preview = if self.site_placement() == Some((i, building)) {
                    self.hovered_tile
                        .filter(|&h| self.site_available(i, building, h))
                        .or(planned)
                } else {
                    planned
                };
                let Some(hex) = preview else {
                    continue;
                };
                let (badge, mut color) = building_badge(building);
                let is_hovered = self.hovered_tile == Some(hex);
                color[3] = if is_hovered { 0.95 } else { 0.55 };
                mesh::polygon_outline(
                    hex.to_world(),
                    WORKED_OUTLINE_RADIUS,
                    0.09,
                    6,
                    0.0,
                    color,
                    out,
                );
                mesh::regular_polygon(
                    hex.to_world(),
                    0.31,
                    4,
                    FRAC_PI_4,
                    [color[0], color[1], color[2], 0.48],
                    out,
                );
                font::push_glyph(hex.to_world(), 0.30, badge, [0.08, 0.05, 0.03, 0.65], out);
            }
        }
        for (hex, city) in &view.cities {
            push_city_marker(hex.to_world(), city, out);
        }
        for city in self.cities.iter().filter(|c| c.team == PLAYER_TEAM) {
            push_worker_count(city.pos.to_world(), city.workers, out);
        }
    }

    /// The player's queued worker jobs: a faded ring on each tile, named.
    fn push_planned_jobs(&self, out: &mut Vec<Vertex>) {
        let queued = self
            .cities
            .iter()
            .filter(|c| c.team == PLAYER_TEAM)
            .flat_map(|c| &c.worker_jobs);
        for job in queued {
            if let Some(across) = job.across {
                let (start, end) = edge_corners(job.hex, across);
                mesh::segment(start, end, BARRIER_WIDTH * 0.6, PLANNED_JOB_COLOR, out);
                continue;
            }
            let center = job.hex.to_world();
            mesh::polygon_outline(
                center,
                WORKED_OUTLINE_RADIUS,
                0.05,
                6,
                0.0,
                PLANNED_JOB_COLOR,
                out,
            );
            font::push_text_centered(
                center + PLANNED_JOB_LABEL_OFFSET,
                PLANNED_JOB_LABEL_HEIGHT,
                job.kind.name(),
                PLANNED_JOB_COLOR,
                out,
            );
        }
    }

    /// Workers out on the map that the player can see: small hollow tokens
    /// with a shovel, tucked into a corner when a unit shares their hex, and
    /// for the player's own, a dotted line to the job they're walking to.
    fn push_field_workers(&self, fog: &Fog, out: &mut Vec<Vertex>) {
        let look = UnitLook {
            icon: UnitIcon::Shovel,
            civilian: true,
        };
        for worker in self.field_workers.iter().filter(|w| fog.shows_worker(w)) {
            let shared = self.units_at(worker.pos).any(|i| fog.shows(&self.units[i]));
            let (center, scale) = if shared {
                (
                    worker.pos.to_world() + WORKER_BESIDE_UNIT,
                    WORKER_BESIDE_SCALE,
                )
            } else {
                (worker.pos.to_world(), WORKER_SCALE)
            };
            if worker.team == PLAYER_TEAM
                && let Some(job) = worker.job.filter(|job| job.hex != worker.pos)
            {
                push_dotted_segment(center, job.hex.to_world(), 0.05, PLANNED_JOB_COLOR, out);
            }
            let color = if self.recent_actors.contains(&worker.id) {
                brighten(worker.team.color())
            } else {
                worker.team.color()
            };
            push_unit_icon(center, look, scale, color, out);
        }
    }

    /// What `push_city_map` shows: the live map in sight, the memory of it
    /// elsewhere. The player's own cities and barracks always show.
    fn map_view(&self, fog: &Fog) -> MapView {
        let mut view = MapView::default();
        view.roads
            .extend(self.roads.iter().filter(|h| fog.sees(**h)));
        for (&h, site) in self.sites.iter().filter(|(h, _)| fog.sees(**h)) {
            view.sites.push((h, site.label));
        }
        view.structures.extend(
            self.structures
                .iter()
                .filter(|(h, _)| fog.sees(**h))
                .map(|(&h, &s)| (h, s)),
        );
        view.barriers.extend(
            self.barriers
                .iter()
                .filter(|((a, b), _)| fog.sees(*a) || fog.sees(*b)),
        );
        for city in &self.cities {
            let own = city.team == PLAYER_TEAM;
            let seen = |health| SeenBuilding {
                team: city.team,
                id: city.id,
                health,
                population: city.population,
                granary: city.built.contains(&super::city::Building::Granary),
            };
            if own || fog.sees(city.pos) {
                view.cities.push((city.pos, seen(1.0)));
            }
            if let Some(hex) = city.barracks.filter(|h| own || fog.sees(*h)) {
                let health = city.barracks_hp / super::city::BARRACKS_MAX_HP;
                view.barracks.push((hex, seen(health)));
            }
        }
        if !self.fog_of_war {
            return view;
        }
        for (&h, seen) in self.memory.iter().filter(|(h, _)| !fog.sees(**h)) {
            if seen.road {
                view.roads.push(h);
            }
            if let Some((label, _)) = seen.site {
                view.sites.push((h, label));
            }
            if let Some(structure) = seen.structure {
                view.structures.push((h, structure));
            }
            for &(across, barrier) in seen.barriers.iter().filter(|(n, _)| !fog.sees(*n)) {
                view.barriers.insert(edge(h, across), barrier);
            }
            if let Some(city) = seen.city.filter(|c| c.team != PLAYER_TEAM) {
                view.cities.push((h, city));
            }
            if let Some(barracks) = seen.barracks.filter(|b| b.team != PLAYER_TEAM) {
                view.barracks.push((h, barracks));
            }
        }
        view
    }
}

/// Roads, improvements, structures, cities and barracks to draw.
#[derive(Default)]
struct MapView {
    roads: Vec<Hex>,
    /// Improvements, by label.
    sites: Vec<(Hex, &'static str)>,
    structures: Vec<(Hex, Structure)>,
    /// Walls and gates, by edge.
    barriers: HashMap<(Hex, Hex), Structure>,
    cities: Vec<(Hex, SeenBuilding)>,
    barracks: Vec<(Hex, SeenBuilding)>,
}

fn building_badge(building: super::city::Building) -> (char, Color) {
    match building {
        super::city::Building::Barracks => ('B', [0.72, 0.35, 0.18, 1.0]),
        super::city::Building::Mill => ('M', [0.35, 0.65, 0.28, 1.0]),
        super::city::Building::Workshop => ('W', [0.38, 0.52, 0.82, 1.0]),
        super::city::Building::Granary => unreachable!(),
    }
}

/// Short dashes show where goods go without looking like a road.
fn push_dotted_segment(a: Vec2, b: Vec2, width: f32, color: Color, out: &mut Vec<Vertex>) {
    const DASHES: usize = 7;
    for index in 0..DASHES {
        let start = (index as f32 + 0.18) / DASHES as f32;
        let end = (index as f32 + 0.62) / DASHES as f32;
        mesh::segment(a.lerp(b, start), a.lerp(b, end), width, color, out);
    }
}

impl GameState {
    /// Alt: holding it shows extra map info (units' turn order, every tile's
    /// yields); releasing it hides it again.
    pub fn set_details(&mut self, held: bool) {
        self.show_details = held;
    }

    /// Yield chips below the unit spot: while Alt is held, on every explored
    /// tile that can be worked; otherwise, while the open city shows its
    /// yields, on the tiles within its economic reach.
    fn push_tile_yields(&self, fog: &Fog, out: &mut Vec<Vertex>) {
        let city = self.yields_city();
        if city.is_none() && !self.show_details {
            return;
        }
        let reach = city.map(|city| (city, self.known_routes(city, fog)));
        for hex in self
            .grid
            .all_hexes()
            .filter(|h| self.grid.terrain(*h).is_workable() && self.is_explored(*h))
        {
            let in_reach = reach.as_ref().is_some_and(|(city, routes)| {
                routes.costs.contains_key(&hex) || self.cities[*city].worked.contains(&hex)
            });
            if !in_reach && !self.show_details {
                continue;
            }
            let (food, production) = self.known_yield(hex, fog);
            push_yield_row(hex.to_world() + YIELD_ROW_OFFSET, food, production, out);
        }
    }
}

/// A tile's yields on a dark pill: food then production, each laid out like
/// the pips on a die, or as one icon and a number past six. Nothing for a
/// tile yielding nothing.
fn push_yield_row(center: Vec2, food: i32, production: i32, out: &mut Vec<Vertex>) {
    let row = yield_row(food, production);
    if row.icons.is_empty() {
        return;
    }
    let pill = rounded_rect(center, row.half, YIELD_PIP_CORNER);
    mesh::polygon(&pill, YIELD_ROW_COLOR, out);
    for (icon, at) in row.icons {
        map_icons::push_map_icon_scaled(center + at, icon, YIELD_ICON_SCALE, out);
    }
    for (text, at) in row.labels {
        font::push_text_centered(
            center + at,
            YIELD_DIGIT_HEIGHT,
            &text,
            YIELD_DIGIT_COLOR,
            out,
        );
    }
}

/// A yield row's contents, as offsets from its center.
struct YieldRow {
    /// Icons in drawing order: within a group, higher ones first so lower
    /// ones overlap them.
    icons: Vec<(MapIcon, Vec2)>,
    /// Amounts past six, beside their single icon.
    labels: Vec<(String, Vec2)>,
    /// How far the pill reaches each way.
    half: Vec2,
}

/// Where the pips for 1 to 6 go, in pip pitches, top row first: a die's
/// faces, but 2 side by side, 3 a triangle and 6 two rows of three.
fn pip_spots(amount: i32) -> &'static [(f32, f32)] {
    match amount {
        1 => &[(0.0, 0.0)],
        2 => &[(-0.5, 0.0), (0.5, 0.0)],
        3 => &[(0.0, 0.5), (-0.5, -0.5), (0.5, -0.5)],
        4 => &[(-0.5, 0.5), (0.5, 0.5), (-0.5, -0.5), (0.5, -0.5)],
        5 => &[
            (-1.0, 0.7),
            (1.0, 0.7),
            (0.0, 0.0),
            (-1.0, -0.7),
            (1.0, -0.7),
        ],
        6 => &[
            (-1.0, 0.5),
            (0.0, 0.5),
            (1.0, 0.5),
            (-1.0, -0.5),
            (0.0, -0.5),
            (1.0, -0.5),
        ],
        _ => &[],
    }
}

/// Lays out a tile's food and production groups side by side, centered.
fn yield_row(food: i32, production: i32) -> YieldRow {
    let mut row = YieldRow {
        icons: Vec::new(),
        labels: Vec::new(),
        half: Vec2::ZERO,
    };
    // Each group's contents around its own center, and its half extent.
    let groups: Vec<YieldRow> = [(MapIcon::Food, food), (MapIcon::Production, production)]
        .into_iter()
        .filter(|&(_, amount)| amount > 0)
        .map(|(icon, amount)| {
            let spots = pip_spots(amount);
            if spots.is_empty() {
                // Past six: one icon, then the number.
                let text = amount.to_string();
                let width = font::world_text_width(&text, YIELD_DIGIT_HEIGHT);
                let half_x = YIELD_ICON_HALF.x + (YIELD_NUMBER_GAP + width) / 2.0;
                return YieldRow {
                    icons: vec![(icon, Vec2::new(YIELD_ICON_HALF.x - half_x, 0.0))],
                    labels: vec![(text, Vec2::new(half_x - width / 2.0, 0.0))],
                    half: Vec2::new(half_x, YIELD_ICON_HALF.y),
                };
            }
            let spots: Vec<Vec2> = spots
                .iter()
                .map(|&(x, y)| Vec2::new(x, y) * YIELD_PIP_PITCH)
                .collect();
            let reach = spots.iter().fold(Vec2::ZERO, |m, s| m.max(s.abs()));
            YieldRow {
                icons: spots.into_iter().map(|at| (icon, at)).collect(),
                labels: Vec::new(),
                half: reach + YIELD_ICON_HALF,
            }
        })
        .collect();
    if groups.is_empty() {
        return row;
    }
    let width: f32 = groups.iter().map(|g| 2.0 * g.half.x).sum::<f32>()
        + YIELD_GROUP_GAP * (groups.len() - 1) as f32;
    let mut left = -width / 2.0;
    for group in groups {
        let shift = Vec2::new(left + group.half.x, 0.0);
        row.icons
            .extend(group.icons.into_iter().map(|(icon, at)| (icon, at + shift)));
        row.labels.extend(
            group
                .labels
                .into_iter()
                .map(|(text, at)| (text, at + shift)),
        );
        row.half.y = row.half.y.max(group.half.y);
        left += 2.0 * group.half.x + YIELD_GROUP_GAP;
    }
    row.half = Vec2::new(width / 2.0, row.half.y) + YIELD_ROW_PADDING;
    row
}

/// The corners of a rectangle with rounded corners of `radius`, centered on
/// `center` and reaching `half` out each way.
fn rounded_rect(center: Vec2, half: Vec2, radius: f32) -> Vec<Vec2> {
    const STEPS: usize = 4;
    let inner = half - Vec2::splat(radius);
    [(1.0, 1.0), (-1.0, 1.0), (-1.0, -1.0), (1.0, -1.0)]
        .into_iter()
        .enumerate()
        .flat_map(|(quarter, (sx, sy))| {
            let corner = center + inner * Vec2::new(sx, sy);
            (0..=STEPS).map(move |step| {
                let angle = (quarter * STEPS + step) as f32 / (4 * STEPS) as f32 * TAU;
                corner + Vec2::from_angle(angle) * radius
            })
        })
        .collect()
}

/// The ground's color, tinted green under forest or jungle.
fn tile_color(tile: Tile) -> Color {
    let ground = match tile.terrain {
        Terrain::Grassland => GRASSLAND_COLOR,
        Terrain::Plains => PLAINS_COLOR,
        Terrain::Desert => DESERT_COLOR,
        Terrain::Tundra => TUNDRA_COLOR,
        Terrain::Snow => SNOWFIELD_COLOR,
        Terrain::Marsh => MARSH_COLOR,
        Terrain::Mountains => MOUNTAIN_COLOR,
        Terrain::Coast => COAST_COLOR,
        Terrain::Ocean => OCEAN_COLOR,
        Terrain::Lake => LAKE_COLOR,
    };
    match tile.feature {
        Some(Feature::Forest) => mix(ground, FOREST_COLOR, 0.5),
        Some(Feature::Jungle) => mix(ground, JUNGLE_COLOR, 0.6),
        None => ground,
    }
}

fn mix(a: Color, b: Color, t: f32) -> Color {
    std::array::from_fn(|i| a[i] + (b[i] - a[i]) * t)
}

/// Symbols drawn over the hex fill, so a tile stays recognizable under
/// selection highlights. They sit clear of the middle, where a unit's icon
/// goes. Hills are two small peaks in a darker shade of their ground along
/// the bottom; forest (pines) and jungle (round canopies) go along the
/// bottom too, or along the top on hills. Bare ground gets its own marks
/// along the bottom: dunes for desert, grass tufts for tundra, reeds for
/// marsh. Mountains get one large snow-capped peak, water waves.
fn push_tile_symbols(center: Vec2, tile: Tile, out: &mut Vec<Vertex>) {
    // A triangle rotated a quarter turn points straight up.
    let peak = |offset: Vec2, radius: f32, color: Color, out: &mut Vec<Vertex>| {
        mesh::regular_polygon(center + offset, radius, 3, FRAC_PI_2, color, out);
    };
    if tile.hills {
        let shade = mix(tile_color(tile), [0.0, 0.0, 0.0, 1.0], 0.55);
        peak(Vec2::new(-0.3, -0.5), 0.2, shade, out);
        peak(Vec2::new(0.25, -0.47), 0.17, shade, out);
    }
    let row_y = if tile.hills { 1.0 } else { -1.0 };
    match tile.feature {
        Some(Feature::Forest) => {
            for (x, y, r) in [(-0.36, 0.46, 0.15), (0.0, 0.6, 0.17), (0.36, 0.46, 0.15)] {
                peak(Vec2::new(x, y * row_y), r, TREE_COLOR, out);
            }
            return;
        }
        Some(Feature::Jungle) => {
            for (x, y, r) in [(-0.34, 0.48, 0.12), (0.0, 0.6, 0.14), (0.34, 0.48, 0.12)] {
                let at = center + Vec2::new(x, y * row_y);
                mesh::regular_polygon(at, r, 10, 0.0, CANOPY_COLOR, out);
            }
            return;
        }
        None if tile.hills => return,
        None => {}
    }
    match tile.terrain {
        Terrain::Grassland | Terrain::Plains | Terrain::Snow => {}
        Terrain::Marsh => {
            for (x, y) in [(-0.34, -0.52), (0.0, -0.62), (0.34, -0.52)] {
                let base = center + Vec2::new(x, y);
                for dx in [-0.06, 0.0, 0.06] {
                    mesh::segment(
                        base + Vec2::new(dx, 0.0),
                        base + Vec2::new(dx, 0.13),
                        0.02,
                        REED_COLOR,
                        out,
                    );
                }
            }
        }
        Terrain::Desert => {
            for (x, y) in [(-0.32, -0.5), (0.08, -0.62), (0.34, -0.42)] {
                wave(center + Vec2::new(x, y), 0.2, DUNE_COLOR, out);
            }
        }
        Terrain::Tundra => {
            for (x, y) in [(-0.34, -0.5), (0.0, -0.62), (0.34, -0.5)] {
                let base = center + Vec2::new(x, y);
                for dx in [-0.05, 0.0, 0.05] {
                    mesh::segment(base, base + Vec2::new(dx, 0.1), 0.025, TUFT_COLOR, out);
                }
            }
        }
        Terrain::Coast | Terrain::Lake => {
            wave(center + Vec2::new(-0.15, 0.15), 0.4, WAVE_COLOR, out);
            wave(center + Vec2::new(0.15, -0.2), 0.4, WAVE_COLOR, out);
        }
        Terrain::Ocean => {
            wave(center + Vec2::new(-0.15, 0.15), 0.4, DEEP_WAVE_COLOR, out);
            wave(center + Vec2::new(0.15, -0.2), 0.4, DEEP_WAVE_COLOR, out);
        }
        Terrain::Mountains => {
            let (base, radius, cap_radius) = (Vec2::new(0.0, -0.05), 0.55, 0.2);
            peak(base, radius, MOUNTAIN_PEAK_COLOR, out);
            // Same shape scaled down so it shares the big peak's apex.
            peak(
                base + Vec2::new(0.0, radius - cap_radius),
                cap_radius,
                SNOW_COLOR,
                out,
            );
        }
    }
}

/// A small wave (or dune), `width` across, centered on `at`.
fn wave(at: Vec2, width: f32, color: Color, out: &mut Vec<Vertex>) {
    let (w, h) = (width / 4.0, width * 0.12);
    let points = [
        at + Vec2::new(-2.0 * w, 0.0),
        at + Vec2::new(-w, h),
        at,
        at + Vec2::new(w, h),
        at + Vec2::new(2.0 * w, 0.0),
    ];
    mesh::polyline(&points, width * 0.1, color, out);
}

/// Rivers along hex edges, with a round joint at each end so consecutive
/// edges meet cleanly.
fn push_rivers(grid: &HexGrid, explored: impl Fn(Hex) -> bool, out: &mut Vec<Vertex>) {
    for (a, b) in grid.rivers().filter(|(a, b)| explored(*a) || explored(*b)) {
        let (start, end) = edge_corners(a, b);
        mesh::segment(start, end, RIVER_WIDTH, RIVER_COLOR, out);
        for p in [start, end] {
            mesh::regular_polygon(p, RIVER_WIDTH / 2.0, 12, 0.0, RIVER_COLOR, out);
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

/// A queued attack's arrow from `from` toward `to`.
fn push_attack_arc(from: Vec2, to: Vec2, out: &mut Vec<Vertex>) {
    let points = attack_arc_points(from, to, 1.0);
    push_arrow(&points, ATTACK_ARC_COLOR, ATTACK_ARC_OUTLINE_COLOR, out);
}

/// The plans of units following a queue: a line along each one's moves with each
/// turn's number (1 is this turn) on the hex it moves to, and each attack's
/// arrow from where the unit stands that turn, numbered at its middle. A hex
/// or arrow used on several turns lists them all ("2,4").
fn push_queue_plans(units: &[&Unit], out: &mut Vec<Vertex>) {
    // Several units' plans (a group's) share labels, so two of them passing
    // one hex on different turns read "2,3" instead of hiding each other.
    let mut stops: Vec<(Hex, Vec<usize>)> = Vec::new();
    let mut strikes: Vec<((Hex, Hex), Vec<usize>)> = Vec::new();
    for unit in units {
        let line = with_alpha(unit.team.color(), QUEUE_LINE_ALPHA);
        for turn in 0..unit.plan_len() {
            let (from, to) = (unit.pos_after(turn), unit.pos_after(turn + 1));
            if from != to {
                mesh::segment(from.to_world(), to.to_world(), QUEUE_LINE_WIDTH, line, out);
                add_label(&mut stops, to, turn + 1);
            }
            if let Some(target) = unit.attack_on_turn(turn) {
                add_label(&mut strikes, (to, target), turn + 1);
            }
        }
    }
    let mut badges = Vec::new();
    for ((from, target), turns) in strikes {
        let points = attack_arc_points(from.to_world(), target.to_world(), 1.0);
        push_arrow(&points, ATTACK_ARC_COLOR, ATTACK_ARC_OUTLINE_COLOR, out);
        let middle = points
            .get(points.len() / 2)
            .copied()
            .unwrap_or(target.to_world());
        badges.push((middle, turns, ATTACK_ARC_COLOR));
    }
    // Every queue shown is the player's, so one team color rims the moves.
    let rim = units.first().map_or(PLAYER_TEAM, |u| u.team).color();
    badges.extend(
        stops
            .into_iter()
            .map(|(hex, turns)| (hex.to_world(), turns, rim)),
    );
    for (at, mut turns, rim) in badges {
        turns.sort_unstable();
        turns.dedup();
        let text: Vec<String> = turns.iter().map(usize::to_string).collect();
        push_turn_badge(at, &text.join(","), 1.0, rim, out);
    }
}

/// Adds `turn` to the turns labeled at `key`, in first-seen order.
fn add_label<K: PartialEq>(labels: &mut Vec<(K, Vec<usize>)>, key: K, turn: usize) {
    match labels.iter_mut().find(|(k, _)| *k == key) {
        Some((_, turns)) => turns.push(turn),
        None => labels.push((key, vec![turn])),
    }
}

/// A dark pill rimmed in `rim` with `text` on it, centered on `center`.
fn push_turn_badge(center: Vec2, text: &str, scale: f32, rim: Color, out: &mut Vec<Vertex>) {
    let height = QUEUE_DIGIT_HEIGHT * scale;
    let radius = QUEUE_BADGE_HALF_HEIGHT * scale;
    // How far the round ends' centers sit from the middle: none for a
    // single digit, which makes the pill a disc.
    let reach = (font::world_text_width(text, height) / 2.0 + QUEUE_BADGE_PADDING * scale - radius)
        .max(0.0);
    push_pill(center, reach, radius + QUEUE_BADGE_RIM * scale, rim, out);
    push_pill(center, reach, radius, BADGE_BG_COLOR, out);
    font::push_text_centered(center, height, text, QUEUE_TEXT_COLOR, out);
}

/// A horizontal pill: discs of `radius` `reach` either side of `center`,
/// joined by a band.
fn push_pill(center: Vec2, reach: f32, radius: f32, color: Color, out: &mut Vec<Vertex>) {
    for side in [-1.0, 1.0] {
        let end = center + Vec2::new(side * reach, 0.0);
        mesh::regular_polygon(end, radius, 20, 0.0, color, out);
    }
    if reach > 0.0 {
        let half = Vec2::new(reach, radius);
        mesh::quad(center - half, center + half, color, out);
    }
}

/// Points along an attack's curve from `from` toward `to`: a quadratic curve
/// bowing to the left of the direction of travel (so opposing attacks
/// between two hexes don't overlap), from the edge of the attacker's icon to
/// just short of the target. Only the first `progress` (0 to 1) of it, for an
/// arrow still in flight. Empty if the two are too close for an arrow.
pub(super) fn attack_arc_points(from: Vec2, to: Vec2, progress: f32) -> Vec<Vec2> {
    let chord = to - from;
    let length = chord.length();
    if length <= ATTACK_ARC_START + ATTACK_ARC_STOP || progress <= 0.0 {
        return Vec::new();
    }
    let control = (from + to) / 2.0 + chord.perp() * ATTACK_ARC_BOW;
    let point = |t: f32| from.lerp(control, t).lerp(control.lerp(to, t), t);
    let start = ATTACK_ARC_START / length;
    let end = start + (1.0 - ATTACK_ARC_STOP / length - start) * progress.min(1.0);
    (0..=ATTACK_ARC_SEGMENTS)
        .map(|i| point(start + (end - start) * i as f32 / ATTACK_ARC_SEGMENTS as f32))
        .collect()
}

/// Draws an arrow along `points` over a darker outline, its head at the last
/// point.
pub(super) fn push_arrow(points: &[Vec2], color: Color, outline: Color, out: &mut Vec<Vertex>) {
    let &[.., before, tip] = points else {
        return;
    };
    let direction = (tip - before).normalize_or_zero();
    for (width, color) in [
        (ATTACK_ARC_OUTLINE_WIDTH, outline),
        (ATTACK_ARC_WIDTH, color),
    ] {
        mesh::polyline(points, width, color, out);
        // The outline pass draws its arrowhead a little bigger all round.
        let grow = (width - ATTACK_ARC_WIDTH) / 2.0;
        let base = tip - direction * grow;
        let side = direction.perp() * (ATTACK_ARROW_WIDTH / 2.0 + grow);
        let point = tip + direction * (ATTACK_ARROW_LENGTH + grow);
        mesh::triangle(point, base + side, base - side, color, out);
    }
}

/// How a unit is drawn, beyond its type: its pictogram, and whether it's a
/// civilian (settler or worker), drawn hollow.
#[derive(Clone, Copy)]
pub(super) struct UnitLook {
    pub icon: UnitIcon,
    pub civilian: bool,
}

/// The unit's token in its team color with a dark outline, so it reads on
/// any terrain, and its pictogram on top. Military units stand on a disc;
/// civilians on a hollow pointy-top hexagon (a pale center inside a
/// team-colored rim), unlike both the disc and the flat-top map hexes.
fn push_unit_icon(center: Vec2, look: UnitLook, scale: f32, color: Color, out: &mut Vec<Vertex>) {
    let (sides, rotation, size) = if look.civilian {
        (6, FRAC_PI_2, CIVILIAN_TOKEN_SIZE)
    } else {
        (TOKEN_SIDES, 0.0, 1.0)
    };
    let radius = UNIT_ICON_RADIUS * scale * size;
    let alpha = color[3];
    mesh::regular_polygon(center, radius, sides, rotation, color, out);
    if look.civilian {
        let pale = with_alpha(mix(color, [1.0, 1.0, 1.0, 1.0], 0.7), alpha);
        mesh::regular_polygon(center, radius * 0.68, sides, rotation, pale, out);
    }
    let outline = with_alpha(ICON_OUTLINE_COLOR, alpha);
    mesh::polygon_outline(
        center,
        radius,
        ICON_OUTLINE_WIDTH * scale,
        sides,
        rotation,
        outline,
        out,
    );
    unit_icons::push_pictogram(
        center,
        radius,
        look.icon,
        with_alpha(LABEL_COLOR, alpha),
        out,
    );
}

/// Axis-aligned rectangles in `color` with a dark border, the border drawn
/// first under all of them so shapes built from several rectangles get one
/// clean outline.
fn push_outlined_rects(rects: &[(Vec2, Vec2)], color: Color, out: &mut Vec<Vertex>) {
    let outline = with_alpha(ICON_OUTLINE_COLOR, color[3]);
    let grow = Vec2::splat(ICON_OUTLINE_WIDTH);
    for &(min, max) in rects {
        mesh::quad(min - grow, max + grow, outline, out);
    }
    for &(min, max) in rects {
        mesh::quad(min, max, color, out);
    }
}

/// A city: a crenellated tower in its team's color with its population on
/// it, and a small gold granary beside it once it has one.
fn push_city_marker(pos: Vec2, city: &SeenBuilding, out: &mut Vec<Vertex>) {
    let rect =
        |x0: f32, y0: f32, x1: f32, y1: f32| (pos + Vec2::new(x0, y0), pos + Vec2::new(x1, y1));
    push_outlined_rects(
        &[
            rect(-0.42, -0.4, 0.42, 0.24),
            // Three merlons along the top.
            rect(-0.42, 0.24, -0.24, 0.4),
            rect(-0.09, 0.24, 0.09, 0.4),
            rect(0.24, 0.24, 0.42, 0.4),
        ],
        city.team.color(),
        out,
    );
    // Centered in the tower's body, below the merlons.
    font::push_text_centered(
        pos + Vec2::new(0.0, -0.08),
        0.3,
        &city.population.to_string(),
        LABEL_COLOR,
        out,
    );
    if city.granary {
        let at = pos + Vec2::new(0.46, -0.42);
        mesh::regular_polygon(
            at,
            0.15 + ICON_OUTLINE_WIDTH,
            16,
            0.0,
            ICON_OUTLINE_COLOR,
            out,
        );
        mesh::regular_polygon(at, 0.15, 16, 0.0, GRANARY_COLOR, out);
        font::push_glyph(at, 0.16, 'G', LABEL_COLOR, out);
    }
}

/// How many workers a city has at home: a dark tag at the tower's lower
/// left with a shovel and the count.
fn push_worker_count(city: Vec2, count: u32, out: &mut Vec<Vertex>) {
    let (min, max) = (city + WORKER_TAG_MIN, city + WORKER_TAG_MAX);
    let edge = Vec2::splat(ICON_OUTLINE_WIDTH / 2.0);
    mesh::quad(min - edge, max + edge, ICON_OUTLINE_COLOR, out);
    mesh::quad(min, max, WORKER_TAG_COLOR, out);
    let middle = (min.y + max.y) / 2.0;
    let height = max.y - min.y;
    unit_icons::push_pictogram(
        Vec2::new(min.x + height / 2.0, middle),
        height * 0.55,
        UnitIcon::Shovel,
        PLANNED_JOB_COLOR_SOLID,
        out,
    );
    font::push_text_centered(
        Vec2::new(max.x - height / 2.0, middle),
        height * 0.55,
        &count.to_string(),
        PLANNED_JOB_COLOR_SOLID,
        out,
    );
}

/// A wall or gate along the edge between `a` and `b`: a band of stone with
/// mortar joints and posts in its team's color at both ends; a gate's middle
/// is a door in the team's color.
fn push_barrier(a: Hex, b: Hex, kind: StructureKind, team: Color, out: &mut Vec<Vertex>) {
    let (start, end) = edge_corners(a, b);
    let along = |t: f32| start.lerp(end, t);
    mesh::segment(
        start,
        end,
        BARRIER_WIDTH + ICON_OUTLINE_WIDTH,
        ICON_OUTLINE_COLOR,
        out,
    );
    mesh::segment(start, end, BARRIER_WIDTH, STONE_COLOR, out);
    let across = (end - start).perp().normalize_or_zero() * (BARRIER_WIDTH / 2.0);
    if kind == StructureKind::Gate {
        let door = [along(0.3), along(0.7)];
        mesh::segment(
            door[0],
            door[1],
            BARRIER_WIDTH + ICON_OUTLINE_WIDTH,
            ICON_OUTLINE_COLOR,
            out,
        );
        mesh::segment(door[0], door[1], BARRIER_WIDTH, team, out);
        mesh::segment(
            along(0.5) - across,
            along(0.5) + across,
            0.02,
            ICON_OUTLINE_COLOR,
            out,
        );
        for t in [0.15, 0.85] {
            mesh::segment(
                along(t) - across,
                along(t) + across,
                0.02,
                MORTAR_COLOR,
                out,
            );
        }
    } else {
        for t in [0.25, 0.5, 0.75] {
            mesh::segment(
                along(t) - across,
                along(t) + across,
                0.02,
                MORTAR_COLOR,
                out,
            );
        }
    }
    for p in [start, end] {
        let half = Vec2::splat(BARRIER_WIDTH * 0.62);
        let edge = Vec2::splat(ICON_OUTLINE_WIDTH / 2.0);
        mesh::quad(p - half - edge, p + half + edge, ICON_OUTLINE_COLOR, out);
        mesh::quad(p - half, p + half, team, out);
    }
}

/// A structure on a tile, in its team's color: an outpost (a watchtower) or
/// a fort (a palisade of stakes).
fn push_structure(center: Vec2, kind: StructureKind, team: Color, out: &mut Vec<Vertex>) {
    let at = |x: f32, y: f32| center + Vec2::new(x, y);
    match kind {
        // Walls and gates stand on hex edges (`push_barrier`).
        StructureKind::Wall | StructureKind::Gate => {}
        StructureKind::Outpost => {
            for (foot, top) in [
                ((-0.16, -0.36), (-0.09, 0.05)),
                ((0.16, -0.36), (0.09, 0.05)),
            ] {
                let (foot, top) = (at(foot.0, foot.1), at(top.0, top.1));
                mesh::segment(foot, top, 0.09, ICON_OUTLINE_COLOR, out);
                mesh::segment(foot, top, 0.05, WOOD_COLOR, out);
            }
            push_outlined_rects(&[(at(-0.15, 0.03), at(0.15, 0.24))], WOOD_COLOR, out);
            let roof = [at(-0.22, 0.24), at(0.22, 0.24), at(0.0, 0.44)];
            mesh::polygon(
                &[at(-0.27, 0.21), at(0.27, 0.21), at(0.0, 0.48)],
                ICON_OUTLINE_COLOR,
                out,
            );
            mesh::polygon(&roof, team, out);
        }
        StructureKind::Fort => {
            // Stakes around the hex, points outward, over a ring in the
            // team's color.
            mesh::polygon_outline(center, 0.56, 0.05, 6, 0.0, team, out);
            for i in 0..12 {
                let out_dir = Vec2::from_angle(i as f32 * std::f32::consts::TAU / 12.0);
                let side = out_dir.perp() * 0.06;
                let base = center + out_dir * 0.56;
                let tip = center + out_dir * 0.74;
                mesh::polygon(
                    &[
                        base - side * 1.6 - out_dir * 0.03,
                        base + side * 1.6 - out_dir * 0.03,
                        tip + out_dir * 0.03,
                    ],
                    ICON_OUTLINE_COLOR,
                    out,
                );
                mesh::polygon(&[base - side, base + side, tip], WOOD_COLOR, out);
            }
        }
    }
}

/// A barracks: a small house (walls and a pitched roof) in `color`, its
/// team's, marked B.
fn push_barracks_marker(pos: Vec2, color: Color, out: &mut Vec<Vertex>) {
    let alpha = color[3];
    let outline = with_alpha(ICON_OUTLINE_COLOR, alpha);
    let (eave, peak, half) = (0.1, 0.4, 0.38);
    let roof = |grow: f32| {
        (
            pos + Vec2::new(-half - grow, eave - grow / 2.0),
            pos + Vec2::new(half + grow, eave - grow / 2.0),
            pos + Vec2::new(0.0, peak + grow),
        )
    };
    let (a, b, c) = roof(ICON_OUTLINE_WIDTH * 1.6);
    mesh::triangle(a, b, c, outline, out);
    let walls = (pos + Vec2::new(-0.28, -0.32), pos + Vec2::new(0.28, eave));
    push_outlined_rects(&[walls], color, out);
    let (a, b, c) = roof(0.0);
    mesh::triangle(a, b, c, color, out);
    font::push_glyph(
        pos + Vec2::new(0.0, -0.09),
        0.26,
        'B',
        with_alpha(LABEL_COLOR, alpha),
        out,
    );
}

/// Status rings behind the icon. They're filled discs, so only the rim shows
/// once the icon is drawn on top; the larger one goes first so both stay visible.
/// A guarding unit also gets a hex outline around everything.
fn push_status_rings(center: Vec2, unit: &Unit, scale: f32, out: &mut Vec<Vertex>) {
    if unit.guarding {
        mesh::polygon_outline(
            center,
            GUARD_OUTLINE_RADIUS * scale,
            GUARD_OUTLINE_WIDTH * scale,
            6,
            0.0,
            GUARD_OUTLINE_COLOR,
            out,
        );
    }
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
fn push_order_badges(center: Vec2, unit: &Unit, look: UnitLook, scale: f32, out: &mut Vec<Vertex>) {
    let badges = [
        (-BADGE_OFFSET_X, Phase::Move, MOVE_ORDER_COLOR),
        (BADGE_OFFSET_X, Phase::Attack, ATTACK_ORDER_COLOR),
    ];
    // Civilians never attack, so they get no attack badge.
    let shown = if look.civilian { 1 } else { 2 };
    for (offset_x, phase, color) in badges.into_iter().take(shown) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::fog::tests::{behind_the_mountain, glance_at, remembered_route_hex};
    use crate::game::unit::{Unit, UnitType};

    #[test]
    fn breached_post_gains_a_distinct_world_marker() {
        let mut game = GameState::siege_scenario();
        let intact = count_color(&game.build_vertices(), SELECTED_COLOR);
        game.cities[1].interior.core_hp = 0.0;
        let breached = count_color(&game.build_vertices(), SELECTED_COLOR);
        assert!(breached > intact, "breached post needs a visible gold ring");
    }

    #[test]
    fn explored_map_has_no_cloud_bank_geometry() {
        let mut game = GameState::world_scenario(3);
        for h in game.grid.all_hexes() {
            game.memory
                .insert(h, super::super::fog::Sighting::default());
        }
        let fog = game.fog();
        let mut vertices = Vec::new();
        game.push_fog(&fog, &mut vertices);
        assert!(
            vertices
                .iter()
                .all(|v| v.color == REMEMBERED_TINT || v.color == FOG_EDGE_COLOR)
        );
        game.fog_of_war = false;
        vertices.clear();
        game.push_fog(&game.fog(), &mut vertices);
        assert!(vertices.is_empty());
    }

    #[test]
    fn unexplored_cloud_geometry_stays_small() {
        let game = GameState::world_scenario(3);
        let mut vertices = Vec::new();
        push_cloud_banks(&game.grid, &mut vertices);
        // The former recursively sampled mesh emitted well over 150,000 fog
        // vertices here. Keep enough headroom for map-size tuning without
        // allowing that per-frame cost back in.
        assert!(vertices.len() < 25_000, "{} cloud vertices", vertices.len());
        assert!(vertices.iter().any(|v| v.color[3] < 1.0));
    }

    /// The color of the last opaque triangle drawn over `point`.
    fn top_color(vertices: &[Vertex], point: Vec2) -> Option<Color> {
        vertices
            .as_chunks::<3>()
            .0
            .iter()
            .rev()
            .filter(|t| t.iter().all(|v| v.color[3] == 1.0))
            .find(|t| {
                let [a, b, c] = [0, 1, 2].map(|i| Vec2::new(t[i].pos[0], t[i].pos[1]));
                let sides = [
                    (b - a).perp_dot(point - a),
                    (c - b).perp_dot(point - b),
                    (a - c).perp_dot(point - c),
                ];
                sides.iter().all(|&s| s >= 0.0) || sides.iter().all(|&s| s <= 0.0)
            })
            .map(|t| t[0].color)
    }

    /// A point in the gap between adjacent hexes, on `from`'s side of their
    /// edge, midway along it.
    fn in_gap(from: Hex, to: Hex) -> Vec2 {
        let (a, b) = edge_corners(from, to);
        let middle = (a + b) / 2.0;
        middle + (from.to_world() - middle).normalize() * FOG_EDGE_WIDTH / 4.0
    }

    #[test]
    fn the_border_spans_the_whole_gap_beside_unexplored_hexes() {
        let mut game = GameState::world_scenario(3);
        game.explore();
        let vertices = game.build_vertices();
        let fog = game.fog();
        // An edge from a hex in sight to a neighbor matching `wanted`, with
        // no river along it.
        let edge_to = |wanted: &dyn Fn(Hex) -> bool| {
            game.grid
                .all_hexes()
                .filter(|&h| fog.sees(h))
                .find_map(|hex| {
                    let n = hex
                        .neighbors()
                        .into_iter()
                        .find(|&n| wanted(n) && !game.grid.has_river(hex, n))?;
                    Some((hex, n))
                })
                .expect("such an edge on the map")
        };

        // Both halves of the gap retain the explored tile's border; cloud
        // puffs are translucent and do not replace that opaque geometry.
        let (hex, unexplored) = edge_to(&|h: Hex| game.grid.contains(h) && !game.is_explored(h));
        for point in [in_gap(hex, unexplored), in_gap(unexplored, hex)] {
            assert_eq!(top_color(&vertices, point), Some(BORDER_COLOR));
        }
        // The unknown tile has no terrain geometry; cloud puffs blend over
        // the renderer's dark background.
        let beyond = unexplored.to_world();
        assert_eq!(top_color(&vertices, beyond), None);
    }

    #[test]
    fn a_site_preview_follows_the_cursor_only_in_its_open_city() {
        let mut game = GameState::city_scenario();
        game.units.clear();
        game.selected = None;
        game.explore();
        let city = game
            .cities
            .iter()
            .position(|c| c.team == PLAYER_TEAM)
            .unwrap();
        let building = crate::game::city::Building::Barracks;
        let site = game
            .grid
            .all_hexes()
            .find(|&h| game.site_available(city, building, h))
            .expect("an open site");
        game.hovered_tile = Some(site);
        let without = |game: &GameState| {
            let mut plain = game.clone();
            plain.placing_building = None;
            scene(&plain)
        };
        game.placing_building = Some((city, building));
        game.selected_city = None;
        assert_eq!(scene(&game), without(&game), "no city open: no preview");
        game.selected_city = Some(city);
        assert_ne!(scene(&game), without(&game), "its city open: a preview");
    }

    /// Every vertex as plain data, sorted: labels over a route map come out
    /// in hash order, so two builds of one scene can differ only in order.
    fn scene(game: &GameState) -> Vec<[u32; 9]> {
        let mut vertices: Vec<[u32; 9]> = game
            .build_vertices()
            .iter()
            .map(|v| {
                let [x, y, z] = v.pos.map(f32::to_bits);
                let [r, g, b, a] = v.color.map(f32::to_bits);
                let [u, w] = v.uv.map(f32::to_bits);
                [x, y, z, r, g, b, a, u, w]
            })
            .collect();
        vertices.sort_unstable();
        vertices
    }

    fn count_color(vertices: &[Vertex], color: Color) -> usize {
        vertices.iter().filter(|v| v.color == color).count()
    }

    #[test]
    fn a_queue_shows_numbered_turns_only_while_selected_or_hovered() {
        let mut game = GameState::new();
        game.fog_of_war = false;
        let melee = game
            .units
            .iter()
            .position(|u| u.team == Team::Blue && u.unit_type == crate::game::unit::UnitType::Melee)
            .unwrap();
        game.selected = Some(melee);
        game.hovered_tile = None;
        let start = game.units[melee].pos;
        let first = start.neighbors()[0];
        let second = first.neighbors()[0];
        assert!(game.queue_move(first) && game.queue_move(second));
        let line = with_alpha(Team::Blue.color(), QUEUE_LINE_ALPHA);
        let ghost = with_alpha(Team::Blue.color(), GHOST_ALPHA);

        let selected = game.build_vertices();
        assert!(count_color(&selected, line) > 0, "the path shows");
        assert_eq!(count_color(&selected, ghost), 0, "numbers, not a ghost");

        game.selected = None;
        let deselected = game.build_vertices();
        assert_eq!(count_color(&deselected, line), 0, "hidden once let go of");
        assert_eq!(count_color(&deselected, ghost), 0);

        game.hovered_tile = Some(start);
        let hovered = game.build_vertices();
        assert_eq!(count_color(&hovered, line), count_color(&selected, line));
    }

    #[test]
    fn turn_order_numbers_show_only_while_alt_is_held() {
        let mut game = GameState::world_scenario(3);
        game.explore();
        assert_eq!(count_color(&game.build_vertices(), MOVE_ORDER_COLOR), 0);
        game.set_details(true);
        assert!(count_color(&game.build_vertices(), MOVE_ORDER_COLOR) > 0);
        game.set_details(false);
        assert_eq!(count_color(&game.build_vertices(), MOVE_ORDER_COLOR), 0);
    }

    #[test]
    fn alt_shows_yields_on_every_explored_tile_outside_the_city_view() {
        let mut game = GameState::city_scenario();
        game.explore();
        let city = game
            .cities
            .iter()
            .position(|c| c.team == PLAYER_TEAM)
            .unwrap();
        // Every pill is the same outline, whatever its width.
        let pill_vertices = count_color(
            &{
                let mut out = Vec::new();
                push_yield_row(Vec2::ZERO, 2, 1, &mut out);
                out
            },
            YIELD_ROW_COLOR,
        );
        let rows =
            |game: &GameState| count_color(&game.build_vertices(), YIELD_ROW_COLOR) / pill_vertices;
        game.selected_city = None;
        assert_eq!(rows(&game), 0, "no yields without the city view or Alt");
        game.selected_city = Some(city);
        let in_reach = rows(&game);
        assert!(in_reach > 0, "the open city shows its tiles' yields");
        game.selected_city = None;
        game.set_details(true);
        let fog = game.fog();
        let yielding = game
            .grid
            .all_hexes()
            .filter(|&h| game.grid.terrain(h).is_workable() && game.is_explored(h))
            .filter(|&h| game.known_yield(h, &fog) != (0, 0))
            .count();
        assert!(yielding > in_reach);
        assert_eq!(rows(&game), yielding, "a row on every tile that yields");
    }

    #[test]
    fn yields_up_to_six_are_die_pips_and_more_are_a_number() {
        let pips = |amount: i32| -> Vec<Vec2> {
            let row = yield_row(amount, 0);
            assert!(row.labels.is_empty(), "{amount} needs no number");
            row.icons.into_iter().map(|(_, at)| at).collect()
        };
        let rows = |spots: &[Vec2]| {
            let mut ys: Vec<i32> = spots
                .iter()
                .map(|s| (s.y * 1000.0).round() as i32)
                .collect();
            ys.dedup();
            ys.len()
        };
        for amount in 1..=6 {
            assert_eq!(pips(amount).len(), amount as usize);
        }
        assert_eq!(rows(&pips(2)), 1, "2 side by side");
        // 3: one on top, centered over two.
        let three = pips(3);
        assert_eq!(rows(&three), 2);
        assert!(three[0].x.abs() < 1e-5 && three[0].y > three[1].y);
        // 4: a square.
        let four = pips(4);
        assert_eq!(rows(&four), 2);
        assert!((four[0].x + four[1].x).abs() < 1e-5);
        // 5: a square with one in the middle.
        let five = pips(5);
        assert!(five.iter().any(|s| s.length() < 1e-5));
        // 6: two rows of three.
        let six = pips(6);
        assert_eq!(rows(&six), 2);
        assert_eq!(six.iter().filter(|s| s.y > 0.0).count(), 3);

        let seven = yield_row(7, 0);
        assert_eq!(seven.icons.len(), 1);
        assert_eq!(seven.labels[0].0, "7");
        assert!(
            seven.labels[0].1.x > seven.icons[0].1.x,
            "the number follows the icon"
        );
        assert!(yield_row(0, 0).icons.is_empty());
    }

    #[test]
    fn a_yield_row_puts_food_left_of_production_and_stays_inside_the_hex() {
        for (food, production) in [(2, 1), (6, 6), (12, 5), (3, 0), (0, 4)] {
            let row = yield_row(food, production);
            let food_x = row.icons.iter().filter(|(i, _)| *i == MapIcon::Food);
            let production_x = row.icons.iter().filter(|(i, _)| *i == MapIcon::Production);
            let rightmost_food = food_x.map(|(_, at)| at.x).fold(f32::MIN, f32::max);
            let leftmost_production = production_x.map(|(_, at)| at.x).fold(f32::MAX, f32::min);
            assert!(rightmost_food < leftmost_production, "{food}/{production}");
            // The pill's corners stay inside the hex's fill.
            let corner = (YIELD_ROW_OFFSET - row.half).abs();
            let fill = HEX_SIZE * HEX_FILL_SCALE * 3f32.sqrt();
            assert!(
                3f32.sqrt() * corner.x + corner.y <= fill,
                "{food}/{production}"
            );
        }
    }

    #[test]
    fn an_unseen_unit_leaves_its_hex_in_the_green_move_range() {
        let (mut game, cavalry, hidden) = behind_the_mountain();
        glance_at(&mut game, cavalry, hidden);
        game.selected = Some(cavalry);
        let empty = count_color(&game.build_vertices(), MOVE_RANGE_COLOR);
        assert!(empty > 0);
        game.units
            .push(Unit::new(2, hidden, Team::Red, UnitType::Melee));
        assert_eq!(count_color(&game.build_vertices(), MOVE_RANGE_COLOR), empty);
    }

    #[test]
    fn an_enemy_seen_before_leaves_no_trace_once_out_of_sight() {
        // A ranged unit (range 2) behind the mountain, so (2, 0) is in range
        // but out of sight: neither a ghost of the enemy seen there nor a
        // target highlight shows.
        let drawn = |enemy_seen: bool| {
            let (mut game, idx, hidden) = behind_the_mountain();
            game.units[idx].unit_type = UnitType::Ranged;
            game.units[idx].ability_queued = false;
            if enemy_seen {
                game.units
                    .push(Unit::new(2, hidden, Team::Red, UnitType::Melee));
            }
            glance_at(&mut game, idx, hidden);
            game.selected = Some(idx);
            scene(&game)
        };
        assert!(drawn(true) == drawn(false));
    }

    #[test]
    fn yield_badges_do_not_react_to_what_happens_out_of_sight() {
        let (mut game, city, far) = remembered_route_hex();
        assert!(game.show_yields && game.yields_city() == Some(city));
        let before = scene(&game);
        game.units
            .push(Unit::new(51, far, Team::Red, UnitType::Melee));
        assert_eq!(scene(&game), before);
        game.sites.insert(
            far,
            crate::game::city::Site {
                team: Team::Red,
                food: 9,
                production: 9,
                label: "FARM",
            },
        );
        assert_eq!(scene(&game), before);
    }

    #[test]
    fn only_hovering_the_manager_draws_a_line_and_it_follows_the_route_home() {
        let mut game = GameState::city_scenario();
        let city = game
            .cities
            .iter()
            .position(|c| c.team == PLAYER_TEAM)
            .unwrap();
        game.selected_city = Some(city);
        let fog = game.fog();
        let routes = game.known_routes(city, &fog);
        // A manager at least two legs from the city, so the route bends
        // through other hexes rather than being one straight hop.
        let manager = routes
            .costs
            .keys()
            .copied()
            .filter(|&h| {
                !game.grid.terrain(h).is_water()
                    && !game.cities[city].worked.contains(&h)
                    && routes.path_from(h).len() >= 3
            })
            .min_by_key(|h| (h.q, h.r))
            .expect("a land tile two legs from the city");
        game.cities[city].worked[0] = manager;
        assert!(
            game.cities[city].worked.len() > 1,
            "the manager has workers"
        );
        let path = routes.path_from(manager);
        assert_eq!(path.first(), Some(&manager));
        assert_eq!(path.last(), Some(&game.cities[city].pos));
        assert!(path.windows(2).all(|leg| {
            leg[0].distance(leg[1]) == 1 && routes.costs[&leg[1]] < routes.costs[&leg[0]]
        }));

        let city_map = |game: &GameState| {
            let mut out = Vec::new();
            game.push_city_map(&fog, &mut out);
            out
        };
        let route_vertices = |out: &[Vertex]| {
            out.iter()
                .filter(|v| v.color == MANAGER_ROUTE_COLOR)
                .map(|v| Vec2::new(v.pos[0], v.pos[1]))
                .collect::<Vec<_>>()
        };

        // Not hovering: no route line, and workers add only their rings,
        // no links to the manager.
        game.hovered_tile = None;
        let with_workers = city_map(&game);
        assert!(route_vertices(&with_workers).is_empty());
        let workers = game.cities[city].worked.len() - 1;
        let alone = {
            let mut alone = game.clone();
            alone.cities[city].worked.truncate(1);
            city_map(&alone)
        };
        let mut ring = Vec::new();
        mesh::polygon_outline(Vec2::ZERO, 1.0, 0.1, 6, 0.0, LABEL_COLOR, &mut ring);
        mesh::polygon_outline(Vec2::ZERO, 1.0, 0.1, 6, 0.0, LABEL_COLOR, &mut ring);
        assert_eq!(with_workers.len() - alone.len(), workers * ring.len());

        // Hovering another worked tile draws nothing either.
        game.hovered_tile = Some(game.cities[city].worked[1]);
        assert!(route_vertices(&city_map(&game)).is_empty());

        // Hovering the manager: dashes along each leg of its route and
        // nowhere else.
        game.hovered_tile = Some(manager);
        let dashes = route_vertices(&city_map(&game));
        let mut one_leg = Vec::new();
        push_dotted_segment(Vec2::ZERO, Vec2::X, 0.06, LABEL_COLOR, &mut one_leg);
        assert_eq!(dashes.len(), (path.len() - 1) * one_leg.len());
        let near_leg = |p: Vec2, a: Hex, b: Hex| {
            let (a, b) = (a.to_world(), b.to_world());
            let t = ((p - a).dot(b - a) / (b - a).length_squared()).clamp(0.0, 1.0);
            p.distance(a.lerp(b, t)) <= MANAGER_ROUTE_WIDTH
        };
        assert!(
            dashes
                .iter()
                .all(|&p| path.windows(2).any(|leg| near_leg(p, leg[0], leg[1])))
        );
        for leg in path.windows(2) {
            assert!(dashes.iter().any(|&p| near_leg(p, leg[0], leg[1])));
        }

        // Carrying the manager to a new tile hides the line.
        game.moving_manager = Some(city);
        assert!(route_vertices(&city_map(&game)).is_empty());
    }

    #[test]
    fn barracks_delivery_labels_do_not_react_to_unseen_units() {
        let (mut game, city, far) = remembered_route_hex();
        // A barracks two hexes from the remembered hex: it sees only 1.
        let fog = game.fog();
        let site = game
            .grid
            .all_hexes()
            .filter(|&h| {
                h.distance(far) == 2
                    && game.site_available(city, crate::game::city::Building::Barracks, h)
            })
            .find(|&h| !fog.sees(far) && game.routes_from(PLAYER_TEAM, h).costs.contains_key(&far))
            .expect("a barracks site whose goods reach the hex");
        game.cities[city].barracks = Some(site);
        game.selected_city = None;
        game.selected_barracks = Some(city);
        assert!(!game.fog().sees(far));
        let before = scene(&game);
        game.units
            .push(Unit::new(51, far, Team::Red, UnitType::Melee));
        assert_eq!(scene(&game), before);
    }
}
