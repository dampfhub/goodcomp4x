//! Builds each frame's geometry from the game state.

use std::collections::{HashMap, HashSet};
use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, FRAC_PI_8, TAU};

use glam::Vec2;

use super::hex::{HEX_SIZE, Hex, HexGrid, edge_corners};
use super::orders::ClickMode;
use super::fog::{Fog, SeenBuilding};
use super::terrain::{Feature, Terrain, Tile};
use super::turn::{Phase, step_rank};
use super::unit::{Team, Unit, UnitStats, UnitType};
use super::{GameState, PLAYER_TEAM, font, mesh};
use crate::renderer::Vertex;

type Color = [f32; 4];

const BORDER_COLOR: Color = [0.10, 0.10, 0.13, 1.0];
/// Each hex's fill as a share of its size; the rest is the border between hexes.
const HEX_FILL_SCALE: f32 = 0.92;
/// The grey veil over remembered hexes out of sight, and the line (a grey a
/// little darker than the veil) where they meet hexes in sight.
const OUT_OF_SIGHT_COLOR: Color = [0.20, 0.20, 0.22, 0.38];
const FOG_EDGE_COLOR: Color = [0.12, 0.12, 0.13, 1.0];
/// The fog edge fills the whole gap between two hexes' fills: each fill stops
/// short of its hex's edge by (1 - HEX_FILL_SCALE) of the apothem, sqrt(3) / 2.
const FOG_EDGE_WIDTH: f32 = HEX_SIZE * (1.0 - HEX_FILL_SCALE) * 1.732_050_8;
/// Faint light puffs over the grey veil, for a look of cloud cover.
const CLOUD_COLOR: Color = [0.85, 0.87, 0.9, 0.07];
const CLOUD_PUFFS: usize = 3;
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

/// Sizes below are for a unit drawn at full scale, alone in its hex.
const UNIT_ICON_RADIUS: f32 = HEX_SIZE * 0.42;
const LABEL_HEIGHT: f32 = UNIT_ICON_RADIUS * 0.9;
/// The dark edge around unit icons and map markers, so they stand out on any
/// terrain.
const ICON_OUTLINE_COLOR: Color = [0.03, 0.03, 0.04, 1.0];
const ICON_OUTLINE_WIDTH: f32 = 0.06;
/// Map markers: the granary beside a city, and improvement badges.
const GRANARY_COLOR: Color = [0.95, 0.72, 0.22, 1.0];
const SITE_BADGE_OFFSET: Vec2 = Vec2::new(-0.5, 0.36);
/// Strategic resources: a gold-edged disc in the hex's top-right corner.
const RESOURCE_BADGE_OFFSET: Vec2 = Vec2::new(0.5, 0.36);
const RESOURCE_COLOR: Color = [0.95, 0.80, 0.35, 1.0];
const SITE_BADGE_HALF: f32 = 0.16;
const SITE_BADGE_COLOR: Color = [0.04, 0.04, 0.05, 0.92];
const FARM_COLOR: Color = [0.95, 0.80, 0.30, 1.0];
const MINE_COLOR: Color = [0.62, 0.62, 0.66, 1.0];
const WOOD_COLOR: Color = [0.62, 0.40, 0.20, 1.0];
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

/// A queued move is drawn as a faded copy of the unit at its destination.
const GHOST_ALPHA: f32 = 0.4;
const GHOST_FAN_RADIUS: f32 = HEX_SIZE * 0.3;

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
                    self.reachable_hexes(unit.pos, stats.move_range)
                } else {
                    HashSet::new()
                },
                locked: self.rival_of(idx).is_some(),
                swapping: self.ui_click_mode == Some(ClickMode::Swap),
            }
        });

        for hex in self.grid.all_hexes() {
            // Never-seen hexes are only the blank fog `push_fog` draws.
            if !self.is_explored(hex) {
                continue;
            }
            let center = hex.to_world();
            let fill = self.hex_fill(hex, selection.as_ref(), &fog);
            mesh::regular_polygon(center, HEX_SIZE, 6, 0.0, BORDER_COLOR, &mut out);
            mesh::regular_polygon(center, HEX_SIZE * HEX_FILL_SCALE, 6, 0.0, fill, &mut out);
            push_tile_symbols(center, self.grid.tile(hex), &mut out);
            if let Some(resource) = self.grid.resource(hex) {
                push_resource_badge(center + RESOURCE_BADGE_OFFSET, resource.glyph(), &mut out);
            }
        }
        push_rivers(&self.grid, |h| self.is_explored(h), &mut out);

        self.push_city_map(&fog, &mut out);
        self.push_remembered_units(&fog, &mut out);
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
            push_unit_icon(center, unit, look, icon_scale, color, &mut out);
            push_order_badges(center, unit, look, scale, &mut out);
            push_health_bar(center, unit.hp / unit.max_hp(), scale, &mut out);
        }

        self.push_tile_yields(&mut out);
        self.push_effects(&mut out);
        out
    }

    /// Veils remembered hexes out of sight in grey, and outlines each stretch
    /// of them in darker grey where it meets hexes in sight. Never-seen hexes need
    /// nothing: nothing is drawn on them, so the background shows. Drawn over
    /// the map and cities but under units and orders.
    fn push_fog(&self, fog: &Fog, out: &mut Vec<Vertex>) {
        if !self.fog_of_war {
            return;
        }
        let remembered: Vec<Hex> = self
            .grid
            .all_hexes()
            .filter(|h| self.is_explored(*h) && !fog.sees(*h))
            .collect();
        for &hex in &remembered {
            mesh::regular_polygon(hex.to_world(), HEX_SIZE, 6, 0.0, OUT_OF_SIGHT_COLOR, out);
        }
        for &hex in &remembered {
            push_cloud_puffs(hex, out);
        }
        for &hex in &remembered {
            for n in hex.neighbors() {
                if self.grid.contains(n) && fog.sees(n) {
                    let (a, b) = edge_corners(hex, n);
                    mesh::segment(a, b, FOG_EDGE_WIDTH, FOG_EDGE_COLOR, out);
                    // Round the joints where edges meet at a corner.
                    for p in [a, b] {
                        mesh::regular_polygon(p, FOG_EDGE_WIDTH / 2.0, 10, 0.0, FOG_EDGE_COLOR, out);
                    }
                }
            }
        }
    }

}

/// A few faint, soft puffs over a remembered hex, so the grey veil reads as
/// patchy cloud cover. Their size and place come from hashing the hex, so
/// they stay put frame to frame and differ between neighbors. They stay
/// inside the hex, clear of the fog's edge.
fn push_cloud_puffs(hex: Hex, out: &mut Vec<Vertex>) {
    let mut bits = (hex.q as u32).wrapping_mul(0x9E37_79B1) ^ (hex.r as u32).wrapping_mul(0x85EB_CA77);
    let mut next = || {
        bits ^= bits << 13;
        bits ^= bits >> 17;
        bits ^= bits << 5;
        (bits % 1000) as f32 / 1000.0
    };
    for _ in 0..CLOUD_PUFFS {
        let angle = next() * TAU;
        let offset = Vec2::from_angle(angle) * (0.1 + 0.25 * next());
        let radius = 0.25 + 0.15 * next();
        mesh::regular_polygon(hex.to_world() + offset, radius, 14, 0.0, CLOUD_COLOR, out);
    }
}

impl GameState {
    /// Ghosts at queued move destinations, links between allies queued to
    /// swap, and diamonds on attacked hexes. Markers for units sharing a target
    /// hex are fanned out so each order stays visible.
    fn push_order_markers(&self, fog: &Fog, out: &mut Vec<Vertex>) {
        let swapping: HashSet<u32> = (0..self.units.len())
            .filter(|&i| self.swap_partner(i).is_some())
            .map(|i| self.units[i].id)
            .collect();

        // Where each moving unit's ghost is drawn, for its attack arc.
        let mut ghosts: HashMap<u32, Vec2> = HashMap::new();
        let plain_moves = group_by_target(&self.units, |u| {
            u.planned_move
                .filter(|_| !swapping.contains(&u.id) && fog.shows(u))
        });
        for (hex, movers) in plain_moves {
            for (i, unit) in movers.iter().enumerate() {
                let pos = fan_position(hex.to_world(), i, movers.len(), GHOST_FAN_RADIUS);
                let color = with_alpha(unit.team.color(), GHOST_ALPHA);
                push_unit_icon(pos, unit, self.unit_look(unit), 1.0, color, out);
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
                .filter(|_| self.is_player_controlled(idx))
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
        let has_enemy = fog.sees(hex) && self.has_enemy_target_at(hex, sel.team);
        if self.is_occupied(hex) || has_enemy {
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
                    mesh::segment(h.to_world(), n.to_world(), 0.09, [0.65, 0.45, 0.24, 1.0], out);
                }
            }
        }
        if let Some(i) = self
            .hovered_city
            .or(self.selected_city)
            .filter(|&i| self.cities[i].team == PLAYER_TEAM || fog.sees(self.cities[i].pos))
        {
            let routes = self.routes(i);
            for (h, cost) in &routes.costs {
                if self.yields_city() != Some(i) {
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
            if !manager_is_moving && let Some(manager) = self.cities[i].worked.first() {
                for worker in self.cities[i].worked.iter().skip(1) {
                    push_dotted_segment(
                        manager.to_world(),
                        worker.to_world(),
                        0.045,
                        [0.78, 0.88, 0.62, 0.9],
                        out,
                    );
                }
                if self.hovered_tile == Some(*manager) {
                    // Hovering the manager exposes each delivery link back to
                    // the city center, alongside the percentage labels.
                    for source in &self.cities[i].worked {
                        push_dotted_segment(
                            source.to_world(),
                            self.cities[i].pos.to_world(),
                            0.06,
                            [0.35, 0.82, 1.0, 0.9],
                            out,
                        );
                    }
                }
            }
        }
        if let Some(i) = self.selected_barracks
            && let Some(barracks) = self.cities[i].barracks
        {
            let routes = self.routes_from(self.cities[i].team, barracks);
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
        for &(h, label, team) in &view.sites {
            push_site_badge(h.to_world() + SITE_BADGE_OFFSET, label, team, out);
        }
        for (hex, barracks) in &view.barracks {
            push_barracks_marker(hex.to_world(), barracks.team.color(), out);
            push_health_bar(hex.to_world(), barracks.health, 0.7, out);
        }
        // A Barracks under construction follows the map hover, with a bright
        // placement ring. The selected site remains as a faint preview until
        // the player confirms the finished building.
        for (i, city) in self.cities.iter().enumerate() {
            if city.planned_barracks.is_none() {
                continue;
            }
            let valid = |hex: Hex| {
                self.grid.is_passable(hex)
                    && self.is_explored(hex)
                    && !self.cities.iter().any(|c| c.pos == hex)
            };
            let preview = if self.placing_barracks == Some(i) {
                self.hovered_tile
                    .filter(|h| valid(*h))
                    .or(city.planned_barracks)
            } else {
                city.planned_barracks
            };
            if let Some(hex) = preview {
                let is_hovered = self.hovered_tile == Some(hex);
                let color = if is_hovered {
                    [1.0, 0.72, 0.20, 0.95]
                } else {
                    [0.85, 0.42, 0.18, 0.55]
                };
                mesh::polygon_outline(
                    hex.to_world(),
                    WORKED_OUTLINE_RADIUS,
                    0.09,
                    6,
                    0.0,
                    color,
                    out,
                );
                push_barracks_marker(hex.to_world(), with_alpha(city.team.color(), 0.5), out);
            }
        }
        for (hex, city) in &view.cities {
            push_city_marker(hex.to_world(), city, out);
            push_health_bar(hex.to_world(), city.health, 1.0, out);
        }
    }

    /// What `push_city_map` shows: the live map in sight, the memory of it
    /// elsewhere. The player's own cities and barracks always show.
    fn map_view(&self, fog: &Fog) -> MapView {
        let mut view = MapView::default();
        view.roads.extend(self.roads.iter().filter(|h| fog.sees(**h)));
        for (&h, site) in self.sites.iter().filter(|(h, _)| fog.sees(**h)) {
            view.sites.push((h, site.label, site.team));
        }
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
                view.cities.push((city.pos, seen(city.hp / super::city::CITY_MAX_HP)));
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
            if let Some((label, team)) = seen.site {
                view.sites.push((h, label, team));
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

    /// Other sides' units where they were last seen, on remembered hexes out
    /// of sight. Drawn before the fog, so its grey veil marks them as old.
    fn push_remembered_units(&self, fog: &Fog, out: &mut Vec<Vertex>) {
        if !self.fog_of_war {
            return;
        }
        for (&hex, seen) in self.memory.iter().filter(|(h, _)| !fog.sees(**h)) {
            // Two units last seen together were contesting the hex.
            let shared = seen.units.len() > 1;
            for (unit, look) in &seen.units {
                let (center, scale) = if shared {
                    let dy = if unit.team == Team::Blue {
                        CONTESTED_OFFSET_Y
                    } else {
                        -CONTESTED_OFFSET_Y
                    };
                    (hex.to_world() + Vec2::new(0.0, dy), CONTESTED_SCALE)
                } else {
                    (hex.to_world(), 1.0)
                };
                push_unit_icon(center, unit, *look, scale, unit.team.color(), out);
                push_health_bar(center, unit.hp / unit.max_hp(), scale, out);
            }
        }
    }
}

/// Roads, improvements, cities and barracks to draw.
#[derive(Default)]
struct MapView {
    roads: Vec<Hex>,
    sites: Vec<(Hex, &'static str, Team)>,
    cities: Vec<(Hex, SeenBuilding)>,
    barracks: Vec<(Hex, SeenBuilding)>,
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
    /// Yield badges around the open city while yields are shown, limited to
    /// its economic reach.
    fn push_tile_yields(&self, out: &mut Vec<Vertex>) {
        let Some(city) = self.yields_city() else {
            return;
        };
        let routes = self.routes(city);
        for hex in self
            .grid
            .all_hexes()
            .filter(|h| self.grid.terrain(*h).is_workable() && self.is_explored(*h))
        {
            if !routes.costs.contains_key(&hex) && !self.cities[city].worked.contains(&hex) {
                continue;
            }
            let (food, production) = self.raw_yield(hex);
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
                    mesh::segment(base + Vec2::new(dx, 0.0), base + Vec2::new(dx, 0.13), 0.02, REED_COLOR, out);
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

/// How a unit is drawn, beyond its type: its letter, and whether it's a
/// civilian (settler or worker), drawn hollow.
#[derive(Clone, Copy)]
pub(super) struct UnitLook {
    pub letter: char,
    pub civilian: bool,
}

/// The icon's side count, rotation and size (relative to the standard
/// icon): a distinct silhouette for each kind of unit.
fn icon_shape(unit: &Unit, look: UnitLook) -> (u32, f32, f32) {
    if look.civilian {
        // A pointy-top hexagon, unlike the flat-top map hexes.
        return (6, FRAC_PI_2, 0.95);
    }
    match unit.unit_type {
        // A quarter turn so the triangle points up.
        UnitType::Melee => (3, FRAC_PI_2, 1.0),
        UnitType::Ranged => (4, FRAC_PI_2, 1.0),
        UnitType::Cavalry => (5, FRAC_PI_2, 1.0),
        // An upright square: an eighth of a turn from the ranged diamond.
        UnitType::Siege => (4, FRAC_PI_4, 0.92),
        // Small and round: light and quick.
        UnitType::Scout => (24, 0.0, 0.8),
        // Heavy horse points down, unlike the cavalry pentagon.
        UnitType::Horse => (5, -FRAC_PI_2, 1.0),
        // A broad octagon: the toughest unit.
        UnitType::Armored => (8, FRAC_PI_8, 1.05),
    }
}

/// The unit's silhouette in its team color with a dark outline, so it reads
/// on any terrain, and its letter on top. Civilians are hollow: a pale
/// center inside a team-colored rim.
fn push_unit_icon(
    center: Vec2,
    unit: &Unit,
    look: UnitLook,
    scale: f32,
    color: Color,
    out: &mut Vec<Vertex>,
) {
    let (sides, rotation, size) = icon_shape(unit, look);
    let radius = UNIT_ICON_RADIUS * scale * size;
    let alpha = color[3];
    mesh::regular_polygon(center, radius, sides, rotation, color, out);
    if look.civilian {
        let pale = with_alpha(mix(color, [1.0, 1.0, 1.0, 1.0], 0.7), alpha);
        mesh::regular_polygon(center, radius * 0.68, sides, rotation, pale, out);
    }
    let outline = with_alpha(ICON_OUTLINE_COLOR, alpha);
    mesh::polygon_outline(center, radius, ICON_OUTLINE_WIDTH * scale, sides, rotation, outline, out);
    let label_color = with_alpha(LABEL_COLOR, alpha);
    font::push_glyph(center, LABEL_HEIGHT * scale * size, look.letter, label_color, out);
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
    let rect = |x0: f32, y0: f32, x1: f32, y1: f32| (pos + Vec2::new(x0, y0), pos + Vec2::new(x1, y1));
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
    let digits = city.population.to_string();
    font::push_text(pos + Vec2::new(-0.11 * digits.len() as f32, -0.08), 0.3, &digits, LABEL_COLOR, out);
    if city.granary {
        let at = pos + Vec2::new(0.46, -0.42);
        mesh::regular_polygon(at, 0.15 + ICON_OUTLINE_WIDTH, 16, 0.0, ICON_OUTLINE_COLOR, out);
        mesh::regular_polygon(at, 0.15, 16, 0.0, GRANARY_COLOR, out);
        font::push_glyph(at, 0.16, 'G', LABEL_COLOR, out);
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
    font::push_glyph(pos + Vec2::new(0.0, -0.09), 0.26, 'B', with_alpha(LABEL_COLOR, alpha), out);
}

/// An improvement's badge in the hex's top-left corner: a dark square
/// edged in its owner's color, with a symbol for what it is.
fn push_site_badge(center: Vec2, label: &str, team: Team, out: &mut Vec<Vertex>) {
    let half = Vec2::splat(SITE_BADGE_HALF);
    let edge = Vec2::splat(ICON_OUTLINE_WIDTH);
    mesh::quad(center - half - edge, center + half + edge, team.color(), out);
    mesh::quad(center - half, center + half, SITE_BADGE_COLOR, out);
    let at = |x: f32, y: f32| center + Vec2::new(x, y);
    match label {
        // Rows of crops.
        "FARM" => {
            for y in [-0.08, 0.0, 0.08] {
                mesh::segment(at(-0.11, y), at(0.11, y), 0.035, FARM_COLOR, out);
            }
        }
        // A heap of ore.
        "MINE" => mesh::triangle(at(-0.12, -0.09), at(0.12, -0.09), at(0.0, 0.1), MINE_COLOR, out),
        // A fence: two posts and two rails.
        "PASTURE" => {
            for x in [-0.08, 0.08] {
                mesh::segment(at(x, -0.1), at(x, 0.1), 0.035, WOOD_COLOR, out);
            }
            for y in [-0.03, 0.05] {
                mesh::segment(at(-0.12, y), at(0.12, y), 0.03, WOOD_COLOR, out);
            }
        }
        // Stacked log ends.
        "LUMBER MILL" => {
            for (x, y) in [(-0.06, -0.05), (0.06, -0.05), (0.0, 0.06)] {
                mesh::regular_polygon(at(x, y), 0.055, 12, 0.0, WOOD_COLOR, out);
            }
        }
        other => font::push_glyph(center, 0.2, other.chars().next().unwrap_or('?'), team.color(), out),
    }
}

/// A strategic resource: a dark disc edged in gold with the resource's letter.
fn push_resource_badge(center: Vec2, glyph: char, out: &mut Vec<Vertex>) {
    mesh::regular_polygon(center, SITE_BADGE_HALF + ICON_OUTLINE_WIDTH, 16, 0.0, RESOURCE_COLOR, out);
    mesh::regular_polygon(center, SITE_BADGE_HALF, 16, 0.0, SITE_BADGE_COLOR, out);
    font::push_glyph(center, 0.2, glyph, RESOURCE_COLOR, out);
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
