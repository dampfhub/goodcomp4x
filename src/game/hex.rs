use std::collections::HashMap;

use glam::Vec2;

use super::terrain::Terrain;

/// Center-to-corner radius of a hex, in world units.
pub const HEX_SIZE: f32 = 1.0;

/// Flat-top hex in axial coordinates. The math follows
/// <https://www.redblobgames.com/grids/hexagons/>.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub struct Hex {
    pub q: i32,
    pub r: i32,
}

impl Hex {
    const DIRECTIONS: [(i32, i32); 6] = [(1, 0), (1, -1), (0, -1), (-1, 0), (-1, 1), (0, 1)];

    pub const fn new(q: i32, r: i32) -> Self {
        Self { q, r }
    }

    pub fn to_world(self) -> Vec2 {
        let sqrt3 = 3f32.sqrt();
        let (q, r) = (self.q as f32, self.r as f32);
        Vec2::new(1.5 * q, sqrt3 * (0.5 * q + r)) * HEX_SIZE
    }

    /// The hex containing a world-space point.
    pub fn from_world(pos: Vec2) -> Self {
        let q = (2.0 / 3.0 * pos.x) / HEX_SIZE;
        let r = (-1.0 / 3.0 * pos.x + 3f32.sqrt() / 3.0 * pos.y) / HEX_SIZE;
        Self::round(q, r)
    }

    /// Rounds fractional axial coordinates via cube coordinates (q + r + s = 0):
    /// round all three, then recompute whichever drifted most from the others.
    fn round(q: f32, r: f32) -> Self {
        let s = -q - r;
        let (mut rq, mut rr, rs) = (q.round(), r.round(), s.round());
        let (dq, dr, ds) = ((rq - q).abs(), (rr - r).abs(), (rs - s).abs());

        if dq > dr && dq > ds {
            rq = -rr - rs;
        } else if dr > ds {
            rr = -rq - rs;
        }
        Hex::new(rq as i32, rr as i32)
    }

    pub fn distance(self, other: Hex) -> i32 {
        let dq = self.q - other.q;
        let dr = self.r - other.r;
        (dq.abs() + dr.abs() + (dq + dr).abs()) / 2
    }

    pub fn neighbors(self) -> [Hex; 6] {
        Self::DIRECTIONS.map(|(dq, dr)| Hex::new(self.q + dq, self.r + dr))
    }
}

/// A hexagon-shaped grid of hexes within `radius` of the origin. Hexes not
/// listed in `terrain` are plains.
#[derive(Clone)]
pub struct HexGrid {
    radius: i32,
    terrain: HashMap<Hex, Terrain>,
}

impl HexGrid {
    pub fn new(radius: i32, terrain: impl IntoIterator<Item = (Hex, Terrain)>) -> Self {
        Self {
            radius,
            terrain: terrain.into_iter().collect(),
        }
    }

    pub fn contains(&self, hex: Hex) -> bool {
        hex.distance(Hex::new(0, 0)) <= self.radius
    }

    pub fn terrain(&self, hex: Hex) -> Terrain {
        self.terrain.get(&hex).copied().unwrap_or_default()
    }

    /// On the grid and not blocked by terrain (units aside).
    pub fn is_passable(&self, hex: Hex) -> bool {
        self.contains(hex) && self.terrain(hex).is_passable()
    }

    pub fn all_hexes(&self) -> impl Iterator<Item = Hex> + '_ {
        let n = self.radius;
        (-n..=n).flat_map(move |q| {
            let r_min = (-n).max(-q - n);
            let r_max = n.min(-q + n);
            (r_min..=r_max).map(move |r| Hex::new(q, r))
        })
    }
}
