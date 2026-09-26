use std::collections::{HashMap, HashSet};

use glam::Vec2;

use super::terrain::{Resource, Terrain, Tile};

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

    /// The hexes strictly between `self` and `other` along the straight line
    /// joining their centers. Where the line runs exactly along an edge it
    /// could pass either side; `nudge` (a small offset, positive or negative)
    /// picks one.
    pub fn line_between(self, other: Hex, nudge: f32) -> impl Iterator<Item = Hex> {
        let steps = self.distance(other);
        let (q0, r0) = (self.q as f32 + nudge, self.r as f32 + nudge);
        let (dq, dr) = ((other.q - self.q) as f32, (other.r - self.r) as f32);
        (1..steps).map(move |i| {
            let t = i as f32 / steps as f32;
            Hex::round(q0 + dq * t, r0 + dr * t - 2.0 * nudge)
        })
    }
}

/// The outline of a grid, centered on the origin.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Shape {
    /// Every hex within `radius` of the origin.
    Hexagon { radius: i32 },
    /// Columns `-cols..=cols`, and rows reaching `rows` hexes above and below
    /// the middle, so the map is a rectangle wider than tall with a slightly
    /// ragged top and bottom edge.
    Rectangle { cols: i32, rows: i32 },
}

/// A grid of hexes in some `Shape`. Hexes not listed in `tiles` are plains.
/// Rivers run along the edges between hexes.
#[derive(Clone)]
pub struct HexGrid {
    shape: Shape,
    tiles: HashMap<Hex, Tile>,
    /// Hex edges with a river, each as the pair of hexes it separates
    /// (see `edge`).
    rivers: HashSet<(Hex, Hex)>,
    /// Strategic resources on hexes.
    resources: HashMap<Hex, Resource>,
}

/// The edge between two adjacent hexes, the same whichever way round they're
/// given.
pub fn edge(a: Hex, b: Hex) -> (Hex, Hex) {
    if (a.q, a.r) <= (b.q, b.r) {
        (a, b)
    } else {
        (b, a)
    }
}

/// The two ends of the edge between adjacent hexes `a` and `b`. Each end is
/// a corner shared with one of the two hexes next to both, which sits at the
/// middle of the three centers.
pub fn edge_corners(a: Hex, b: Hex) -> (Vec2, Vec2) {
    let mut shared = a.neighbors().into_iter().filter(|n| n.distance(b) == 1);
    let mut corner = || {
        let c = shared.next().expect("adjacent hexes share two neighbors");
        (a.to_world() + b.to_world() + c.to_world()) / 3.0
    };
    (corner(), corner())
}

impl HexGrid {
    /// A hexagon of `radius`.
    pub fn new<T: Into<Tile>>(radius: i32, tiles: impl IntoIterator<Item = (Hex, T)>) -> Self {
        Self::shaped(Shape::Hexagon { radius }, tiles)
    }

    pub fn shaped<T: Into<Tile>>(shape: Shape, tiles: impl IntoIterator<Item = (Hex, T)>) -> Self {
        Self {
            shape,
            tiles: tiles
                .into_iter()
                .map(|(hex, tile)| (hex, tile.into()))
                .collect(),
            rivers: HashSet::new(),
            resources: HashMap::new(),
        }
    }

    /// A hexagon of `radius` with strategic resources placed on it.
    pub fn with_resources<T: Into<Tile>>(
        radius: i32,
        tiles: impl IntoIterator<Item = (Hex, T)>,
        resources: impl IntoIterator<Item = (Hex, Resource)>,
    ) -> Self {
        let mut grid = Self::new(radius, tiles);
        grid.resources = resources.into_iter().collect();
        grid
    }

    pub fn with_rivers(mut self, rivers: HashSet<(Hex, Hex)>) -> Self {
        self.rivers = rivers;
        self
    }

    pub fn shape(&self) -> Shape {
        self.shape
    }

    pub fn contains(&self, hex: Hex) -> bool {
        self.edge_distance(hex) >= 0
    }

    /// How many hexes in from the edge `hex` is: 0 on the outermost ring,
    /// negative off the grid.
    pub fn edge_distance(&self, hex: Hex) -> i32 {
        match self.shape {
            Shape::Hexagon { radius } => radius - hex.distance(Hex::new(0, 0)),
            Shape::Rectangle { cols, rows } => {
                // Doubled rows: 2r + q is even in even columns, odd in odd ones.
                let row2 = 2 * hex.r + hex.q;
                (cols - hex.q.abs()).min((2 * rows - row2.abs()).div_euclid(2))
            }
        }
    }

    /// The whole tile: ground, hills and feature.
    pub fn tile(&self, hex: Hex) -> Tile {
        self.tiles.get(&hex).copied().unwrap_or_default()
    }

    /// Just the ground, without hills or feature.
    pub fn terrain(&self, hex: Hex) -> Terrain {
        self.tile(hex).terrain
    }

    /// Every river edge, as pairs of hexes.
    pub fn rivers(&self) -> impl Iterator<Item = (Hex, Hex)> + '_ {
        self.rivers.iter().copied()
    }

    pub fn has_river(&self, a: Hex, b: Hex) -> bool {
        self.rivers.contains(&edge(a, b))
    }

    /// Land beside a river or a lake. Fresh water adds food.
    pub fn has_fresh_water(&self, hex: Hex) -> bool {
        !self.terrain(hex).is_water()
            && hex.neighbors().into_iter().any(|n| {
                self.has_river(hex, n) || (self.contains(n) && self.terrain(n) == Terrain::Lake)
            })
    }

    pub fn resource(&self, hex: Hex) -> Option<Resource> {
        self.resources.get(&hex).copied()
    }

    /// On the grid and not blocked by terrain (units aside).
    pub fn is_passable(&self, hex: Hex) -> bool {
        self.contains(hex) && self.terrain(hex).is_passable()
    }

    pub fn all_hexes(&self) -> impl Iterator<Item = Hex> + '_ {
        // The bounding box of either shape, filtered down to the shape.
        let (q_max, r_max) = match self.shape {
            Shape::Hexagon { radius } => (radius, radius),
            Shape::Rectangle { cols, rows } => (cols, rows + cols / 2 + 1),
        };
        (-q_max..=q_max).flat_map(move |q| {
            (-r_max..=r_max)
                .map(move |r| Hex::new(q, r))
                .filter(|h| self.contains(*h))
        })
    }
}
