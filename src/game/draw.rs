//! Builds each frame's geometry from the game state.

use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, TAU};

use glam::Vec2;

use super::city::{Stock, resource_icon};
use super::fast_hash::{HashMap, HashSet};
use super::fog::{Fog, SeenBuilding, SeenJob};
use super::hex::{HEX_SIZE, Hex, HexGrid, edge, edge_corners};
use super::map_icons::{
    self, IMPROVEMENT_SPOT, LANDMARK_SCALE, LANDMARK_SPOT, MapIcon, RESOURCE_SPOT,
};
use super::orders::ClickMode;
use super::ruins::RUIN_HOLD_TURNS;
use super::terrain::{Feature, Terrain, Tile};
use super::turn::{Phase, step_rank};
use super::unit::{Team, Unit, UnitStats};
use super::unit_icons::{self, UnitIcon};
use super::workers::{Structure, StructureKind, WorkerJob};
use super::{GameState, font, mesh};
use crate::renderer::Vertex;

use super::mesh::Color;

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
/// The veil over a remembered hex's border where it faces a never-seen hex:
/// the border's color already darkened by `REMEMBERED_TINT`, drawn opaque,
/// because the bands of two hexes meeting a never-seen one overlap at their
/// shared corner, and a translucent veil would darken twice there.
const REMEMBERED_BORDER_COLOR: Color = [
    BORDER_COLOR[0] * (1.0 - REMEMBERED_TINT[3]),
    BORDER_COLOR[1] * (1.0 - REMEMBERED_TINT[3]),
    BORDER_COLOR[2] * (1.0 - REMEMBERED_TINT[3]),
    1.0,
];
/// The row of pips beside ruins counting the turns they've been held: where
/// the first sits from the hex's center, and the step to the next.
const RUIN_PIPS: Vec2 = Vec2::new(-0.06, -0.7);
const RUIN_PIP_RADIUS: f32 = 0.05;
const RUIN_PIP_GAP: f32 = 0.13;
/// Thin rims inside hexes worth scouting for: special tiles and ruins, and
/// animal dens.
const SPECIAL_RIM_COLOR: Color = [0.95, 0.66, 0.12, 1.0];
const RUIN_RIM_COLOR: Color = [0.62, 0.58, 0.50, 1.0];
const DEN_RIM_COLOR: Color = [0.36, 0.20, 0.09, 1.0];
const LANDMARK_RIM_WIDTH: f32 = 0.05;
/// Distance between cloud banks, in world units (a hex is 1 from center to corner).
const CLOUD_SPACING: f32 = 2.8;
/// The widest window, width over height, the fog is built for (a 21:9
/// ultrawide); a narrower one just draws a little fog off its sides.
const MAX_VIEW_ASPECT: f32 = 2.4;
/// Under the clouds, filling the gaps between puffs: the shade of their
/// undersides, so the fog reads as cloud all the way through.
const CLOUD_BASE_COLOR: Color = [0.10, 0.103, 0.123, 1.0];
/// The fog without clouds (the Fog setting's SOLID GREY).
const SOLID_FOG_COLOR: Color = [0.13, 0.135, 0.155, 1.0];
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
/// While something is being placed for a city's workers, the tint over tiles
/// they can reach...
const WORKER_REACH_TINT: Color = [0.95, 0.78, 0.42, 0.16];
/// ...and over the explored tiles they can't.
const OUT_OF_REACH_TINT: Color = [0.0, 0.0, 0.0, 0.45];
/// A queued wall or gate: an opaque, muted gold, so where edges meet the
/// rounded ends blend into one line instead of doubling up.
const PLANNED_EDGE_COLOR: Color = [0.52, 0.42, 0.24, 1.0];
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
/// Tile yields: below the unit spot, food, wood then metal as die-face pips
/// on a dark see-through pill.
const YIELD_ROW_OFFSET: Vec2 = Vec2::new(0.0, -0.56);
/// Distance between neighboring pips, across and up.
const YIELD_PIP_PITCH: Vec2 = Vec2::new(0.11, 0.15);
const YIELD_ICON_SCALE: f32 = 0.8;
/// How far a pip icon, at that scale, reaches from its center.
const YIELD_ICON_HALF: Vec2 = Vec2::new(0.05, 0.076);
/// Space between neighboring groups (food, wood, metal).
const YIELD_GROUP_GAP: f32 = 0.08;
const YIELD_ROW_PADDING: Vec2 = Vec2::new(0.06, 0.035);
/// The widest a yield pill may be, from its center, and stay inside the hex.
const YIELD_ROW_MAX_HALF_WIDTH: f32 = 0.46;
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
/// A job a worker is out on, brighter than one still queued.
const JOB_UNDER_WAY_COLOR: Color = [1.0, 0.88, 0.52, 1.0];
/// The armed worker job's preview on a tile where it can't go.
const BLOCKED_JOB_COLOR: Color = [0.90, 0.30, 0.25, 1.0];
/// A job on a tile is a hexagon nested inside the worked-tile ring
/// (`WORKED_OUTLINE_RADIUS`), so a worked tile with a job shows both, and
/// clear of a guard's outline and a fort's ring inside it. It is open along
/// its top side, where the job's name goes.
const JOB_RING_RADIUS: f32 = HEX_SIZE * 0.70;
/// Where the job ring's open top side runs: its apothem.
const JOB_RING_TOP: f32 = JOB_RING_RADIUS * 0.866_025_4;
const JOB_RING_WIDTH: f32 = 0.06;
const PLANNED_JOB_RING_WIDTH: f32 = 0.05;
/// A job's name sits in its ring's open top...
const PLANNED_JOB_LABEL_OFFSET: Vec2 = Vec2::new(0.0, JOB_RING_TOP);
const PLANNED_JOB_LABEL_HEIGHT: f32 = 0.12;
/// ...on a dark plate reaching past the text by this much, so it reads over
/// the ring and any ground.
const JOB_LABEL_PADDING: Vec2 = Vec2::new(0.05, 0.03);
const JOB_LABEL_PLATE_COLOR: Color = [0.01, 0.01, 0.012, 0.8];
/// A tile's delivery share, atop the hex: its baseline's left end from the
/// hex's center, and its capital height.
const SHARE_LABEL_OFFSET: Vec2 = Vec2::new(-0.3, 0.52);
const SHARE_LABEL_HEIGHT: f32 = 0.18;
const SHARE_LABEL_COLOR: Color = [0.65, 0.85, 0.65, 1.0];
/// A road's dot and the segments joining it to its neighbors.
const ROAD_COLOR: Color = [0.65, 0.45, 0.24, 1.0];
/// A job's name on a tile showing its delivery share: just under the share.
const JOB_LABEL_UNDER_SHARE: Vec2 = Vec2::new(0.0, 0.40);
const WORKER_TAG_MIN: Vec2 = Vec2::new(-0.78, -0.58);
const WORKER_TAG_MAX: Vec2 = Vec2::new(-0.3, -0.32);
const WORKER_TAG_COLOR: Color = [0.03, 0.03, 0.04, 0.92];
/// A city waiting for the stockpile (`waiting_badge`): a tag over its tower,
/// from this height up, an icon for each resource it lacks.
const WAITING_TAG_BOTTOM: f32 = 0.44;
const WAITING_TAG_HEIGHT: f32 = 0.3;
const WAITING_ICON_PITCH: f32 = 0.21;
const WAITING_ICON_HEIGHT: f32 = 0.21;
const WAITING_TAG_RIM: Color = [0.98, 0.30, 0.20, 1.0];
/// Structures workers build.
const STONE_COLOR: Color = [0.24, 0.23, 0.21, 1.0];
/// How thick a wall or gate is along its hex edge.
const BARRIER_WIDTH: f32 = 0.16;
const MORTAR_COLOR: Color = [0.09, 0.085, 0.08, 1.0];
const WOOD_COLOR: Color = [0.40, 0.20, 0.07, 1.0];
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
/// A red reticle around a unit on alert: a ring outside any status rings,
/// with four ticks on the diagonals (clear of the turn-order badges and the
/// health bar).
const ALERT_COLOR: Color = [0.96, 0.26, 0.16, 1.0];
const ALERT_RING_RADIUS: f32 = UNIT_ICON_RADIUS * 1.45;
const ALERT_TICK_SPAN: (f32, f32) = (UNIT_ICON_RADIUS * 1.2, UNIT_ICON_RADIUS * 1.95);
const ALERT_WIDTH: f32 = 0.05;

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

/// Outline around each tile the hovered or selected city works; a job on the
/// tile nests inside it (`JOB_RING_RADIUS`).
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
/// Where a plan's last number sits from its ghost: below it, clear of the
/// pictogram.
const QUEUE_END_BADGE_OFFSET: Vec2 = Vec2::new(0.0, -0.36);
/// The orange hex outline on a click waiting to be repeated to replace a queue.
const QUEUE_REPLACE_OUTLINE_RADIUS: f32 = 0.9;
const QUEUE_REPLACE_OUTLINE_WIDTH: f32 = 0.08;
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
    #[cfg(test)]
    pub fn build_vertices(&self) -> Vec<Vertex> {
        self.build_scene(Vec::new())
    }

    /// `build_vertices` into `buffer`, reusing its memory: the app keeps one
    /// from frame to frame, as a scene is a few megabytes.
    pub fn build_vertices_into(&self, buffer: &mut Vec<Vertex>) {
        *buffer = self.build_scene(std::mem::take(buffer));
    }

    fn build_scene(&self, mut out: Vec<Vertex>) -> Vec<Vertex> {
        out.clear();
        if let Some(city) = self.interior_view {
            out.extend(self.build_interior_vertices(city));
            return out;
        }
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
                    self.known_reachable_for_domain(
                        unit.pos,
                        stats.move_range,
                        unit.team,
                        &fog,
                        unit.is_naval(),
                    )
                } else {
                    HashSet::default()
                },
                locked: self.rival_of(idx).is_some(),
                swapping: self.ui_click_mode == Some(ClickMode::Swap),
            }
        });

        // The fog sits behind the map: a flat fill over every hex, and the
        // fixed cloud field on top of it unless the player chose solid fog.
        // Known terrain painted afterward hides it without clipping or
        // rebuilding around sight.
        if self.fog_of_war {
            let cloud = self.settings.cloud_fog;
            let base = if cloud {
                CLOUD_BASE_COLOR
            } else {
                SOLID_FOG_COLOR
            };
            // Only what the camera may show: explored hexes cover the fill
            // anyway, and a big map has far more hexes than a view.
            let view = self.cloud_view();
            let in_view =
                |p: Vec2| p.cmpge(view.0 - Vec2::ONE).all() && p.cmple(view.1 + Vec2::ONE).all();
            for hex in self.grid.all_hexes() {
                if !self.is_explored(hex) && in_view(hex.to_world()) {
                    mesh::regular_polygon(
                        hex.to_world(),
                        OUTER_BORDER_RADIUS,
                        6,
                        0.0,
                        base,
                        &mut out,
                    );
                }
            }
            if cloud {
                push_cloud_banks(&self.grid, self.cloud_time, view, &mut out);
            }
        }

        // Never-seen hexes aren't drawn at all: the background shows there.
        // Of those, only what the camera may show.
        let explored: Vec<Hex> = self
            .grid
            .all_hexes()
            .filter(|&h| self.may_show(h) && self.is_explored(h))
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
            if let Some(special) = self.grid.special(hex) {
                push_landmark_rim(center, SPECIAL_RIM_COLOR, &mut out);
                let icon = MapIcon::special(special);
                let spot = center + LANDMARK_SPOT;
                map_icons::push_map_icon_scaled(spot, icon, LANDMARK_SCALE, &mut out);
            }
            self.push_known_ruin(hex, &fog, &mut out);
            self.push_known_den(hex, &fog, &mut out);
        }
        push_rivers(
            &self.grid,
            |h| self.may_show(h) && self.is_explored(h),
            &mut out,
        );

        self.push_city_map(&fog, &mut out);
        self.push_fog(&fog, &mut out);
        // Placing something for a city's workers: the tiles they can reach
        // (as the player knows the board) are lit, and the rest dimmed, so
        // the reach stands out.
        if self.placing_job.is_some() {
            for hex in self.grid.all_hexes().filter(|&h| self.is_explored(h)) {
                let reach = self.grid.is_passable(hex) && self.known_worker_reach(hex);
                let tint = if reach {
                    WORKER_REACH_TINT
                } else {
                    OUT_OF_REACH_TINT
                };
                mesh::regular_polygon(
                    hex.to_world(),
                    HEX_SIZE * HEX_FILL_SCALE,
                    6,
                    0.0,
                    tint,
                    &mut out,
                );
            }
        }
        self.push_turn_dim(&mut out);
        self.push_order_markers(&fog, &mut out);

        for (idx, unit) in self.units.iter().enumerate() {
            if !fog.shows(unit) {
                continue;
            }
            let (center, scale) = self.drawn_unit_layout(idx);
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
            if unit.plans_later_turns() && self.is_player_controlled(idx) {
                let text = crate::game::city::turns_icon(unit.plan_len() as i32);
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
                if city.team == self.local_team {
                    0.17
                } else {
                    0.23
                },
                if city.team == self.local_team {
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

    /// The world rectangle (min, max) the camera may show: its full height,
    /// and as wide as the widest window it's drawn in (`MAX_VIEW_ASPECT`),
    /// since the vertices are built without knowing the window's shape.
    pub(super) fn cloud_view(&self) -> (Vec2, Vec2) {
        let half = self.camera.half_height;
        let reach = Vec2::new(half * MAX_VIEW_ASPECT, half);
        (self.camera.center - reach, self.camera.center + reach)
    }

    /// Whether anything drawn on `hex` may be on screen (`cloud_view`, with
    /// room for what reaches past the hex, like its border and labels). The
    /// map's per-hex layers skip the rest: a big map has far more hexes than
    /// a view.
    fn may_show(&self, hex: Hex) -> bool {
        let (min, max) = self.cloud_view();
        let margin = Vec2::splat(2.0 * HEX_SIZE);
        let p = hex.to_world();
        p.cmpge(min - margin).all() && p.cmple(max + margin).all()
    }

    /// Ruins on `hex` as the player knows them: in sight, with a pip for
    /// each turn their holder has held them, in its color; out of sight, as
    /// last seen, without.
    fn push_known_ruin(&self, hex: Hex, fog: &Fog, out: &mut Vec<Vertex>) {
        let ruin = if fog.sees(hex) {
            match self.ruin_at(hex) {
                Some(ruin) => Some(ruin),
                None => return,
            }
        } else if self.memory.get(&hex).is_some_and(|seen| seen.ruin) {
            None
        } else {
            return;
        };
        push_landmark_rim(hex.to_world(), RUIN_RIM_COLOR, out);
        let spot = hex.to_world() + LANDMARK_SPOT;
        map_icons::push_map_icon_scaled(spot, MapIcon::Ruins, LANDMARK_SCALE, out);
        let Some(ruin) = ruin else { return };
        let color = ruin.holder.map_or(BORDER_COLOR, Team::color);
        for i in 0..RUIN_HOLD_TURNS {
            let pip = hex.to_world() + RUIN_PIPS + Vec2::new(i as f32 * RUIN_PIP_GAP, 0.0);
            let fill = if i < ruin.held { color } else { BORDER_COLOR };
            mesh::regular_polygon(pip, RUIN_PIP_RADIUS + 0.012, 6, 0.0, BORDER_COLOR, out);
            mesh::regular_polygon(pip, RUIN_PIP_RADIUS, 6, 0.0, fill, out);
        }
    }

    /// An animal den on `hex` as the player knows it (`animals.rs`): in
    /// sight, or out of sight as last seen. A dark rim and a paw print.
    fn push_known_den(&self, hex: Hex, fog: &Fog, out: &mut Vec<Vertex>) {
        let known = if fog.sees(hex) {
            self.den_at(hex).is_some()
        } else {
            self.memory.get(&hex).is_some_and(|seen| seen.den.is_some())
        };
        if known {
            push_landmark_rim(hex.to_world(), DEN_RIM_COLOR, out);
            let spot = hex.to_world() + LANDMARK_SPOT;
            map_icons::push_map_icon_scaled(spot, MapIcon::Paw, LANDMARK_SCALE, out);
        }
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
            .filter(|&h| self.may_show(h) && self.is_explored(h) && !fog.sees(h))
            .collect();
        let blank = |h: Hex| !self.grid.contains(h) || !self.is_explored(h);
        for &hex in &remembered {
            let center = hex.to_world();
            mesh::regular_polygon(center, HEX_SIZE, 6, 0.0, REMEMBERED_TINT, out);
            // Facing blank, the border reaches past the hex (see
            // OUTER_BORDER_RADIUS); veil that outer half too, so the band
            // is one shade: opaque, in the veiled border's own color, as
            // the bands of neighbors overlap at their shared corners.
            for n in hex.neighbors().into_iter().filter(|&n| blank(n)) {
                let (a, b) = edge_corners(hex, n);
                let grow = |p: Vec2| center + (p - center) * (OUTER_BORDER_RADIUS / HEX_SIZE);
                mesh::convex_polygon(&[a, b, grow(b), grow(a)], REMEMBERED_BORDER_COLOR, out);
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

/// A line from `a` to `b` with round ends, so lines meeting at a point join
/// smoothly.
fn push_rounded_segment(a: Vec2, b: Vec2, width: f32, color: Color, out: &mut Vec<Vertex>) {
    mesh::segment(a, b, width, color, out);
    for end in [a, b] {
        mesh::regular_polygon(end, width / 2.0, 12, 0.0, color, out);
    }
}

/// A thin rim just inside the fill of the hex at `center`, marking a
/// landmark (a special tile or ruins).
fn push_landmark_rim(center: Vec2, color: Color, out: &mut Vec<Vertex>) {
    let radius = HEX_SIZE * HEX_FILL_SCALE - LANDMARK_RIM_WIDTH / 2.0;
    let corners: Vec<Vec2> = (0..6)
        .map(|i| center + Vec2::from_angle(i as f32 * TAU / 6.0) * radius)
        .collect();
    mesh::outline(&corners, LANDMARK_RIM_WIDTH, color, out);
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

/// One cloud puff: a soft disc lit from above.
struct Puff {
    center: Vec2,
    radius: f32,
    /// Its opacity: less past the map's edge (`edge_fade`).
    alpha: f32,
}

/// The most puffs in one cloud bank; each has between 4 and this many.
const MAX_BANK_PUFFS: usize = 9;

/// The shape of the cloud bank at lattice point (`x`, `y`): its puffs'
/// places around the bank's center and their radii, in units of the bank's
/// scale. The bank grows from one puff: each next one buds off a puff
/// already placed, in any direction and overlapping it, so every bank is
/// its own irregular clump rather than a copy of one template.
fn bank_shape(x: i32, y: i32) -> ([(Vec2, f32); MAX_BANK_PUFFS], usize) {
    let hash =
        |i: usize, salt: u32| cloud_hash(x, y, salt ^ (i as u32 + 1).wrapping_mul(0x9E37_79B9));
    let count = 4 + (cloud_hash(x, y, 0x3C6E_F372) * (MAX_BANK_PUFFS - 3) as f32) as usize;
    let count = count.min(MAX_BANK_PUFFS);
    let mut puffs = [(Vec2::ZERO, 0.0); MAX_BANK_PUFFS];
    for i in 0..count {
        let radius = 0.2 + hash(i, 0x68E3_1DA4) * 0.16;
        puffs[i].1 = radius;
        if i > 0 {
            let (parent, parent_radius) = puffs[(hash(i, 0x1B87_3593) * i as f32) as usize % i];
            let angle = hash(i, 0x7E95_761E) * TAU;
            let reach = (parent_radius + radius) * (0.5 + hash(i, 0x2F1D_9A07) * 0.25);
            puffs[i].0 = parent + Vec2::from_angle(angle) * reach;
        }
    }
    // Centered on the bank, so a large bank doesn't lean off its place.
    let mean = puffs[..count].iter().map(|p| p.0).sum::<Vec2>() / count as f32;
    for puff in &mut puffs[..count] {
        puff.0 -= mean;
    }
    (puffs, count)
}

/// How fast the whole cloud field drifts, in world units a second (a hex is
/// 1 from center to corner): slow enough to read as weather, not motion.
const CLOUD_WIND: Vec2 = Vec2::new(0.09, 0.025);
/// Each puff swells and shrinks by this share of its radius...
const CLOUD_BILLOW: f32 = 0.06;
/// ...and wanders by this much (world units) around its place in the bank...
const CLOUD_WANDER: f32 = 0.12;
/// ...over about this many seconds, each puff at its own pace.
const CLOUD_BILLOW_PERIOD: f32 = 14.0;

impl GameState {
    /// Moves the fog's clouds on by `dt` seconds (`push_cloud_banks`). The
    /// app calls it every frame, except in screenshot mode.
    pub fn animate_clouds(&mut self, dt: f32) {
        // Wrapped long before f32 loses the precision a frame needs.
        self.cloud_time = (self.cloud_time + dt) % 100_000.0;
    }
}

/// How much of a cloud puff centered at `point` shows: all of it over the
/// map, fading out over the hexes beyond its edge, so puffs drifting past
/// the edge come and go smoothly.
fn edge_fade(grid: &HexGrid, point: Vec2) -> f32 {
    let hex = Hex::from_world(point);
    if grid.contains(hex) {
        return 1.0;
    }
    let nearest = hex
        .neighbors()
        .into_iter()
        .flat_map(|n| std::iter::once(n).chain(n.neighbors()))
        .filter(|&n| grid.contains(n))
        .map(|n| n.to_world().distance(point))
        .fold(f32::INFINITY, f32::min);
    1.0 - ((nearest - 1.0) / 1.2).clamp(0.0, 1.0)
}

/// Draws the unexplored map as banks of cumulus, those that reach into
/// `view` (a world rectangle, min and max): one bank per point of a
/// world-space lattice that drifts with `CLOUD_WIND` as `time` (seconds)
/// passes, each bank a soft shadow under an irregular clump of puffs
/// (`bank_shape`) that slowly billow. Every puff fades out at its edge and is lit from above
/// (`push_cloud_puff`); higher puffs are drawn first, so the lit top of a
/// lower billow overlaps the shaded underside of the one behind it. A bank
/// keeps its lattice point's hash as it drifts, so the pattern moves whole.
/// The work per puff is constant and cheap: no noise sampling per vertex.
fn push_cloud_banks(grid: &HexGrid, time: f32, view: (Vec2, Vec2), out: &mut Vec<Vertex>) {
    let (min, max) = grid.bounds();
    // A bank reaches about two spacings from its lattice point.
    let reach = Vec2::splat(2.0 * CLOUD_SPACING);
    let (min, max) = (min.max(view.0 - reach), max.min(view.1 + reach));
    if min.cmpgt(max).any() {
        return;
    }
    let drift = CLOUD_WIND * time;
    let (min, max) = (min - drift, max - drift);
    let min_x = (min.x / CLOUD_SPACING).floor() as i32 - 1;
    let max_x = (max.x / CLOUD_SPACING).ceil() as i32 + 1;
    let min_y = (min.y / CLOUD_SPACING).floor() as i32 - 1;
    let max_y = (max.y / CLOUD_SPACING).ceil() as i32 + 1;
    let mut shadows = Vec::new();
    let mut puffs = Vec::new();
    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let look = cached_bank(x, y);
            let (bank, scale) = (look.place + drift, look.scale);
            for puff in &look.puffs[..look.count] {
                let phase = puff.phase + time * puff.pace * TAU / CLOUD_BILLOW_PERIOD;
                let wander = Vec2::new(phase.cos(), (phase * 0.8).sin()) * CLOUD_WANDER;
                let center = bank + puff.place * scale + wander;
                let alpha = edge_fade(grid, center);
                if alpha <= 0.0 {
                    continue;
                }
                let radius = puff.size * scale * 1.5 * (1.0 + CLOUD_BILLOW * (phase * 1.3).sin());
                puffs.push(Puff {
                    center,
                    radius,
                    alpha,
                });
            }
            let shadow = bank + Vec2::new(0.04, -0.12) * scale;
            let alpha = edge_fade(grid, shadow);
            if alpha > 0.0 {
                shadows.push((shadow, alpha));
            }
        }
    }
    puffs.sort_unstable_by(|a, b| b.center.y.total_cmp(&a.center.y));
    out.reserve((shadows.len() + puffs.len()) * CLOUD_PUFF_VERTICES);
    for (center, alpha) in shadows {
        let color = with_alpha(CLOUD_SHADOW_COLOR, CLOUD_SHADOW_COLOR[3] * alpha);
        push_soft_disc(center, Vec2::splat(SHADOW_RADIUS), color, color, out);
    }
    for puff in &puffs {
        push_cloud_puff(puff, out);
    }
}

/// A cloud bank's fixed look, all from its lattice point's hash: where it
/// sits, how big it is, and its puffs.
#[derive(Clone, Copy)]
struct Bank {
    /// Its place before the wind moves it.
    place: Vec2,
    scale: f32,
    puffs: [BankPuff; MAX_BANK_PUFFS],
    count: usize,
}

/// A puff of a `Bank`: its place around the bank's center and radius, in
/// units of the bank's scale, and how it billows (its pace, and its phase at
/// time 0).
#[derive(Clone, Copy, Default)]
struct BankPuff {
    place: Vec2,
    size: f32,
    pace: f32,
    phase: f32,
}

fn bank(x: i32, y: i32) -> Bank {
    // Rows are offset by half a bank, and each bank wanders off its lattice
    // point, so the lattice doesn't show.
    let jitter = Vec2::new(
        cloud_hash(x, y, 0xA341_316C) - 0.5,
        cloud_hash(x, y, 0xC801_3EA4) - 0.5,
    ) * 0.7;
    let row_shift = if y.rem_euclid(2) == 1 { 0.5 } else { 0.0 };
    let (shape, count) = bank_shape(x, y);
    let mut puffs = [BankPuff::default(); MAX_BANK_PUFFS];
    for (i, (puff, &(place, size))) in puffs.iter_mut().zip(&shape[..count]).enumerate() {
        let salt = (i as u32 + 1).wrapping_mul(0x9E37_79B9);
        // Each puff billows at its own phase and pace.
        *puff = BankPuff {
            place,
            size,
            pace: 0.75 + cloud_hash(x, y, 0x2545_F491 ^ salt) * 0.5,
            phase: cloud_hash(x, y, 0x5851_F42D ^ salt) * TAU,
        };
    }
    Bank {
        place: (Vec2::new(x as f32 + row_shift, y as f32) + jitter) * CLOUD_SPACING,
        scale: CLOUD_SPACING * (0.8 + cloud_hash(x, y, 0xAD90_777D) * 0.5),
        puffs,
        count,
    }
}

/// `bank`, worked out once per lattice point: the clouds are rebuilt every
/// frame and it's most of their cost. The wind brings new points into view
/// slowly, so the cache is simply emptied if it ever gets big.
fn cached_bank(x: i32, y: i32) -> Bank {
    thread_local! {
        static BANKS: std::cell::RefCell<HashMap<(i32, i32), Bank>> = Default::default();
    }
    BANKS.with_borrow_mut(|banks| {
        if banks.len() > 20_000 {
            banks.clear();
        }
        *banks.entry((x, y)).or_insert_with(|| bank(x, y))
    })
}

/// A cloud bank's shadow's radius.
const SHADOW_RADIUS: f32 = 0.62 * CLOUD_SPACING;

/// Vertices in one cloud disc: a quad the shader rounds and feathers
/// (`soft_disc_uv`).
const CLOUD_PUFF_VERTICES: usize = 6;
/// The dark band under each cloud bank, fading out at its edge.
const CLOUD_SHADOW_COLOR: Color = [0.03, 0.03, 0.04, 0.55];
/// A puff's lit top and shaded underside (linear colors): muted, so the
/// clouds stay behind the map rather than competing with it.
const CLOUD_LIGHT: Color = [0.25, 0.26, 0.29, 0.96];
const CLOUD_DARK: Color = [0.10, 0.105, 0.125, 0.96];

fn push_cloud_puff(puff: &Puff, out: &mut Vec<Vertex>) {
    push_soft_disc(
        puff.center,
        Vec2::splat(puff.radius),
        with_alpha(CLOUD_LIGHT, CLOUD_LIGHT[3] * puff.alpha),
        with_alpha(CLOUD_DARK, CLOUD_DARK[3] * puff.alpha),
        out,
    );
}

/// An ellipse with half-axes `radii`, solid in the middle and fading to
/// transparent at its rim (`soft_disc_uv`), shaded from `light` at the
/// top to `dark` at the bottom.
fn push_soft_disc(center: Vec2, radii: Vec2, light: Color, dark: Color, out: &mut Vec<Vertex>) {
    // The light comes from above and a little to the left.
    let shade = |local: Vec2| {
        let t = (0.5 + 0.5 * local.dot(Vec2::new(-0.35, 0.94))).clamp(0.0, 1.0);
        let mut color = [0.0; 4];
        for (c, (l, d)) in color.iter_mut().zip(light.iter().zip(dark)) {
            *c = d + (l - d) * t;
        }
        color
    };
    let vertex = |x: f32, y: f32| {
        let local = Vec2::new(x, y);
        let p = center + local * radii;
        Vertex {
            pos: [p.x, p.y, 0.0],
            color: shade(local),
            uv: crate::renderer::soft_disc_uv([x, y]),
        }
    };
    let (bl, br, tr, tl) = (
        vertex(-1.0, -1.0),
        vertex(1.0, -1.0),
        vertex(1.0, 1.0),
        vertex(-1.0, 1.0),
    );
    out.extend([bl, br, tr, bl, tr, tl]);
}

impl GameState {
    /// Ghosts at queued move destinations, links between allies queued to
    /// swap, and diamonds on attacked hexes. Markers for units sharing a target
    /// hex are fanned out so each order stays visible. Units whose queue
    /// reaches past this turn show their numbered plan instead, and only while
    /// selected or hovered; a queue of this turn alone draws like plain orders.
    fn push_order_markers(&self, fog: &Fog, out: &mut Vec<Vertex>) {
        let swapping: HashSet<u32> = (0..self.units.len())
            .filter(|&i| self.swap_partner(i).is_some())
            .map(|i| self.units[i].id)
            .collect();

        // Where each moving unit's ghost is drawn, for its attack arc.
        let mut ghosts: HashMap<u32, Vec2> = HashMap::default();
        let plain_moves = group_by_target(&self.units, |u| {
            u.planned_move
                .filter(|_| !swapping.contains(&u.id) && !u.plans_later_turns() && fog.shows(u))
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
                .filter(|_| self.is_player_controlled(idx) && !unit.plans_later_turns())
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
        let plans: Vec<(&Unit, UnitLook)> = (0..self.units.len())
            .filter(|&i| selection.contains(&i) || hovered == Some(i))
            .filter(|&i| self.units[i].plans_later_turns() && self.is_player_controlled(i))
            .map(|i| (&self.units[i], self.unit_look(&self.units[i])))
            .collect();
        push_queue_plans(&plans, out);
        // A click waiting to be repeated to replace the selection's queue.
        if let Some(hex) = self.queue_replace_hex() {
            let rim = HEX_SIZE * QUEUE_REPLACE_OUTLINE_RADIUS;
            mesh::polygon_outline(
                hex.to_world(),
                rim,
                QUEUE_REPLACE_OUTLINE_WIDTH,
                6,
                0.0,
                ATTACK_ARC_COLOR,
                out,
            );
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
        // The team earlier in `Team::ALL` (the player, if either is) draws
        // on top.
        let rival = self.rival_of(idx).map(|r| self.units[r].team);
        let offset_y = if rival.is_none_or(|rival| unit.team < rival) {
            CONTESTED_OFFSET_Y
        } else {
            -CONTESTED_OFFSET_Y
        };
        (center + Vec2::new(0.0, offset_y), CONTESTED_SCALE)
    }

    /// Whether `point` (world space) is on unit `idx`'s token as
    /// `build_vertices` draws it: at `unit_layout`'s center and scale (larger
    /// while it's lit as having just acted), its disc or civilian hexagon
    /// out to the edge of its dark outline. The city view uses it to tell a
    /// click on a unit from a click on the tile under it.
    pub(super) fn unit_token_contains(&self, idx: usize, point: Vec2) -> bool {
        let unit = &self.units[idx];
        let (center, mut scale) = self.unit_layout(idx);
        if self.recent_actors.contains(&unit.id) {
            scale *= ACTED_SCALE;
        }
        let (sides, rotation, size) = token_shape(self.unit_look(unit));
        // As `mesh::polygon_outline` draws it: centered on the rim, its
        // corners pushed out further than its edges.
        let half_step = TAU / 2.0 / sides as f32;
        let outer =
            UNIT_ICON_RADIUS * scale * size + ICON_OUTLINE_WIDTH * scale / 2.0 / half_step.cos();
        // Inside a regular polygon: within its apothem along the normal of
        // the edge the point faces.
        let offset = point - center;
        let from_corner = (offset.to_angle() - rotation).rem_euclid(2.0 * half_step);
        offset.length() * (from_corner - half_step).cos() <= outer * half_step.cos()
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
            mesh::regular_polygon(h.to_world(), 0.12, 8, 0.0, ROAD_COLOR, out);
        }
        // Road segments join roads to each other and to cities, even though a
        // city center is drawn as its larger marker.
        let joins = |h: Hex| view.roads.contains(&h) || view.cities.iter().any(|c| c.0 == h);
        for &h in &view.roads {
            for n in h.neighbors() {
                let city = !view.roads.contains(&n);
                if joins(n) && (city || (h.q, h.r) < (n.q, n.r)) {
                    mesh::segment(h.to_world(), n.to_world(), 0.09, ROAD_COLOR, out);
                }
            }
        }
        // After the roads, which run through where a share sits (#215).
        let shares = self.push_delivery_shares(fog, out);
        if let Some(i) = self
            .hovered_city
            .or(self.selected_city)
            .filter(|&i| self.cities[i].team == self.local_team || fog.sees(self.cities[i].pos))
        {
            // Delivery labels show routes as the player knows them; the
            // worked-tile rings below show whether goods really arrive.
            let routes = self.routes(i);
            let known_routes = self.known_routes(i, fog);
            // A manager picked up takes its cluster with it until it's
            // placed: none show.
            let clusters = &self.cities[i].clusters;
            let several = clusters.len() > 1;
            for (k, cluster) in clusters.iter().enumerate() {
                if self.moving_manager == Some((i, k)) {
                    continue;
                }
                for (worker_index, h) in cluster.tiles().enumerate() {
                    let color = if routes.costs.contains_key(&h) {
                        if worker_index == 0 {
                            [1.0, 0.78, 0.20, 1.0]
                        } else {
                            [0.25, 1.0, 0.4, 1.0]
                        }
                    } else {
                        [1.0, 0.25, 0.2, 1.0]
                    };
                    // A colored ring on a slightly wider dark one, just inside
                    // the hex's fill so it doesn't blur into the grid lines.
                    let center = h.to_world();
                    let radius = WORKED_OUTLINE_RADIUS;
                    let rim = WORKED_OUTLINE_RIM_COLOR;
                    mesh::polygon_outline(
                        center,
                        radius,
                        WORKED_OUTLINE_RIM_WIDTH,
                        6,
                        0.0,
                        rim,
                        out,
                    );
                    mesh::polygon_outline(center, radius, WORKED_OUTLINE_WIDTH, 6, 0.0, color, out);
                }
                // The manager's mark: M, or with several clusters M1 to M4.
                let center = cluster.manager.to_world();
                if several {
                    let label = super::city::manager_label(k, clusters.len());
                    font::push_text_centered(center, 0.34, &label, LABEL_COLOR, out);
                } else {
                    font::push_glyph(center, 0.34, 'M', LABEL_COLOR, out);
                }
                if self.hovered_tile == Some(cluster.manager) {
                    // Hovering a manager traces the way its goods travel to
                    // the city center, as the player knows the board.
                    for leg in known_routes.path_from(cluster.manager).windows(2) {
                        let (from, to) = (leg[0].to_world(), leg[1].to_world());
                        push_dotted_segment(
                            from,
                            to,
                            MANAGER_ROUTE_WIDTH,
                            MANAGER_ROUTE_COLOR,
                            out,
                        );
                    }
                }
            }
        }
        if let Some(i) = self.selected_barracks
            && let Some(barracks) = self.cities[i].barracks
        {
            let routes = self.known_routes_from(self.cities[i].team, barracks, fog);
            let mut costs: Vec<_> = routes
                .costs
                .iter()
                .filter(|(h, _)| self.is_explored(**h))
                .collect();
            costs.sort_unstable_by_key(|(h, _)| (h.q, h.r));
            for (hex, cost) in costs {
                font::push_text(
                    hex.to_world() + SHARE_LABEL_OFFSET,
                    SHARE_LABEL_HEIGHT,
                    &format!("{}%", super::city::delivered_share(*cost) * 25),
                    [1.0, 0.70, 0.30, 1.0],
                    out,
                );
            }
            for source in self.cities[i].worked() {
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
        self.push_planned_jobs(&shares, out);
        self.push_others_construction(&view.construction, &shares, out);
        // Shared corner posts overlap: choose their painter order by edge,
        // independently of hash keys, capacity or insertion order.
        let mut barriers: Vec<_> = view.barriers.iter().collect();
        barriers.sort_unstable_by_key(|((a, b), _)| (a.q, a.r, b.q, b.r));
        for (&(a, b), barrier) in barriers {
            push_barrier(a, b, barrier.kind, barrier.team.color(), out);
        }
        // Where the armed worker job would go: an edge, or a ring on a tile,
        // red where it can't.
        if let (Some((hex, across)), Some(kind)) = (self.hovered_job, self.placing_job) {
            match across {
                Some(across) => {
                    let (start, end) = edge_corners(hex, across);
                    push_rounded_segment(start, end, BARRIER_WIDTH, PLANNED_JOB_COLOR_SOLID, out);
                }
                None => {
                    let color = if self.job_unavailable(hex, kind).is_none()
                        || self.job_taken(self.local_team, WorkerJob::on_tile(hex, kind))
                    {
                        PLANNED_JOB_COLOR_SOLID
                    } else {
                        BLOCKED_JOB_COLOR
                    };
                    push_job_ring(hex, JOB_RING_WIDTH, color, out);
                }
            }
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
            for building in super::city::Building::PLACEABLE {
                if building == super::city::Building::Barracks {
                    continue;
                }
                let Some(hex) = city.placed_site(building) else {
                    continue;
                };
                if city.team != self.local_team && !fog.sees(hex) {
                    continue;
                }
                let (badge, color) = building_badge(building);
                mesh::regular_polygon(hex.to_world(), 0.31, 4, FRAC_PI_4, color, out);
                font::push_glyph(hex.to_world(), 0.30, badge, LABEL_COLOR, out);
                if building == super::city::Building::CoastalBattery {
                    push_health_bar(hex.to_world(), city.coastal_battery_hp / 150.0, 0.62, out);
                }
            }
        }
        for (hex, city) in &view.cities {
            push_city_marker(hex.to_world(), city, out);
        }
        for city in self.cities.iter().filter(|c| c.team == self.local_team) {
            push_worker_count(city.pos.to_world(), city.workers, out);
        }
        // The player's cities that wait for the stockpile: the first item of
        // a queue can't be paid for this turn (`city_waits_for`).
        let shown: Vec<usize> = (0..self.cities.len())
            .filter(|&i| {
                let city = &self.cities[i];
                city.team == self.local_team
                    && self.may_show(city.pos)
                    && !(city.queue.is_empty() && city.barracks_queue.is_empty())
            })
            .collect();
        if !shown.is_empty() {
            let forecast = self.forecast(self.local_team);
            for i in shown {
                if let Some(short) = self.city_waits_for(&forecast, i) {
                    push_waiting_badge(self.cities[i].pos.to_world(), short, out);
                }
            }
        }
    }

    /// Each tile's delivery share to `shares_city` (the percentage atop
    /// the hex), as the player knows the routes: every explored tile in
    /// reach but those a building covers. Returns the tiles labeled, whose
    /// job names then sit below the share.
    fn push_delivery_shares(&self, fog: &Fog, out: &mut Vec<Vertex>) -> HashSet<Hex> {
        let mut labeled = HashSet::default();
        let Some(i) = self.shares_city() else {
            return labeled;
        };
        let known_routes = self.known_routes(i, fog);
        let mut costs: Vec<_> = known_routes.costs.iter().collect();
        costs.sort_unstable_by_key(|(h, _)| (h.q, h.r));
        for (h, cost) in costs {
            // Nothing is known of a tile never seen, delivery included.
            if !self.may_show(*h) || !self.is_explored(*h) || self.known_building_at(*h, fog) {
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
                h.to_world() + SHARE_LABEL_OFFSET,
                SHARE_LABEL_HEIGHT,
                &label,
                SHARE_LABEL_COLOR,
                out,
            );
            labeled.insert(*h);
        }
        labeled
    }

    /// Whether the player knows of a city or placed building on `hex`, which
    /// shows no yields (`closed_to_citizens`): their own always, anyone's in
    /// sight, and a remembered city or barracks.
    /// `known_building_at` but for city centers, which keep their chips:
    /// their own yield comes in on its own.
    fn known_building_at_off_center(&self, hex: Hex, fog: &Fog) -> bool {
        let center = self.cities.iter().any(|c| c.pos == hex)
            || self.remembered(hex).is_some_and(|seen| seen.city.is_some()) && !fog.sees(hex);
        !center && self.known_building_at(hex, fog)
    }

    fn known_building_at(&self, hex: Hex, fog: &Fog) -> bool {
        let own = self.cities.iter().any(|c| {
            c.team == self.local_team
                && (c.pos == hex
                    || super::city::Building::PLACEABLE
                        .iter()
                        .any(|&b| c.placed_site(b) == Some(hex)))
        });
        if own || fog.sees(hex) {
            return self.closed_to_citizens(hex);
        }
        self.remembered(hex)
            .is_some_and(|seen| seen.city.is_some() || seen.barracks.is_some())
    }

    /// Where a job's name goes on its tile: atop the hex, or just under the
    /// delivery share when the tile shows one.
    fn job_label_at(hex: Hex, shares: &HashSet<Hex>) -> Vec2 {
        let offset = if shares.contains(&hex) {
            JOB_LABEL_UNDER_SHARE
        } else {
            PLANNED_JOB_LABEL_OFFSET
        };
        hex.to_world() + offset
    }

    /// The player's worker jobs: those under way (`push_jobs_under_way`),
    /// and those queued, a faded ring on each tile, named. A job partly
    /// built (a worker left it) adds the turns of work it has left.
    fn push_planned_jobs(&self, shares: &HashSet<Hex>, out: &mut Vec<Vertex>) {
        self.push_jobs_under_way(shares, out);
        let queued = self
            .cities
            .iter()
            .filter(|c| c.team == self.local_team)
            .flat_map(|c| &c.worker_jobs);
        for job in queued {
            let left = (job.done > 0).then(|| {
                let left = self.job_turns_left(self.local_team, *job);
                format!(
                    "{} {}",
                    self.job_name(*job),
                    crate::game::city::turns_icon(left as i32)
                )
            });
            if let Some(across) = job.across {
                let (start, end) = edge_corners(job.hex, across);
                push_rounded_segment(start, end, BARRIER_WIDTH * 0.6, PLANNED_EDGE_COLOR, out);
                if let Some(label) = left {
                    let at = (start + end) / 2.0 + PLANNED_JOB_LABEL_OFFSET * 0.5;
                    let height = PLANNED_JOB_LABEL_HEIGHT;
                    font::push_text_centered(at, height, &label, PLANNED_JOB_COLOR, out);
                }
                continue;
            }
            push_job_ring(job.hex, PLANNED_JOB_RING_WIDTH, PLANNED_JOB_COLOR, out);
            push_job_label(
                Self::job_label_at(job.hex, shares),
                left.as_deref().unwrap_or(self.job_name(*job)),
                PLANNED_JOB_COLOR,
                out,
            );
        }
    }

    /// The jobs the player's workers are out on: a solid ring on the tile (or
    /// the edge, for a wall or gate) named with the job and, once the worker
    /// is there working, the turns of work left, like "IMPROVE" and the clock with 2.
    fn push_jobs_under_way(&self, shares: &HashSet<Hex>, out: &mut Vec<Vertex>) {
        let working = self
            .field_workers
            .iter()
            .filter(|w| w.team == self.local_team && !w.recalled);
        for worker in working {
            let Some(job) = worker.job else { continue };
            let label = match worker.work_left.filter(|_| worker.pos == job.hex) {
                Some(left) => format!(
                    "{} {}",
                    self.job_name(job),
                    crate::game::city::turns_icon(left as i32)
                ),
                None => self.job_name(job).into(),
            };
            let at = match job.across {
                Some(across) => {
                    let (start, end) = edge_corners(job.hex, across);
                    push_rounded_segment(start, end, BARRIER_WIDTH * 0.6, JOB_UNDER_WAY_COLOR, out);
                    (start + end) / 2.0 + PLANNED_JOB_LABEL_OFFSET * 0.5
                }
                None => {
                    push_job_ring(job.hex, JOB_RING_WIDTH, JOB_UNDER_WAY_COLOR, out);
                    Self::job_label_at(job.hex, shares)
                }
            };
            push_job_label(at, &label, JOB_UNDER_WAY_COLOR, out);
        }
    }

    /// Other sides' construction the player knows of (`MapView`): like the
    /// player's own jobs under way, a ring on the tile (or the edge, for a
    /// wall or gate) named with the job, but in the builder's color and
    /// without the turns left, which would give away its Workshop.
    fn push_others_construction(
        &self,
        jobs: &[SeenJob],
        shares: &HashSet<Hex>,
        out: &mut Vec<Vertex>,
    ) {
        for seen in jobs.iter().filter(|seen| self.may_show(seen.job.hex)) {
            let color = seen.team.color();
            let at = match seen.job.across {
                Some(across) => {
                    let (start, end) = edge_corners(seen.job.hex, across);
                    push_rounded_segment(start, end, BARRIER_WIDTH * 0.6, color, out);
                    (start + end) / 2.0 + PLANNED_JOB_LABEL_OFFSET * 0.5
                }
                None => {
                    push_job_ring(seen.job.hex, JOB_RING_WIDTH, color, out);
                    Self::job_label_at(seen.job.hex, shares)
                }
            };
            push_job_label(at, seen.name, color, out);
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
            let center = center + self.worker_glide(worker);
            if worker.team == self.local_team
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
            let own = city.team == self.local_team;
            let seen = |health| SeenBuilding {
                team: city.team,
                id: city.id,
                health,
                population: city.population,
            };
            if own || fog.sees(city.pos) {
                view.cities.push((city.pos, seen(1.0)));
            }
            if let Some(hex) = city.barracks.filter(|h| own || fog.sees(*h)) {
                let health = city.barracks_hp / super::city::BARRACKS_MAX_HP;
                view.barracks.push((hex, seen(health)));
            }
        }
        view.construction.extend(
            self.field_workers
                .iter()
                .filter(|w| fog.sees(w.pos))
                .filter_map(|w| self.others_construction(w, self.local_team)),
        );
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
            if let Some(city) = seen.city.filter(|c| c.team != self.local_team) {
                view.cities.push((h, city));
            }
            if let Some(barracks) = seen.barracks.filter(|b| b.team != self.local_team) {
                view.barracks.push((h, barracks));
            }
            view.construction.extend(
                seen.construction
                    .iter()
                    .filter(|seen| seen.team != self.local_team),
            );
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
    /// Other sides' construction, by the tile its worker stands on.
    construction: Vec<SeenJob>,
}

/// A job's ring on `hex` (`JOB_RING_RADIUS`): five sides of a hexagon, open
/// along the top where its name goes.
fn push_job_ring(hex: Hex, width: f32, color: Color, out: &mut Vec<Vertex>) {
    let center = hex.to_world();
    // From the top left corner around to the top right one, the long way.
    let corners: [Vec2; 6] = std::array::from_fn(|i| {
        center + Vec2::from_angle((i + 2) as f32 * TAU / 6.0) * JOB_RING_RADIUS
    });
    mesh::polyline(&corners, width, color, out);
}

/// A job's name centered on `at`, on a dark plate.
fn push_job_label(at: Vec2, text: &str, color: Color, out: &mut Vec<Vertex>) {
    let height = PLANNED_JOB_LABEL_HEIGHT;
    let radius = height / 2.0 + JOB_LABEL_PADDING.y;
    let reach =
        (font::world_text_width(text, height) / 2.0 + JOB_LABEL_PADDING.x - radius).max(0.0);
    push_pill(at, reach, radius, JOB_LABEL_PLATE_COLOR, out);
    font::push_text_centered(at, height, text, color, out);
}

fn building_badge(building: super::city::Building) -> (char, Color) {
    match building {
        super::city::Building::Barracks => ('B', [0.72, 0.35, 0.18, 1.0]),
        super::city::Building::Mill => ('M', [0.35, 0.65, 0.28, 1.0]),
        super::city::Building::Workshop => ('W', [0.38, 0.52, 0.82, 1.0]),
        super::city::Building::CanoeHouse => ('C', [0.30, 0.65, 0.82, 1.0]),
        super::city::Building::Forge => ('F', [0.82, 0.43, 0.22, 1.0]),
        super::city::Building::Stable => ('S', [0.67, 0.49, 0.27, 1.0]),
        super::city::Building::Watchpost => ('V', [0.78, 0.76, 0.35, 1.0]),
        super::city::Building::FieldHospital => ('H', [0.80, 0.33, 0.36, 1.0]),
        super::city::Building::Cannery => ('N', [0.44, 0.72, 0.47, 1.0]),
        super::city::Building::WorkCamp => ('K', [0.67, 0.55, 0.36, 1.0]),
        super::city::Building::Smelter => ('T', [0.82, 0.43, 0.29, 1.0]),
        super::city::Building::Railhead => ('R', [0.47, 0.68, 0.79, 1.0]),
        super::city::Building::Harbor => ('P', [0.34, 0.67, 0.90, 1.0]),
        super::city::Building::CoastalBattery => ('D', [0.85, 0.57, 0.28, 1.0]),
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
        // The cheap tests first: most hexes fail one.
        for hex in self.grid.all_hexes().filter(|&h| {
            self.may_show(h)
                && self.grid.terrain(h).is_workable()
                && (self.show_details
                    || reach.as_ref().is_some_and(|(city, routes)| {
                        routes.costs.contains_key(&h) || self.cities[*city].works(h)
                    }))
                && self.is_explored(h)
                && !self.known_building_at_off_center(h, fog)
        }) {
            let goods = self.known_yield(hex, fog);
            push_yield_row(hex.to_world() + YIELD_ROW_OFFSET, goods, out);
        }
    }
}

/// A tile's yields on a dark pill: food, wood then metal (`raw_yield`),
/// each laid out like the pips on a die, or as one icon and a number past
/// six. Nothing for a tile yielding nothing.
fn push_yield_row(center: Vec2, goods: (i32, i32, i32), out: &mut Vec<Vertex>) {
    let row = yield_row(goods);
    if row.icons.is_empty() {
        return;
    }
    let pill = rounded_rect(center, row.half, YIELD_PIP_CORNER);
    mesh::convex_polygon(&pill, YIELD_ROW_COLOR, out);
    for (icon, at) in row.icons {
        map_icons::push_map_icon_scaled(center + at, icon, YIELD_ICON_SCALE * row.scale, out);
    }
    for (text, at) in row.labels {
        font::push_text_centered(
            center + at,
            YIELD_DIGIT_HEIGHT * row.scale,
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
    /// How much the row was shrunk to fit its hex (1 if it fit).
    scale: f32,
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

/// Lays out a tile's food, wood and metal groups side by side, centered.
fn yield_row((food, wood, metal): (i32, i32, i32)) -> YieldRow {
    let mut row = YieldRow {
        icons: Vec::new(),
        labels: Vec::new(),
        half: Vec2::ZERO,
        scale: 1.0,
    };
    // Each group's contents around its own center, and its half extent.
    let groups: Vec<YieldRow> = [
        (MapIcon::Food, food),
        (MapIcon::Wood, wood),
        (MapIcon::Metal, metal),
    ]
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
                scale: 1.0,
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
            scale: 1.0,
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
    // Three full groups would reach past the hex: shrink the whole row.
    if row.half.x > YIELD_ROW_MAX_HALF_WIDTH {
        let shrink = YIELD_ROW_MAX_HALF_WIDTH / row.half.x;
        for (_, at) in &mut row.icons {
            *at *= shrink;
        }
        for (_, at) in &mut row.labels {
            *at *= shrink;
        }
        row.half *= shrink;
        row.scale = shrink;
    }
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
        push_rounded_segment(start, end, RIVER_WIDTH, RIVER_COLOR, out);
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

/// What `push_queue_plans` draws, worked out first: the plan lines, the ghosts
/// where plans end, the attack arrows, and the numbered badges (their text is
/// the turn numbers, "2,4" for one unit using a hex on two turns).
struct QueuePlans {
    lines: Vec<(Vec2, Vec2, Color)>,
    ghosts: Vec<(Vec2, UnitLook, Color)>,
    arrows: Vec<Vec<Vec2>>,
    badges: Vec<(Vec2, String, Color)>,
}

fn queue_plans(units: &[(&Unit, UnitLook)]) -> QueuePlans {
    // Which units stop on each hex, in order, to fan them apart.
    let mut visitors: Vec<(Hex, Vec<usize>)> = Vec::new();
    for (u, (unit, _)) in units.iter().enumerate() {
        for turn in 0..unit.plan_len() {
            let to = unit.pos_after(turn + 1);
            if unit.pos_after(turn) != to {
                match visitors.iter_mut().find(|(hex, _)| *hex == to) {
                    Some((_, list)) if list.contains(&u) => {}
                    Some((_, list)) => list.push(u),
                    None => visitors.push((to, vec![u])),
                }
            }
        }
    }
    let spot = |hex: Hex, u: usize| match visitors.iter().find(|(h, _)| *h == hex) {
        Some((_, list)) => {
            let slot = list.iter().position(|&v| v == u).unwrap_or(0);
            fan_position(hex.to_world(), slot, list.len(), GHOST_FAN_RADIUS)
        }
        None => hex.to_world(),
    };

    let mut plans = QueuePlans {
        lines: Vec::new(),
        ghosts: Vec::new(),
        arrows: Vec::new(),
        badges: Vec::new(),
    };
    let label = |turns: Vec<usize>| {
        let mut turns = turns;
        turns.sort_unstable();
        turns.dedup();
        let text: Vec<String> = turns.iter().map(usize::to_string).collect();
        text.join(",")
    };
    for (u, &(unit, look)) in units.iter().enumerate() {
        let color = unit.team.color();
        let line = with_alpha(color, QUEUE_LINE_ALPHA);
        let stand = |hex: Hex| {
            if hex == unit.pos {
                unit.pos.to_world()
            } else {
                spot(hex, u)
            }
        };
        let mut at = unit.pos.to_world();
        let mut stops: Vec<(Hex, Vec<usize>)> = Vec::new();
        let mut strikes: Vec<((Hex, Hex), Vec<usize>)> = Vec::new();
        for turn in 0..unit.plan_len() {
            let (from, to) = (unit.pos_after(turn), unit.pos_after(turn + 1));
            if from != to {
                let next = stand(to);
                plans.lines.push((at, next, line));
                at = next;
                add_label(&mut stops, to, turn + 1);
            }
            if let Some(target) = unit.attack_on_turn(turn) {
                add_label(&mut strikes, (to, target), turn + 1);
            }
        }
        let end = unit.plan_end();
        if end != unit.pos {
            plans
                .ghosts
                .push((stand(end), look, with_alpha(color, GHOST_ALPHA)));
        }
        for ((from, target), turns) in strikes {
            let points = attack_arc_points(stand(from), target.to_world(), 1.0);
            let middle = points
                .get(points.len() / 2)
                .copied()
                .unwrap_or(target.to_world());
            plans.badges.push((middle, label(turns), ATTACK_ARC_COLOR));
            plans.arrows.push(points);
        }
        for (hex, turns) in stops {
            // The number at the end sits under the ghost, not on it.
            let offset = if hex == end {
                QUEUE_END_BADGE_OFFSET
            } else {
                Vec2::ZERO
            };
            plans
                .badges
                .push((stand(hex) + offset, label(turns), color));
        }
    }
    plans
}

/// The plans of units following a queue, each unit's on its own: a line from
/// the unit through each hex it moves to, numbered by turn (1 is this turn),
/// ending in a faded ghost of the unit where its plan leaves it; its attacks
/// are arrows from where it stands that turn, numbered at their middle. Where
/// several units' plans stop on one hex, each unit's stop is fanned out
/// around it (as ghosts are), so a number never belongs to two units. A hex
/// or arrow one unit uses on several turns lists them ("2,4").
fn push_queue_plans(units: &[(&Unit, UnitLook)], out: &mut Vec<Vertex>) {
    let plans = queue_plans(units);
    for (from, to, color) in plans.lines {
        mesh::segment(from, to, QUEUE_LINE_WIDTH, color, out);
    }
    for (at, look, color) in plans.ghosts {
        push_unit_icon(at, look, 1.0, color, out);
    }
    for points in plans.arrows {
        push_arrow(&points, ATTACK_ARC_COLOR, ATTACK_ARC_OUTLINE_COLOR, out);
    }
    for (at, text, rim) in plans.badges {
        push_turn_badge(at, &text, 1.0, rim, out);
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

/// A unit's token (as on the map) sized so a military unit's disc has
/// `radius`, in whatever space `center` is in: the UI draws the unit strip
/// with it.
pub(super) fn push_unit_token(
    center: Vec2,
    look: UnitLook,
    radius: f32,
    color: Color,
    out: &mut Vec<Vertex>,
) {
    push_unit_icon(center, look, radius / UNIT_ICON_RADIUS, color, out);
}

/// A token's outline: its sides, rotation and size relative to a military
/// unit's disc. Drawing and the city view's click test share it.
fn token_shape(look: UnitLook) -> (u32, f32, f32) {
    if look.civilian {
        (6, FRAC_PI_2, CIVILIAN_TOKEN_SIZE)
    } else {
        (TOKEN_SIDES, 0.0, 1.0)
    }
}

/// The unit's token in its team color with a dark outline, so it reads on
/// any terrain, and its pictogram on top. Military units stand on a disc;
/// civilians on a hollow pointy-top hexagon (a pale center inside a
/// team-colored rim), unlike both the disc and the flat-top map hexes.
fn push_unit_icon(center: Vec2, look: UnitLook, scale: f32, color: Color, out: &mut Vec<Vertex>) {
    let (sides, rotation, size) = token_shape(look);
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
fn push_outlined_rects(rects: &[(Vec2, Vec2)], scale: f32, color: Color, out: &mut Vec<Vertex>) {
    let outline = with_alpha(ICON_OUTLINE_COLOR, color[3]);
    let grow = Vec2::splat(ICON_OUTLINE_WIDTH * scale);
    for &(min, max) in rects {
        mesh::quad(min - grow, max + grow, outline, out);
    }
    for &(min, max) in rects {
        mesh::quad(min, max, color, out);
    }
}

/// A city: a crenellated tower in its team's color with its population on
/// it.
fn push_city_marker(pos: Vec2, city: &SeenBuilding, out: &mut Vec<Vertex>) {
    push_city_tower(pos, 1.0, city.team.color(), out);
    // Centered in the tower's body, below the merlons.
    font::push_text_centered(
        pos + Vec2::new(0.0, -0.08),
        0.3,
        &city.population.to_string(),
        LABEL_COLOR,
        out,
    );
}

/// A city's crenellated tower in `color`, `scale` times its size on the map
/// (0.84 wide, 0.8 tall), in whatever space `pos` is in: the turn strip
/// draws it too.
pub(super) fn push_city_tower(pos: Vec2, scale: f32, color: Color, out: &mut Vec<Vertex>) {
    let rect = |x0: f32, y0: f32, x1: f32, y1: f32| {
        (
            pos + Vec2::new(x0, y0) * scale,
            pos + Vec2::new(x1, y1) * scale,
        )
    };
    let rects = [
        rect(-0.42, -0.4, 0.42, 0.24),
        // Three merlons along the top.
        rect(-0.42, 0.24, -0.24, 0.4),
        rect(-0.09, 0.24, 0.09, 0.4),
        rect(0.24, 0.24, 0.42, 0.4),
    ];
    push_outlined_rects(&rects, scale, color, out);
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

/// What a city waiting for the stockpile lacks (`city_waits_for`): a dark
/// tag over its tower, rimmed in red, with the icon of each resource it's
/// short of (`map_icons`), food, wood then metal.
fn push_waiting_badge(city: Vec2, short: Stock, out: &mut Vec<Vertex>) {
    let icons: Vec<MapIcon> = short
        .parts()
        .into_iter()
        .filter(|&(_, amount)| amount > 0)
        .filter_map(|(name, _)| map_icons::inline_icon(resource_icon(name)))
        .collect();
    if icons.is_empty() {
        return;
    }
    let width = WAITING_TAG_HEIGHT - WAITING_ICON_PITCH + icons.len() as f32 * WAITING_ICON_PITCH;
    let min = city + Vec2::new(-width / 2.0, WAITING_TAG_BOTTOM);
    let max = min + Vec2::new(width, WAITING_TAG_HEIGHT);
    let edge = Vec2::splat(ICON_OUTLINE_WIDTH / 2.0);
    mesh::quad(min - edge, max + edge, WAITING_TAG_RIM, out);
    mesh::quad(min, max, WORKER_TAG_COLOR, out);
    let middle = (min.y + max.y) / 2.0;
    let first = min.x + WAITING_TAG_HEIGHT / 2.0;
    for (k, icon) in icons.into_iter().enumerate() {
        let center = Vec2::new(first + k as f32 * WAITING_ICON_PITCH, middle);
        // The icons are laid out about a fifth of a hex tall.
        map_icons::push_map_icon_scaled(center, icon, WAITING_ICON_HEIGHT * 5.0 / HEX_SIZE, out);
    }
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
            push_outlined_rects(&[(at(-0.15, 0.03), at(0.15, 0.24))], 1.0, WOOD_COLOR, out);
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
    push_outlined_rects(&[walls], 1.0, color, out);
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

/// The reticle of a unit on alert (`Unit::alert`): a red ring with a tick on
/// each diagonal, unlike Guard's white hex.
fn push_alert_reticle(center: Vec2, scale: f32, out: &mut Vec<Vertex>) {
    mesh::polygon_outline(
        center,
        ALERT_RING_RADIUS * scale,
        ALERT_WIDTH * scale,
        32,
        0.0,
        ALERT_COLOR,
        out,
    );
    let (inner, outer) = ALERT_TICK_SPAN;
    for degrees in [45.0_f32, 135.0, 225.0, 315.0] {
        let direction = Vec2::from_angle(degrees.to_radians());
        mesh::segment(
            center + direction * inner * scale,
            center + direction * outer * scale,
            ALERT_WIDTH * 1.4 * scale,
            ALERT_COLOR,
            out,
        );
    }
}

/// Status rings behind the icon. They're filled discs, so only the rim shows
/// once the icon is drawn on top; the larger one goes first so both stay visible.
/// A guarding unit also gets a hex outline around everything, and a unit on
/// alert a red reticle.
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
    if unit.alert {
        push_alert_reticle(center, scale, out);
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
        push_rank_badge(pos, step_rank(unit.unit_type, phase), scale, color, out);
    }
}

/// Preserve single-digit ink centering; future ranks also support multiple digits.
fn push_rank_badge(pos: Vec2, rank: u32, scale: f32, color: Color, out: &mut Vec<Vertex>) {
    let text = rank.to_string();
    let height = BADGE_DIGIT_HEIGHT * scale;
    let radius = (BADGE_RADIUS * scale).max(font::world_text_width(&text, height) / 2.0);
    mesh::regular_polygon(pos, radius, 12, 0.0, BADGE_BG_COLOR, out);
    if rank < 10 {
        font::push_glyph(pos, height, char::from(b'0' + rank as u8), color, out);
    } else {
        font::push_text_centered(pos, height, &text, color, out);
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
    use crate::game::PLAYER_TEAM;
    use crate::game::fog::tests::{behind_the_mountain, glance_at, remembered_route_hex};
    use crate::game::unit::{Unit, UnitType};

    #[test]
    fn order_badges_render_multidigit_ranks_without_panicking() {
        for rank in [0, 9, 10, 100, u32::MAX] {
            let mut vertices = Vec::new();
            push_rank_badge(Vec2::ZERO, rank, 1.0, LABEL_COLOR, &mut vertices);
            let glyphs = vertices.iter().filter(|v| v.color == LABEL_COLOR).count();
            assert_eq!(glyphs, rank.to_string().len() * 6);
            assert!(vertices.iter().all(|v| v.pos.iter().all(|x| x.is_finite())));
        }
    }

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
        assert!(vertices.iter().all(|v| {
            [REMEMBERED_TINT, REMEMBERED_BORDER_COLOR, FOG_EDGE_COLOR].contains(&v.color)
        }));
        game.fog_of_war = false;
        vertices.clear();
        game.push_fog(&game.fog(), &mut vertices);
        assert!(vertices.is_empty());
    }

    #[test]
    fn remembered_hexes_meeting_the_unexplored_veil_their_shared_corner_once() {
        let mut game = GameState::world_scenario(3);
        game.memory.clear();
        let fog = game.fog();
        // Two neighbors remembered out of sight, and a third hex touching
        // both never seen: their veils meet at the corner of all three.
        let a = game
            .grid
            .all_hexes()
            .find(|&h| {
                !fog.sees(h)
                    && h.neighbors()
                        .iter()
                        .all(|n| game.grid.contains(*n) && !fog.sees(*n))
            })
            .expect("a hex out of sight");
        let b = a.neighbors()[0];
        let c = *a
            .neighbors()
            .iter()
            .find(|n| b.neighbors().contains(n))
            .unwrap();
        for hex in [a, b] {
            game.memory
                .insert(hex, super::super::fog::Sighting::default());
        }
        let corner = [edge_corners(a, c).0, edge_corners(a, c).1]
            .into_iter()
            .find(|p| {
                let (p0, p1) = edge_corners(b, c);
                p.distance(p0) < 1e-4 || p.distance(p1) < 1e-4
            })
            .unwrap();
        let point = corner + (c.to_world() - corner).normalize() * 0.03;
        // Only what the camera may show is drawn.
        game.camera.center = a.to_world();
        let mut vertices = Vec::new();
        game.push_fog(&fog, &mut vertices);
        let translucent_over = vertices
            .as_chunks::<3>()
            .0
            .iter()
            .filter(|t| t[0].color[3] < 1.0)
            .filter(|t| {
                let [p, q, r] = [0, 1, 2].map(|i| Vec2::new(t[i].pos[0], t[i].pos[1]));
                let sides = [
                    (q - p).perp_dot(point - p),
                    (r - q).perp_dot(point - q),
                    (p - r).perp_dot(point - r),
                ];
                sides.iter().all(|&s| s >= 0.0) || sides.iter().all(|&s| s <= 0.0)
            })
            .count();
        assert_eq!(translucent_over, 0, "no stacked veils past the corner");
        assert_eq!(top_color(&vertices, point), Some(REMEMBERED_BORDER_COLOR));
    }

    /// A view taking in any map.
    const WHOLE_WORLD: (Vec2, Vec2) = (Vec2::splat(-1.0e4), Vec2::splat(1.0e4));

    #[test]
    fn unexplored_cloud_geometry_stays_small() {
        let game = GameState::world_scenario(3);
        let clouds = |view| {
            let mut vertices = Vec::new();
            push_cloud_banks(&game.grid, 0.0, view, &mut vertices);
            vertices
        };
        // The view the game starts in: only the banks it takes in.
        let view = clouds(game.cloud_view());
        assert!(view.len() < 25_000, "{} cloud vertices in view", view.len());
        assert!(view.iter().any(|v| v.color[3] < 1.0));
        // Zoomed all the way out, the whole map. The former recursively
        // sampled mesh emitted well over 150,000 fog vertices on a smaller
        // map than this one.
        let whole = clouds(WHOLE_WORLD);
        assert!(whole.len() > view.len());
        assert!(whole.len() < 120_000, "{} cloud vertices", whole.len());
        // Nothing when the view is off the map.
        assert!(clouds((Vec2::splat(500.0), Vec2::splat(600.0))).is_empty());
    }

    #[test]
    fn the_fog_setting_picks_clouds_or_solid_grey() {
        let mut game = GameState::world_scenario(3);
        let soft = |vertices: &[Vertex]| vertices.iter().filter(|v| v.uv[0] <= -2.0).count();
        let flat = |vertices: &[Vertex]| vertices.iter().any(|v| v.color == SOLID_FOG_COLOR);
        assert!(game.settings.cloud_fog, "clouds by default");
        let clouds = game.build_vertices();
        assert!(soft(&clouds) > 0);
        assert!(!flat(&clouds));
        game.set_setting(crate::game::settings::Setting::FogStyle, 0);
        assert!(!game.settings.cloud_fog);
        assert_eq!(game.notice, "FOG: SOLID GREY");
        let solid = game.build_vertices();
        assert_eq!(soft(&solid), 0, "no cloud puffs");
        assert!(flat(&solid));
        // Without fog of war there is neither.
        game.fog_of_war = false;
        let clear = game.build_vertices();
        assert!(!flat(&clear));
        assert_eq!(soft(&clear), 0);
    }

    #[test]
    fn the_clouds_drift_and_billow_but_stay_over_the_map() {
        let game = GameState::world_scenario(3);
        let clouds = |time: f32| {
            let mut vertices = Vec::new();
            push_cloud_banks(&game.grid, time, WHOLE_WORLD, &mut vertices);
            vertices
        };
        let centers: Vec<Vec2> = game.grid.all_hexes().map(Hex::to_world).collect();
        let min = centers.iter().copied().reduce(Vec2::min).unwrap();
        let max = centers.iter().copied().reduce(Vec2::max).unwrap();
        let reach = Vec2::splat(2.5 * CLOUD_SPACING);
        let still = clouds(0.0);
        assert_eq!(
            still.len(),
            clouds(0.0).len(),
            "the same time, the same clouds"
        );
        for time in [0.5, 60.0, 600.0, 5000.0] {
            let later = clouds(time);
            assert!(
                later.len() < 120_000,
                "{} cloud vertices at {time} s",
                later.len()
            );
            let moved = still.iter().zip(&later).any(|(a, b)| a.pos != b.pos);
            assert!(moved, "the clouds move by {time} s");
            // Whatever has drifted past the map's edge fades out: nothing is
            // drawn more than a bank's width beyond it.
            for v in &later {
                let at = Vec2::new(v.pos[0], v.pos[1]);
                let inside = at.cmpge(min - reach).all() && at.cmple(max + reach).all();
                assert!(inside, "a cloud vertex at {at} is far off the map");
            }
        }
        let mut game = game;
        game.animate_clouds(2.0);
        game.animate_clouds(3.0);
        assert_eq!(game.cloud_time, 5.0);
    }

    #[test]
    fn a_puff_fades_out_past_the_map_edge() {
        let game = GameState::world_scenario(3);
        let inside = game.grid.all_hexes().next().unwrap().to_world();
        assert_eq!(edge_fade(&game.grid, inside), 1.0);
        let far = Vec2::new(1.0e4, 0.0);
        assert_eq!(edge_fade(&game.grid, far), 0.0);
    }

    #[test]
    fn placing_for_workers_tints_the_tiles_in_reach() {
        let mut game = GameState::city_scenario();
        game.explore();
        game.open_city(0);
        assert_eq!(count_color(&game.build_vertices(), WORKER_REACH_TINT), 0);
        game.arm_worker_job(crate::game::workers::JobKind::Road);
        let tinted = count_color(&game.build_vertices(), WORKER_REACH_TINT);
        let reachable = game
            .grid
            .all_hexes()
            .filter(|&h| {
                game.is_explored(h) && game.grid.is_passable(h) && game.known_worker_reach(h)
            })
            .count();
        assert!(reachable > 0);
        assert_eq!(
            tinted % reachable,
            0,
            "the same fill on each reachable tile"
        );
        assert!(tinted > 0);
    }

    #[test]
    fn queued_walls_are_drawn_with_round_ends() {
        let mut game = GameState::city_scenario();
        let city = game.cities[0].pos;
        let (a, b) = (city.neighbors()[0], city.neighbors()[1]);
        game.cities[0]
            .worker_jobs
            .push(crate::game::workers::WorkerJob {
                hex: a,
                kind: crate::game::workers::JobKind::Wall,
                across: Some(b),
                done: 0,
            });
        let vertices = game.build_vertices();
        // A segment is two triangles; each round end is twelve more.
        assert_eq!(count_color(&vertices, PLANNED_EDGE_COLOR), 6 + 2 * 12 * 3);
    }

    #[test]
    fn delivery_labels_show_only_on_explored_tiles() {
        let mut game = GameState::city_scenario();
        game.explore();
        game.select_city();
        let city = game.selected_city.unwrap();
        let label = SHARE_LABEL_COLOR;
        let seen = count_color(&game.build_vertices(), label);
        assert!(seen > 0, "labels on the explored tiles");
        // Forget an explored tile the known routes reach: its label goes.
        let fog = game.fog();
        let forgotten = game
            .known_routes(city, &fog)
            .costs
            .keys()
            .copied()
            .find(|&h| h != game.cities[city].pos && game.is_explored(h))
            .expect("an explored routed tile");
        game.memory.remove(&forgotten);
        assert!(!game.is_explored(forgotten));
        let fewer = count_color(&game.build_vertices(), label);
        assert!(fewer < seen, "{fewer} vs {seen}");
    }

    /// #215: a road through a tile runs where its share label sits, so the
    /// label is drawn after the road, whole on top of it.
    #[test]
    fn a_share_label_is_drawn_over_a_road_through_its_tile() {
        let mut game = GameState::city_scenario();
        game.units.clear();
        game.explore();
        game.open_city(0);
        // A road through a tile beside the city, on to the hex above it.
        let center = game.cities[0].pos;
        let tile = Hex::new(center.q + 1, center.r);
        let above = Hex::new(tile.q, tile.r + 1);
        game.roads.extend([tile, above]);
        let fog = game.fog();
        let mut out = Vec::new();
        game.push_city_map(&fog, &mut out);

        // The label's box, and the triangles of each color that touch it.
        let cost = game.known_routes(0, &fog).costs[&tile];
        let text = format!("{}%", super::super::city::delivered_share(cost) * 25);
        let min = tile.to_world() + SHARE_LABEL_OFFSET;
        let size = Vec2::new(
            font::world_text_width(&text, SHARE_LABEL_HEIGHT),
            SHARE_LABEL_HEIGHT,
        );
        let max = min + size;
        let touches = |t: &[Vertex; 3]| {
            let at = |v: &Vertex| Vec2::new(v.pos[0], v.pos[1]);
            let lo = t.iter().fold(Vec2::MAX, |m, v| m.min(at(v)));
            let hi = t.iter().fold(Vec2::MIN, |m, v| m.max(at(v)));
            lo.x < max.x && hi.x > min.x && lo.y < max.y && hi.y > min.y
        };
        let triangles = out.as_chunks::<3>().0;
        let indices = |color: Color| -> Vec<usize> {
            (0..triangles.len())
                .filter(|&i| triangles[i][0].color == color && touches(&triangles[i]))
                .collect()
        };
        let (road, label) = (indices(ROAD_COLOR), indices(SHARE_LABEL_COLOR));
        assert!(!road.is_empty(), "the road runs through the label");
        assert!(!label.is_empty(), "the tile shows its share");
        assert!(
            road.iter().max() < label.iter().min(),
            "the road's triangles {road:?} come before the label's {label:?}"
        );
    }

    #[test]
    fn work_under_way_is_ringed_and_counts_its_turns_left() {
        let mut game = GameState::city_scenario();
        game.explore();
        game.open_city(0);
        let city = game.cities[0].pos;
        // Two hexes out: a turn walking, then at work.
        let hex = game
            .grid
            .all_hexes()
            .filter(|&h| h.distance(city) == 2)
            .find(|&h| {
                game.job_unavailable(h, crate::game::workers::JobKind::Improve)
                    .is_none()
            })
            .unwrap();
        game.placing_job = Some(crate::game::workers::JobKind::Improve);
        assert!(game.place_job_at(hex, None));
        game.placing_job = None;
        let solid = |game: &GameState| count_color(&game.build_vertices(), JOB_UNDER_WAY_COLOR);
        assert_eq!(solid(&game), 0, "only queued, not under way");
        // Out and walking there: its name in solid gold.
        game.resolve_workers();
        let walking = solid(&game);
        assert!(walking > 0);
        assert_ne!(game.field_workers[0].pos, hex, "still walking");
        // At work: the clock and 3 after "IMPROVE", more shapes.
        game.resolve_workers();
        assert_eq!(game.field_workers[0].work_left, Some(3));
        assert!(solid(&game) > walking);
    }

    #[test]
    fn a_job_on_a_worked_tile_nests_inside_the_worked_ring() {
        use crate::game::workers::{FieldWorker, JobKind};
        let mut game = GameState::city_scenario();
        game.explore();
        game.open_city(0);
        let (manager, worker) = (
            game.cities[0].clusters[0].manager,
            game.cities[0].clusters[0].workers[0],
        );
        // A job under way on the manager's tile, one queued on the worker's.
        let job = WorkerJob::on_tile(manager, JobKind::Improve);
        game.field_workers.push(FieldWorker {
            id: 900,
            team: game.cities[0].team,
            home: 0,
            base: game.cities[0].pos,
            pos: manager,
            job: Some(job),
            work_left: Some(2),
            recalled: false,
        });
        game.cities[0]
            .worker_jobs
            .push(WorkerJob::on_tile(worker, JobKind::Road));
        let fog = game.fog();
        let mut out = Vec::new();
        game.push_city_map(&fog, &mut out);

        // Each ring's line, below the tile's center (clear of labels), shows
        // its own color on top: the job's doesn't cover the worked ring's.
        let manager_gold = [1.0, 0.78, 0.20, 1.0];
        let below = |hex: Hex, radius: f32| hex.to_world() - Vec2::new(0.0, radius * 0.866);
        assert_eq!(
            top_color(&out, below(manager, WORKED_OUTLINE_RADIUS)),
            Some(manager_gold)
        );
        assert_eq!(
            top_color(&out, below(manager, JOB_RING_RADIUS)),
            Some(JOB_UNDER_WAY_COLOR)
        );

        // The shapes' radii: every corner of a job's ring (solid, not its
        // name's glyphs) lies inside the worked ring's dark rim, on both tiles.
        let distances = |hex: Hex, color: Color| {
            out.iter()
                .filter(|v| v.color == color && v.uv == crate::renderer::SOLID_UV)
                .map(|v| Vec2::new(v.pos[0], v.pos[1]).distance(hex.to_world()))
                .filter(|&d| d < HEX_SIZE)
                .collect::<Vec<f32>>()
        };
        let rim_inside = |hex: Hex| {
            distances(hex, WORKED_OUTLINE_RIM_COLOR)
                .into_iter()
                .fold(f32::MAX, f32::min)
        };
        for (hex, color) in [(manager, JOB_UNDER_WAY_COLOR), (worker, PLANNED_JOB_COLOR)] {
            let ring = distances(hex, color);
            assert!(!ring.is_empty());
            let outermost = ring.into_iter().fold(0.0, f32::max);
            assert!(
                outermost < rim_inside(hex),
                "{outermost} vs {}",
                rim_inside(hex)
            );
        }
    }

    /// A worker of `team` at `job`'s tile: at work on it, or just arrived.
    fn worker_at(team: Team, job: WorkerJob, working: bool) -> crate::game::workers::FieldWorker {
        crate::game::workers::FieldWorker {
            id: 77,
            team,
            home: 0,
            base: job.hex,
            pos: job.hex,
            job: Some(job),
            work_left: working.then_some(2),
            recalled: false,
        }
    }

    /// Vertices of `color` for a job's ring (or edge) and its name: what
    /// another side's construction adds.
    fn construction_vertices(job: WorkerJob, name: &str, color: Color) -> usize {
        let mut out = Vec::new();
        match job.across {
            Some(across) => {
                let (start, end) = edge_corners(job.hex, across);
                push_rounded_segment(start, end, BARRIER_WIDTH * 0.6, color, &mut out);
            }
            None => push_job_ring(job.hex, JOB_RING_WIDTH, color, &mut out),
        }
        push_job_label(Vec2::ZERO, name, color, &mut out);
        count_color(&out, color)
    }

    #[test]
    fn another_sides_construction_in_sight_shows_in_its_color_named_without_turns() {
        use crate::game::city::Building;
        use crate::game::workers::JobKind;
        let (mut game, _, _) = behind_the_mountain();
        game.camera.center = Vec2::ZERO;
        let near = Hex::new(0, 1);
        assert!(game.fog().sees(near));
        let red = Team::Red.color();
        let colored = |game: &GameState, color: Color| count_color(&game.build_vertices(), color);

        // Red's worker just arrived shows only itself; at work, its
        // Barracks going up too: a red ring named BARRACKS, no turns left.
        let barracks = WorkerJob::on_tile(near, JobKind::Build(Building::Barracks));
        game.field_workers = vec![worker_at(Team::Red, barracks, false)];
        let idle = colored(&game, red);
        game.field_workers = vec![worker_at(Team::Red, barracks, true)];
        assert_eq!(
            colored(&game, red) - idle,
            construction_vertices(barracks, "BARRACKS", red)
        );
        assert_eq!(colored(&game, JOB_UNDER_WAY_COLOR), 0);

        // A wall on one of the tile's edges: the edge, named.
        let wall = WorkerJob {
            hex: near,
            kind: JobKind::Wall,
            across: Some(Hex::new(1, 1)),
            done: 0,
        };
        game.field_workers = vec![worker_at(Team::Red, wall, false)];
        let idle = colored(&game, red);
        game.field_workers = vec![worker_at(Team::Red, wall, true)];
        assert_eq!(
            colored(&game, red) - idle,
            construction_vertices(wall, "WALL", red)
        );

        // The player's own job is drawn as before, in bright gold with its
        // turns left, and not in team color.
        let blue = game.local_team.color();
        game.field_workers = vec![worker_at(game.local_team, barracks, false)];
        let idle = colored(&game, blue);
        game.field_workers = vec![worker_at(game.local_team, barracks, true)];
        assert_eq!(colored(&game, blue), idle);
        assert!(colored(&game, JOB_UNDER_WAY_COLOR) > 0);
    }

    #[test]
    fn another_sides_construction_out_of_sight_shows_only_as_last_seen() {
        use crate::game::workers::JobKind;
        let (mut game, cavalry, hidden) = behind_the_mountain();
        game.camera.center = Vec2::ZERO;
        let red = Team::Red.color();
        let job = WorkerJob::on_tile(hidden, JobKind::Fort);
        let colored = |game: &GameState| count_color(&game.build_vertices(), red);

        // Never seen: nothing of it shows.
        let before = scene(&game);
        game.field_workers = vec![worker_at(Team::Red, job, true)];
        game.explore();
        assert_eq!(scene(&game), before);

        // Seen at work, then out of sight: it stays as last seen, even
        // once the worker has gone.
        glance_at(&mut game, cavalry, hidden);
        let shown = construction_vertices(job, "FORT", red);
        assert_eq!(colored(&game), shown);
        game.field_workers.clear();
        game.explore();
        assert_eq!(colored(&game), shown);

        // Seen again with nobody at work there: gone.
        glance_at(&mut game, cavalry, hidden);
        assert_eq!(colored(&game), 0);
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
        // The unknown tile has no terrain geometry: only the fog's flat fill,
        // which translucent cloud puffs blend over.
        let beyond = unexplored.to_world();
        assert_eq!(top_color(&vertices, beyond), Some(CLOUD_BASE_COLOR));
    }

    #[test]
    fn a_building_placed_for_workers_shows_its_name_on_its_tile() {
        let mut game = GameState::city_scenario();
        game.units.clear();
        game.selected = None;
        game.explore();
        let site = Hex::new(-2, 0);
        let barracks = crate::game::city::Building::Barracks;
        let before = count_color(&game.build_vertices(), PLANNED_JOB_COLOR);
        game.open_city(0);
        game.queue_selected_city_building(barracks);
        assert!(game.place_job_at(site, None));
        game.leave_city_view();
        // A faded ring and its name, like any job waiting for a worker.
        assert!(count_color(&game.build_vertices(), PLANNED_JOB_COLOR) > before);
        let job = game.cities[0].worker_jobs[0];
        assert_eq!(game.job_name(job), "BARRACKS");
    }

    #[test]
    fn a_city_waiting_for_the_stockpile_shows_what_it_lacks_over_its_tower() {
        use crate::game::city::{Build, BuildUnit, Queued};
        let mut game = GameState::city_scenario();
        game.units.clear();
        game.selected = None;
        let rim = |game: &GameState| count_color(&game.build_vertices(), WAITING_TAG_RIM);
        assert_eq!(rim(&game), 0, "nothing queued");
        // Broke: a Melee at the head of the queue waits for wood.
        game.stockpiles[Team::Blue.index()] = Stock::default();
        game.cities[0].queue = vec![Queued::new(Build::Unit(BuildUnit::Melee))];
        let forecast = game.forecast(Team::Blue);
        let short = game.city_waits_for(&forecast, 0).expect("it waits");
        assert!(short.wood > 0, "{short:?}");
        assert!(rim(&game) > 0, "the badge shows");
        let one = rim(&game);
        // A Gather behind it doesn't stop it waiting: the head still does.
        game.cities[0].queue.push(Queued::new(Build::Gather));
        assert_eq!(rim(&game), one);
        // Paid for, the head no longer waits.
        game.cities[0].queue[0].paid = true;
        assert_eq!(rim(&game), 0);
        // Another side's waiting city shows nothing: its stockpile isn't
        // the player's to see.
        game.stockpiles[Team::Red.index()] = Stock::default();
        game.cities[1].queue = vec![Queued::new(Build::Unit(BuildUnit::Melee))];
        game.explore();
        assert_eq!(rim(&game), 0);
    }

    #[test]
    fn shared_corner_walls_have_identical_unsorted_vertices_on_every_frame() {
        let mut game = GameState::city_scenario();
        game.fog_of_war = false;
        let a = Hex::new(0, 0);
        let b = Hex::new(1, 0);
        let c = Hex::new(0, 1);
        for (neighbor, team) in [(b, Team::Blue), (c, Team::Red)] {
            game.barriers.insert(
                edge(a, neighbor),
                Structure {
                    kind: StructureKind::Wall,
                    team,
                },
            );
        }
        let vertices = |game: &GameState| {
            game.build_vertices()
                .into_iter()
                .map(|v| (v.pos, v.color, v.uv))
                .collect::<Vec<_>>()
        };
        let expected = vertices(&game);
        for _ in 0..20 {
            assert_eq!(vertices(&game), expected);
        }
        // Rebuild with a different backing capacity and reversed insertion order.
        let entries: Vec<_> = game.barriers.drain().collect();
        game.barriers.reserve(100);
        game.barriers.extend(entries.into_iter().rev());
        assert_eq!(vertices(&game), expected);
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
        assert!(
            count_color(&selected, ghost) > 0,
            "a ghost where the plan ends"
        );

        game.selected = None;
        let deselected = game.build_vertices();
        assert_eq!(count_color(&deselected, line), 0, "hidden once let go of");
        assert_eq!(count_color(&deselected, ghost), 0);

        game.hovered_tile = Some(start);
        let hovered = game.build_vertices();
        assert_eq!(count_color(&hovered, line), count_color(&selected, line));
    }

    #[test]
    fn two_units_passing_one_hex_get_their_own_numbers() {
        use crate::game::unit::TurnOrder;
        let (shared, a_first, b_last) = (Hex::new(0, 0), Hex::new(-1, 0), Hex::new(1, 0));
        // A reaches the shared hex on turn 2, B on turn 1 and moves on.
        let mut a = Unit::new(1, Hex::new(-2, 0), Team::Blue, UnitType::Melee);
        a.planned_move = Some(a_first);
        a.following_queue = true;
        a.queued.push(TurnOrder {
            from: a_first,
            move_to: Some(shared),
            attack: None,
        });
        let mut b = Unit::new(2, Hex::new(0, 2), Team::Blue, UnitType::Cavalry);
        b.planned_move = Some(shared);
        b.following_queue = true;
        b.queued.push(TurnOrder {
            from: shared,
            move_to: Some(b_last),
            attack: None,
        });
        let look = |unit: &Unit| UnitLook {
            icon: UnitIcon::of(unit.unit_type),
            civilian: false,
        };
        let plans = queue_plans(&[(&a, look(&a)), (&b, look(&b))]);

        let texts: Vec<&str> = plans.badges.iter().map(|b| b.1.as_str()).collect();
        assert!(!texts.contains(&"1,2"), "no number shared by two units");
        let near_shared: Vec<(Vec2, &str)> = plans
            .badges
            .iter()
            .filter(|b| b.0.distance(shared.to_world()) < HEX_SIZE * 0.8)
            .map(|b| (b.0, b.1.as_str()))
            .collect();
        assert_eq!(near_shared.len(), 2, "{near_shared:?}");
        assert_ne!(near_shared[0].0, near_shared[1].0, "fanned apart");
        // Each plan ends in its own ghost.
        assert_eq!(plans.ghosts.len(), 2);
        assert_ne!(plans.ghosts[0].0, plans.ghosts[1].0);
    }

    #[test]
    fn a_queue_of_this_turn_alone_draws_a_plain_ghost_and_arrow() {
        let mut game = GameState::new();
        game.fog_of_war = false;
        let melee = game
            .units
            .iter()
            .position(|u| u.team == Team::Blue && u.unit_type == crate::game::unit::UnitType::Melee)
            .unwrap();
        game.selected = Some(melee);
        game.hovered_tile = None;
        let first = game.units[melee].pos.neighbors()[0];
        let target = first.neighbors()[0];
        let line = with_alpha(Team::Blue.color(), QUEUE_LINE_ALPHA);
        let ghost = with_alpha(Team::Blue.color(), GHOST_ALPHA);
        let arrows = |game: &GameState| count_color(&game.build_vertices(), ATTACK_ARC_COLOR);
        let before = arrows(&game);

        // Shift-clicks that only fill this turn: a move, then an attack from there.
        assert!(game.queue_move(first) && game.queue_attack(target));
        assert!(
            game.units[melee].queued.is_empty(),
            "nothing past this turn"
        );
        let vertices = game.build_vertices();
        assert!(
            count_color(&vertices, ghost) > 0,
            "a ghost, not a numbered 1"
        );
        assert_eq!(count_color(&vertices, line), 0, "no plan line");
        assert!(arrows(&game) > before, "the attack's plain arrow");

        // A second turn switches to the numbered plan.
        let next = first
            .neighbors()
            .into_iter()
            .find(|&h| game.grid.is_passable(h) && game.units.iter().all(|u| u.pos != h))
            .expect("an open hex next to the first move");
        assert!(game.queue_move(next));
        let vertices = game.build_vertices();
        assert!(count_color(&vertices, line) > 0);
        assert!(
            count_color(&vertices, ghost) > 0,
            "the ghost moves to the plan's end"
        );
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
                push_yield_row(Vec2::ZERO, (2, 1, 0), &mut out);
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
            .filter(|&h| game.known_yield(h, &fog) != (0, 0, 0))
            .count();
        assert!(yielding > in_reach);
        assert_eq!(rows(&game), yielding, "a row on every tile that yields");
    }

    #[test]
    fn yields_up_to_six_are_die_pips_and_more_are_a_number() {
        let pips = |amount: i32| -> Vec<Vec2> {
            let row = yield_row((amount, 0, 0));
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

        let seven = yield_row((7, 0, 0));
        assert_eq!(seven.icons.len(), 1);
        assert_eq!(seven.labels[0].0, "7");
        assert!(
            seven.labels[0].1.x > seven.icons[0].1.x,
            "the number follows the icon"
        );
        assert!(yield_row((0, 0, 0)).icons.is_empty());
    }

    #[test]
    fn a_yield_row_puts_food_wood_and_metal_left_to_right_and_stays_inside_the_hex() {
        for goods in [
            (2, 1, 0),
            (6, 2, 0),
            (12, 1, 5),
            (3, 0, 0),
            (0, 1, 3),
            (1, 2, 1),
            (4, 1, 4),
        ] {
            let row = yield_row(goods);
            let span = |icon: MapIcon| {
                let xs: Vec<f32> = row
                    .icons
                    .iter()
                    .filter(|(i, _)| *i == icon)
                    .map(|(_, at)| at.x)
                    .collect();
                (
                    xs.iter().copied().fold(f32::MAX, f32::min),
                    xs.iter().copied().fold(f32::MIN, f32::max),
                )
            };
            let (food, wood, metal) = (
                span(MapIcon::Food),
                span(MapIcon::Wood),
                span(MapIcon::Metal),
            );
            assert!(food.1 < wood.0 && food.1 < metal.0, "{goods:?}");
            assert!(wood.1 < metal.0, "{goods:?}");
            let count = |icon| row.icons.iter().filter(|(i, _)| *i == icon).count();
            assert_eq!(count(MapIcon::Wood), goods.1.clamp(0, 6) as usize);
            // The pill's corners stay inside the hex's fill.
            let corner = (YIELD_ROW_OFFSET - row.half).abs();
            let fill = HEX_SIZE * HEX_FILL_SCALE * 3f32.sqrt();
            assert!(3f32.sqrt() * corner.x + corner.y <= fill, "{goods:?}");
        }
    }

    #[test]
    fn a_picked_up_manager_takes_its_citizens_off_the_map() {
        let mut game = GameState::city_scenario();
        game.units.clear();
        game.explore();
        game.open_city(0);
        assert!(game.cities[0].working() > 1);
        let citizen_ring = [0.25, 1.0, 0.4, 1.0];
        assert!(count_color(&game.build_vertices(), citizen_ring) > 0);
        let manager = game.cities[0].clusters[0].manager;
        game.city_click(manager);
        assert_eq!(game.moving_manager, Some((0, 0)));
        assert_eq!(count_color(&game.build_vertices(), citizen_ring), 0);
        // Cancelling puts them back.
        game.city_click(manager);
        assert!(count_color(&game.build_vertices(), citizen_ring) > 0);
    }

    #[test]
    fn placing_shows_the_citys_shares_with_yields_on_or_alt() {
        let mut game = GameState::city_scenario();
        game.units.clear();
        game.explore();
        game.show_yields = true;
        game.open_city(0);
        game.arm_worker_job(crate::game::workers::JobKind::Road);
        let city = 0;
        assert_eq!(game.yields_city(), Some(city));
        assert_eq!(game.shares_city(), Some(city));
        let fog = game.fog();
        let mut out = Vec::new();
        assert!(!game.push_delivery_shares(&fog, &mut out).is_empty());
        // Yields off: the shares only while Alt is held.
        game.show_yields = false;
        assert_eq!(game.shares_city(), None);
        game.set_details(true);
        assert_eq!(game.shares_city(), Some(city));
        game.set_details(false);
        game.press_escape();
        assert_eq!(game.shares_city(), None, "stopped placing");
    }

    #[test]
    fn a_building_hides_its_tiles_yields_and_a_job_name_clears_the_share() {
        let mut game = GameState::city_scenario();
        game.units.clear();
        game.explore();
        game.open_city(0);
        let fog = game.fog();
        let center = game.cities[0].pos;
        let barracks = center.neighbors()[0];
        let mut out = Vec::new();
        assert!(
            game.push_delivery_shares(&fog, &mut out)
                .contains(&barracks)
        );
        game.cities[0].barracks = Some(barracks);
        let mut out = Vec::new();
        let shares = game.push_delivery_shares(&fog, &mut out);
        assert!(!shares.contains(&barracks), "no share on a building");
        assert!(game.known_building_at_off_center(barracks, &fog));
        assert!(
            !game.known_building_at_off_center(center, &fog),
            "the center keeps its chips"
        );
        // A job's name on a tile with a share sits below it, clear of it.
        let tile = *shares.iter().next().unwrap();
        let label = GameState::job_label_at(tile, &shares);
        let share_bottom = tile.to_world().y + SHARE_LABEL_OFFSET.y;
        assert!(label.y + PLANNED_JOB_LABEL_HEIGHT / 2.0 < share_bottom);
        let bare = GameState::job_label_at(tile, &HashSet::default());
        assert_eq!(bare, tile.to_world() + PLANNED_JOB_LABEL_OFFSET);
    }

    #[test]
    fn yield_chips_split_production_into_wood_and_metal() {
        let mut game = GameState::city_scenario();
        game.units.clear();
        // Plains hills with a mine: a wood from the plains, the hill's and
        // the mine's metal.
        let mine = Hex::new(-2, 2);
        assert_eq!(game.sites[&mine].label, "MINE");
        let (food, wood, metal) = game.raw_yield(mine);
        assert_eq!((food, wood + metal), game.tile_yield(mine));
        assert_eq!(metal, 3);
        assert_eq!(wood, 1);
        // The city center: 2 food and 1 wood, whatever its ground.
        assert_eq!(game.raw_yield(game.cities[0].pos), (2, 1, 0));
        // A farm: food only.
        let farm = Hex::new(-4, 1);
        let (_, wood, metal) = game.raw_yield(farm);
        assert_eq!((wood, metal), (0, 0));
        let row = yield_row(game.raw_yield(mine));
        let metal_pips = row.icons.iter().filter(|(i, _)| *i == MapIcon::Metal);
        assert_eq!(metal_pips.count(), 3);
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
                    && !game.cities[city].works(h)
                    && routes.path_from(h).len() >= 3
            })
            .min_by_key(|h| (h.q, h.r))
            .expect("a land tile two legs from the city");
        game.cities[city].clusters[0].manager = manager;
        assert!(game.cities[city].working() > 1, "the manager has workers");
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
        let workers = game.cities[city].working() - 1;
        let alone = {
            let mut alone = game.clone();
            alone.cities[city].clusters[0].workers.clear();
            city_map(&alone)
        };
        let mut ring = Vec::new();
        mesh::polygon_outline(Vec2::ZERO, 1.0, 0.1, 6, 0.0, LABEL_COLOR, &mut ring);
        mesh::polygon_outline(Vec2::ZERO, 1.0, 0.1, 6, 0.0, LABEL_COLOR, &mut ring);
        assert_eq!(with_workers.len() - alone.len(), workers * ring.len());

        // Hovering another worked tile draws nothing either.
        game.hovered_tile = Some(game.cities[city].clusters[0].workers[0]);
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
        game.moving_manager = Some((city, 0));
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
