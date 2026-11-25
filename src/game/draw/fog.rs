//! The fog of war over the map: remembered hexes veiled, and ruins and dens
//! as last seen.

use super::*;

/// The grey veil over remembered hexes out of sight, and the line where they
/// meet hexes in sight: a light, cool grey like the cloud, so it can't be
/// mistaken for the dark gaps between ordinary hexes.
pub(super) const FOG_EDGE_COLOR: Color = [0.24, 0.25, 0.28, 1.0];
/// The fog edge fills the whole gap between two hexes' fills: each fill stops
/// short of its hex's edge by (1 - HEX_FILL_SCALE) of the apothem, sqrt(3) / 2.
pub(super) const FOG_EDGE_WIDTH: f32 = HEX_SIZE * (1.0 - HEX_FILL_SCALE) * 1.732_050_8;
/// Along a river the fog edge widens to cover it whole, since a river is wider
/// than the gap; otherwise a sliver of it would show on the side in sight.
pub(super) const FOG_RIVER_EDGE_WIDTH: f32 = RIVER_WIDTH + 0.02;
pub(super) const REMEMBERED_TINT: Color = [0.0, 0.0, 0.0, 0.58];
/// The veil over a remembered hex's border where it faces a never-seen hex:
/// the border's color already darkened by `REMEMBERED_TINT`, drawn opaque,
/// because the bands of two hexes meeting a never-seen one overlap at their
/// shared corner, and a translucent veil would darken twice there.
pub(super) const REMEMBERED_BORDER_COLOR: Color = [
    BORDER_COLOR[0] * (1.0 - REMEMBERED_TINT[3]),
    BORDER_COLOR[1] * (1.0 - REMEMBERED_TINT[3]),
    BORDER_COLOR[2] * (1.0 - REMEMBERED_TINT[3]),
    1.0,
];
/// The row of pips beside ruins counting the turns they've been held: where
/// the first sits from the hex's center, and the step to the next.
pub(super) const RUIN_PIPS: Vec2 = Vec2::new(-0.06, -0.7);
pub(super) const RUIN_PIP_RADIUS: f32 = 0.05;
pub(super) const RUIN_PIP_GAP: f32 = 0.13;

pub(super) const RUIN_RIM_COLOR: Color = [0.62, 0.58, 0.50, 1.0];
pub(super) const DEN_RIM_COLOR: Color = [0.36, 0.20, 0.09, 1.0];

impl GameState {
    /// The world rectangle (min, max) the camera may show in this viewport.
    pub(in crate::game) fn cloud_view(&self) -> (Vec2, Vec2) {
        self.camera.view_bounds()
    }

    /// Ruins on `hex` as the player knows them: in sight, with a pip for
    /// each turn their holder has held them, in its color; out of sight, as
    /// last seen, without.
    pub(super) fn push_known_ruin(&self, hex: Hex, fog: &Fog, out: &mut Vec<Vertex>) {
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
        crate::game::sprites::push_icon(spot, MapIcon::Ruins, LANDMARK_SCALE, out);
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
    pub(super) fn push_known_den(&self, hex: Hex, fog: &Fog, out: &mut Vec<Vertex>) {
        let known = if fog.sees(hex) {
            self.den_at(hex).is_some()
        } else {
            self.memory.get(&hex).is_some_and(|seen| seen.den.is_some())
        };
        if known {
            push_landmark_rim(hex.to_world(), DEN_RIM_COLOR, out);
            let spot = hex.to_world() + LANDMARK_SPOT;
            crate::game::sprites::push_icon(spot, MapIcon::Paw, LANDMARK_SCALE, out);
        }
    }

    /// Veils remembered hexes out of sight, edging them where they meet hexes
    /// in sight. Drawn over the map and cities but under units and orders;
    /// unexplored hexes are the fog's fill and clouds, under the map
    /// (`build_vertices`, `clouds.rs`).
    pub(super) fn push_fog(&self, fog: &Fog, out: &mut Vec<Vertex>) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::draw::tests::top_color;

    #[test]
    fn remembered_hexes_get_the_veil_and_no_clouds() {
        let mut game = GameState::world_scenario(3);
        for h in game.grid.all_hexes() {
            game.memory.insert(h, crate::game::fog::Sighting::default());
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
                .insert(hex, crate::game::fog::Sighting::default());
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
}
