//! Terrain: each hex's fill (with the selected unit's ranges and other
//! tints), tile colors and symbols, waves, rivers, landmark rims.

use super::*;

pub(super) const LANDMARK_RIM_WIDTH: f32 = 0.05;

pub(super) const PLAINS_COLOR: Color = [0.26, 0.24, 0.12, 1.0];
pub(super) const GRASSLAND_COLOR: Color = [0.12, 0.20, 0.08, 1.0];
pub(super) const DESERT_COLOR: Color = [0.45, 0.36, 0.17, 1.0];
pub(super) const DUNE_COLOR: Color = [0.30, 0.22, 0.09, 1.0];
pub(super) const TUNDRA_COLOR: Color = [0.20, 0.21, 0.19, 1.0];
pub(super) const TUFT_COLOR: Color = [0.36, 0.38, 0.33, 1.0];
pub(super) const SNOWFIELD_COLOR: Color = [0.55, 0.60, 0.66, 1.0];
pub(super) const FOREST_COLOR: Color = [0.03, 0.10, 0.03, 1.0];
pub(super) const MARSH_COLOR: Color = [0.09, 0.15, 0.11, 1.0];
pub(super) const REED_COLOR: Color = [0.22, 0.30, 0.15, 1.0];
pub(super) const JUNGLE_COLOR: Color = [0.02, 0.12, 0.05, 1.0];
pub(super) const CANOPY_COLOR: Color = [0.01, 0.06, 0.02, 1.0];
pub(super) const TREE_COLOR: Color = [0.015, 0.045, 0.015, 1.0];
pub(super) const COAST_COLOR: Color = [0.06, 0.16, 0.30, 1.0];
pub(super) const OCEAN_COLOR: Color = [0.02, 0.06, 0.16, 1.0];
pub(super) const LAKE_COLOR: Color = [0.07, 0.19, 0.30, 1.0];
pub(super) const WAVE_COLOR: Color = [0.16, 0.32, 0.52, 1.0];
pub(super) const DEEP_WAVE_COLOR: Color = [0.05, 0.13, 0.28, 1.0];
pub(super) const RIVER_COLOR: Color = [0.12, 0.34, 0.70, 1.0];

pub(super) const MOUNTAIN_COLOR: Color = [0.13, 0.12, 0.12, 1.0];
pub(super) const MOUNTAIN_PEAK_COLOR: Color = [0.44, 0.42, 0.42, 1.0];
pub(super) const SNOW_COLOR: Color = [0.90, 0.92, 0.95, 1.0];

pub(super) const CONTESTED_COLOR: Color = [0.55, 0.32, 0.10, 1.0];

pub(super) const ATTACK_RANGE_EMPTY_COLOR: Color = [0.36, 0.24, 0.20, 1.0];

/// A thin rim just inside the fill of the hex at `center`, marking a
/// landmark (a special tile or ruins).
pub(super) fn push_landmark_rim(center: Vec2, color: Color, out: &mut Vec<Vertex>) {
    let radius = HEX_SIZE * HEX_FILL_SCALE - LANDMARK_RIM_WIDTH / 2.0;
    let corners: Vec<Vec2> = (0..6)
        .map(|i| center + Vec2::from_angle(i as f32 * TAU / 6.0) * radius)
        .collect();
    mesh::outline(&corners, LANDMARK_RIM_WIDTH, color, out);
}

impl GameState {
    /// The terrain's color, unless the hex is selected, contested, or in the
    /// selected unit's move or attack range. Mountains are never highlighted
    /// since nothing can move to or stand on them.
    pub(super) fn hex_fill(&self, hex: Hex, selection: Option<&Selection>, fog: &Fog) -> Color {
        let tile = self.grid.tile(hex);
        let base = tile_color(tile);
        if !tile.terrain.is_passable() {
            return base;
        }
        let in_group = self.group.iter().any(|&i| self.units[i].pos == hex);
        if in_group || selection.is_some_and(|sel| sel.pos == hex) {
            return SELECTED_COLOR;
        }
        // Only in sight: out of it, no unit is known to be anywhere.
        if fog.sees(hex) && self.is_contested(hex) {
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

/// The ground's color, tinted green under forest or jungle.
pub(super) fn tile_color(tile: Tile) -> Color {
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

/// Symbols drawn over the hex fill, so a tile stays recognizable under
/// selection highlights. They sit clear of the middle, where a unit's icon
/// goes. Hills are two small peaks in a darker shade of their ground along
/// the bottom; forest (pines) and jungle (round canopies) go along the
/// bottom too, or along the top on hills. Bare ground gets its own marks
/// along the bottom: dunes for desert, grass tufts for tundra, reeds for
/// marsh. Mountains get one large snow-capped peak, water waves.
pub(in crate::game) fn push_tile_symbols(center: Vec2, tile: Tile, out: &mut Vec<Vertex>) {
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
pub(super) fn wave(at: Vec2, width: f32, color: Color, out: &mut Vec<Vertex>) {
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
pub(super) fn push_rivers(grid: &HexGrid, explored: impl Fn(Hex) -> bool, out: &mut Vec<Vertex>) {
    for (a, b) in grid.rivers().filter(|(a, b)| explored(*a) || explored(*b)) {
        let (start, end) = edge_corners(a, b);
        push_rounded_segment(start, end, RIVER_WIDTH, RIVER_COLOR, out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::draw::tests::{count_color, scene};
    use crate::game::fog::tests::{behind_the_mountain, glance_at};
    use crate::game::unit::{Unit, UnitType};

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
    fn a_fight_out_of_sight_leaves_its_hex_uncolored() {
        let drawn = |fight: bool| {
            let (mut game, idx, hidden) = behind_the_mountain();
            glance_at(&mut game, idx, hidden);
            if fight {
                for (id, team) in [(2, Team::Red), (3, Team::Green)] {
                    game.units
                        .push(Unit::new(id, hidden, team, UnitType::Melee));
                }
                assert!(game.is_contested(hidden));
            }
            scene(&game)
        };
        assert!(drawn(true) == drawn(false));

        // In sight, the fight shows.
        let (mut game, idx, _) = behind_the_mountain();
        let near = Hex::new(0, 1);
        game.units
            .push(Unit::new(2, near, Team::Red, UnitType::Melee));
        game.units[idx].pos = near;
        game.selected = None;
        game.explore();
        assert!(count_color(&game.build_vertices(), CONTESTED_COLOR) > 0);
    }
}
