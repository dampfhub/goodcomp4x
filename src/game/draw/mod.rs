//! Builds each frame's geometry from the game state: `build_vertices`
//! puts the layers together, back to front, and each part below draws one
//! concern. Each part takes `use super::*`, so the imports they share are
//! here, as are the shapes and sizes more than one part uses.

use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, TAU};

use glam::Vec2;

use super::city::{
    BARRACKS_MAX_HP, Build, Building, COASTAL_BATTERY_MAX_HP, Lane, SETTLER_MIN_POPULATION,
    WaitsFor, stock_icons, turns_icon,
};
use super::combat::{AttackPreview, Hurt, Loss};
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

mod city_map;
mod clouds;
mod fog;
mod interior;
mod markers;
mod orders;
mod palette;
mod terrain;
mod units;
mod workers;
mod yields;

use city_map::*;
use clouds::*;
use markers::*;
use orders::*;
use palette::*;
use terrain::*;
use units::*;
use workers::*;

pub(super) use markers::{push_barracks_house, push_city_tower};
pub(super) use orders::{attack_arc_points, push_arrow};
pub(super) use terrain::push_tile_symbols;
pub(super) use units::{UnitLook, push_unit_token};

/// Each hex's fill as a share of its size; the rest is the border between hexes.
const HEX_FILL_SCALE: f32 = 0.92;
/// How far an explored hex's border reaches: past its own edge by as much as
/// a neighbor's border reaches in, so facing a never-seen hex or the map's
/// edge it is as wide as the whole gap between two hexes.
const OUTER_BORDER_RADIUS: f32 = HEX_SIZE * (2.0 - HEX_FILL_SCALE);

/// Thin rims inside hexes worth scouting for: special tiles and ruins, and
/// animal dens.
const SPECIAL_RIM_COLOR: Color = [0.95, 0.66, 0.12, 1.0];

/// Under the clouds, filling the gaps between puffs: the shade of their
/// undersides, so the fog reads as cloud all the way through.
const CLOUD_BASE_COLOR: Color = [0.10, 0.103, 0.123, 1.0];
/// The fog without clouds (the Fog setting's SOLID GREY).
const SOLID_FOG_COLOR: Color = [0.13, 0.135, 0.155, 1.0];

/// Rivers are a little wider than the dark gap between hexes they run along.
const RIVER_WIDTH: f32 = 0.16;

/// While something is being placed for a city's workers, the tint over tiles
/// they can reach...
const WORKER_REACH_TINT: Color = [0.95, 0.78, 0.42, 0.16];
/// ...and over the explored tiles they can't.
const OUT_OF_REACH_TINT: Color = [0.0, 0.0, 0.0, 0.45];

/// A unit's disc drawn at full scale, alone in its hex, as units' sizes
/// are given. Small enough to leave the hex's top corners to resource and
/// improvement icons.
const UNIT_ICON_RADIUS: f32 = HEX_SIZE * 0.36;

const ICON_OUTLINE_WIDTH: f32 = 0.06;

/// A job on a tile is a hexagon nested inside the worked-tile ring
/// (`WORKED_OUTLINE_RADIUS`), so a worked tile with a job shows both, and
/// clear of a guard's outline and a fort's ring inside it. It is open along
/// its top side, where the job's name goes.
const JOB_RING_RADIUS: f32 = HEX_SIZE * 0.70;

const JOB_RING_WIDTH: f32 = 0.06;

/// How thick a wall or gate is along its hex edge.
const BARRIER_WIDTH: f32 = 0.16;

/// Icon growth while a unit is highlighted for having just acted.
const ACTED_SCALE: f32 = 1.35;

/// Structures' health bars, relative to a unit's.
const BARRACKS_BAR_SCALE: f32 = 0.7;
const BATTERY_BAR_SCALE: f32 = 0.62;

/// Outline around each tile the hovered or selected city works; a job on the
/// tile nests inside it (`JOB_RING_RADIUS`).
const WORKED_OUTLINE_RADIUS: f32 = HEX_SIZE * 0.84;

/// A queued move is drawn as a faded copy of the unit at its destination.
const GHOST_ALPHA: f32 = 0.4;

const QUEUE_TAG_OFFSET: Vec2 = Vec2::new(-0.44, -0.4);

const QUEUE_TAG_SCALE: f32 = 0.7;

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
    /// The whole scene as a triangle list, back to front: the fog's fill and
    /// clouds, hexes (borders, fills, symbols, rivers), cities, the veil over
    /// remembered hexes, worker reach, the turn dim, order markers, units,
    /// the attack preview, field workers, yields, then effects
    /// (`scene_stages` names them). While a city interior is open, its grid
    /// instead.
    #[cfg(test)]
    pub fn build_vertices(&self) -> Vec<Vertex> {
        self.build_scene(Vec::new())
    }

    /// `build_vertices` into `buffer`, reusing its memory: the app keeps one
    /// from frame to frame, as a scene is a few megabytes.
    pub fn build_vertices_into(&self, buffer: &mut Vec<Vertex>) {
        *buffer = self.build_scene(std::mem::take(buffer));
    }

    fn build_scene(&self, out: Vec<Vertex>) -> Vec<Vertex> {
        self.build_scene_staged(out, |_, _| {})
    }

    /// Each layer's vertex count, in drawing order: what a frame spends
    /// where (`perf.rs` checks a budget with it).
    #[cfg(test)]
    pub(super) fn scene_stages(&self) -> Vec<(&'static str, usize)> {
        let mut stages = Vec::new();
        let mut last = 0;
        self.build_scene_staged(Vec::new(), |name, len| {
            stages.push((name, len - last));
            last = len;
        });
        stages
    }

    /// `build_scene`, calling `stage` with each layer's name and the total
    /// vertex count once that layer is drawn.
    fn build_scene_staged(
        &self,
        mut out: Vec<Vertex>,
        mut stage: impl FnMut(&'static str, usize),
    ) -> Vec<Vertex> {
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
        stage("fog fill and clouds", out.len());

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
        stage("hex borders", out.len());
        for &hex in &explored {
            let center = hex.to_world();
            let fill = self.hex_fill(hex, selection.as_ref(), &fog);
            mesh::regular_polygon(center, HEX_SIZE * HEX_FILL_SCALE, 6, 0.0, fill, &mut out);
            super::sprites::push_tile(center, self.grid.tile(hex), &mut out);
            if let Some(resource) = self.grid.resource(hex) {
                let icon = MapIcon::resource(resource);
                super::sprites::push_icon(center + RESOURCE_SPOT, icon, 1.0, &mut out);
            }
            if let Some(special) = self.grid.special(hex) {
                push_landmark_rim(center, SPECIAL_RIM_COLOR, &mut out);
                let icon = MapIcon::special(special);
                let spot = center + LANDMARK_SPOT;
                super::sprites::push_icon(spot, icon, LANDMARK_SCALE, &mut out);
            }
            self.push_known_ruin(hex, &fog, &mut out);
            self.push_known_den(hex, &fog, &mut out);
        }
        stage("hex fills and symbols", out.len());
        push_rivers(
            &self.grid,
            |h| self.may_show(h) && self.is_explored(h),
            &mut out,
        );
        stage("rivers", out.len());

        self.push_city_map(&fog, &mut out);
        stage("city map", out.len());
        self.push_fog(&fog, &mut out);
        stage("remembered veil", out.len());
        // Placing something for a city's workers: the tiles they can reach
        // (as the player knows the board) are lit, and the rest dimmed, so
        // the reach stands out. Only what the camera may show.
        if self.placing_job.is_some() {
            let shown = |h: Hex| self.may_show(h) && self.is_explored(h);
            for hex in self.grid.all_hexes().filter(|&h| shown(h)) {
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
        stage("worker reach", out.len());
        self.push_turn_dim(&mut out);
        stage("turn dim", out.len());
        self.push_order_markers(&fog, &mut out);
        stage("order markers", out.len());

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
        stage("units", out.len());
        if let Some(preview) = self.attack_preview() {
            self.push_attack_preview(&preview, &mut out);
        }
        stage("attack preview", out.len());
        self.push_field_workers(&fog, &mut out);
        stage("field workers", out.len());
        self.push_tile_yields(&fog, &mut out);
        stage("tile yields", out.len());
        self.push_effects(&mut out);
        stage("effects", out.len());
        out
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
}

/// A line from `a` to `b` with round ends, so lines meeting at a point join
/// smoothly.
fn push_rounded_segment(a: Vec2, b: Vec2, width: f32, color: Color, out: &mut Vec<Vertex>) {
    mesh::segment(a, b, width, color, out);
    for end in [a, b] {
        mesh::regular_polygon(end, width / 2.0, 12, 0.0, color, out);
    }
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

#[cfg(test)]
mod tests {
    use super::fog::FOG_EDGE_WIDTH;
    use super::*;
    use crate::game::PLAYER_TEAM;
    use crate::game::fog::tests::{behind_the_mountain, glance_at};
    use crate::game::unit::{Unit, UnitType};

    /// The color of the last opaque triangle drawn over `point`.
    pub(super) fn top_color(vertices: &[Vertex], point: Vec2) -> Option<Color> {
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
    pub(super) fn in_gap(from: Hex, to: Hex) -> Vec2 {
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

    /// Every vertex as plain data, sorted: labels over a route map come out
    /// in hash order, so two builds of one scene can differ only in order.
    pub(super) fn scene(game: &GameState) -> Vec<[u32; 9]> {
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

    pub(super) fn count_color(vertices: &[Vertex], color: Color) -> usize {
        vertices.iter().filter(|v| v.color == color).count()
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

    /// The hash `golden_scenes_stay_the_same` expects.
    const GOLDEN_SCENES: u64 = 17138018699754215187;

    /// One hash of the scenes of every scenario: as it starts; without fog,
    /// with Alt held and a unit selected; with a city and then its interior
    /// open; with the AI's orders planned for the player; and a few turns on.
    /// A refactor of the drawing code must leave it the same. Ignored, as any
    /// change to the drawing changes it: run it before such a refactor
    /// (updating `GOLDEN_SCENES` if the drawing has changed since) and after,
    /// with `cargo test --release golden_scenes -- --ignored`.
    #[test]
    #[ignore]
    fn golden_scenes_stay_the_same() {
        use crate::game::scenario::Scenario;
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        let mut add = |game: &GameState| {
            for v in game.build_vertices() {
                let floats = v.pos.iter().chain(&v.color).chain(&v.uv);
                for byte in floats.flat_map(|x| x.to_bits().to_le_bytes()) {
                    hash = (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3);
                }
            }
        };
        for scenario in Scenario::ALL {
            let mut game = GameState::new();
            game.settings.instant_playback = true;
            game.seed_rng(7);
            game.switch_scenario(scenario);
            add(&game);

            let mut open = game.clone();
            open.fog_of_war = false;
            open.set_details(true);
            open.selected = open.units.iter().position(|u| u.team == PLAYER_TEAM);
            add(&open);

            let mut city = game.clone();
            if let Some(i) = city.cities.iter().position(|c| c.team == PLAYER_TEAM) {
                city.open_city(i);
                add(&city);
                city.open_city_interior(i);
                add(&city);
            }

            game.plan_ai_turn(PLAYER_TEAM);
            add(&game);
            for _ in 0..6 {
                game.selected = None;
                game.group.clear();
                game.plan_ai_turn(PLAYER_TEAM);
                game.resolve_turn();
                game.update(0.0);
            }
            add(&game);
        }
        assert_eq!(
            hash, GOLDEN_SCENES,
            "the scenes changed: a refactor of the drawing must leave them the same"
        );
    }
}
