//! Map icons: small pictures of strategic resources and tile improvements,
//! drawn straight on the tile in the hex's top corners, of special tiles and
//! ruins in its bottom-left corner, and of food and production in the yield
//! rows. Each shape is edged in dark so it reads on
//! any terrain. Shapes are laid out in the coordinates of
//! the mockups they were designed in: a hex of radius 100 with Y pointing
//! down, centered on the icon's spot.

use glam::Vec2;

use super::fast_hash::HashMap;
use super::hex::HEX_SIZE;
use super::mesh;
use super::terrain::{Resource, Special};
use crate::renderer::Vertex;

type Color = [f32; 4];

/// Hex radius the shapes are laid out on.
const DESIGN_HEX_RADIUS: f32 = 100.0;
/// Sides of the polygons that stand in for circles and ellipses.
const ROUND_SIDES: usize = 16;

/// Where icons sit, from the hex's center: a resource in the top-right
/// corner, an improvement in the top-left.
pub(super) const RESOURCE_SPOT: Vec2 = Vec2::new(0.5 * HEX_SIZE, 0.40 * HEX_SIZE);
pub(super) const IMPROVEMENT_SPOT: Vec2 = Vec2::new(-0.5 * HEX_SIZE, 0.42 * HEX_SIZE);
/// Where a special tile's or ruins' icon sits: the bottom-left corner, drawn
/// `LANDMARK_SCALE` times the usual size so it can be spotted from afar.
pub(super) const LANDMARK_SPOT: Vec2 = Vec2::new(-0.36 * HEX_SIZE, -0.5 * HEX_SIZE);
pub(super) const LANDMARK_SCALE: f32 = 1.3;

/// The edge around every shape, the same near-black as unit outlines.
const OUTLINE: Color = [0.03, 0.03, 0.04, 1.0];
// The mockups' colors, converted from sRGB to linear.
const HORSE: Color = [0.32, 0.12, 0.032, 1.0];
const MANE: Color = [0.042, 0.018, 0.006, 1.0];
const STEEL: Color = [0.26, 0.28, 0.31, 1.0];
const STEEL_LIT: Color = [0.55, 0.58, 0.63, 1.0];
const STEEL_SHADE: Color = [0.14, 0.155, 0.18, 1.0];
const WHEAT: Color = [0.87, 0.58, 0.08, 1.0];
const ORE: Color = [0.10, 0.084, 0.08, 1.0];
const CART: Color = [0.155, 0.068, 0.018, 1.0];
const WHEEL: Color = [0.023, 0.023, 0.027, 1.0];
const FRESH_WOOD: Color = [0.58, 0.28, 0.098, 1.0];
const WEATHERED_WOOD: Color = [0.43, 0.195, 0.055, 1.0];
const BARK: Color = [0.254, 0.102, 0.032, 1.0];
const GROWTH_RING: Color = [0.195, 0.08, 0.021, 1.0];
const STONE: Color = [0.42, 0.40, 0.36, 1.0];
const STONE_LIT: Color = [0.68, 0.65, 0.58, 1.0];
const STONE_SHADE: Color = [0.20, 0.19, 0.17, 1.0];
const LEAVES: Color = [0.06, 0.30, 0.05, 1.0];
/// Metal on yield chips and in text: brighter than the Iron deposit's
/// ingot so it reads small.
const METAL: Color = [0.42, 0.46, 0.52, 1.0];
const CLOCK_FACE: Color = [0.80, 0.80, 0.74, 1.0];
const FRUIT: Color = [0.80, 0.08, 0.04, 1.0];

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(super) enum MapIcon {
    /// Horses: a horse's head, facing right.
    HorseHead,
    /// Iron: an ingot, seen from above and to the left.
    Ingot,
    /// Farm: a single stalk of wheat.
    Wheat,
    /// Mine: a cart heaped with ore.
    OreCart,
    /// Pasture: a fence, two posts and two rails.
    Fence,
    /// Lumber mill: three stacked logs, cut ends toward the viewer.
    Logs,
    /// Food, on a yield chip and in text: a small wheat stalk.
    Food,
    /// Wood, on a yield chip and in text: a leaning log.
    Wood,
    /// Metal, on a yield chip and in text: a small ingot.
    Metal,
    /// Turns, in text: a clock face.
    Clock,
    /// Ruins: two broken columns on a slab.
    Ruins,
    /// Orchard: a fruit tree.
    FruitTree,
    /// Quarry: three cut stone blocks.
    StoneBlocks,
}

impl MapIcon {
    pub(super) fn resource(resource: Resource) -> Self {
        match resource {
            Resource::Horses => Self::HorseHead,
            Resource::Iron => Self::Ingot,
        }
    }

    pub(super) fn special(special: Special) -> Self {
        match special {
            Special::Orchard => Self::FruitTree,
            Special::Quarry => Self::StoneBlocks,
        }
    }

    /// An improvement's icon, by the label it's built with.
    pub(super) fn improvement(label: &str) -> Option<Self> {
        match label {
            "FARM" => Some(Self::Wheat),
            "MINE" => Some(Self::OreCart),
            "PASTURE" => Some(Self::Fence),
            "LUMBER MILL" => Some(Self::Logs),
            _ => None,
        }
    }
}

/// Characters that stand for an icon in UI text: every text renderer (the
/// classic font, `font::Face`, and ImGui's `rich_*` helpers) draws the
/// icon in their place, the same pictures as the map's yield chips.
pub(in crate::game) const FOOD_ICON: char = '\u{E000}';
pub(in crate::game) const WOOD_ICON: char = '\u{E001}';
pub(in crate::game) const METAL_ICON: char = '\u{E002}';
pub(in crate::game) const TIME_ICON: char = '\u{E003}';

/// The icon `ch` stands for in UI text, if it's one of the icon characters.
pub(in crate::game) fn inline_icon(ch: char) -> Option<MapIcon> {
    match ch {
        FOOD_ICON => Some(MapIcon::Food),
        WOOD_ICON => Some(MapIcon::Wood),
        METAL_ICON => Some(MapIcon::Metal),
        TIME_ICON => Some(MapIcon::Clock),
        _ => None,
    }
}

/// Draws the icon `ch` stands for in UI space (pixels, Y up), `height`
/// pixels tall, centered on `center`. `dim` darkens it to sit in disabled
/// text.
pub(in crate::game) fn push_inline_icon(
    center: Vec2,
    height: f32,
    ch: char,
    dim: bool,
    out: &mut Vec<Vertex>,
) {
    let Some(icon) = inline_icon(ch) else { return };
    let start = out.len();
    // The chip icons are laid out about 20 design units tall.
    push_map_icon_scaled(center, icon, height * 5.0 / HEX_SIZE, out);
    if dim {
        for vertex in &mut out[start..] {
            for channel in &mut vertex.color[..3] {
                *channel *= 0.3;
            }
        }
    }
}

/// Draws `icon` centered on `center`.
pub(super) fn push_map_icon(center: Vec2, icon: MapIcon, out: &mut Vec<Vertex>) {
    push_map_icon_scaled(center, icon, 1.0, out);
}

/// Draws `icon` centered on `center`, `scale` times its usual size. Each
/// icon's triangles are worked out once (`build_map_icon`), then placed.
pub(super) fn push_map_icon_scaled(center: Vec2, icon: MapIcon, scale: f32, out: &mut Vec<Vertex>) {
    thread_local! {
        static MESHES: std::cell::RefCell<HashMap<MapIcon, Vec<Vertex>>> = Default::default();
    }
    MESHES.with_borrow_mut(|meshes| {
        let shape = meshes.entry(icon).or_insert_with(|| {
            let mut shape = Vec::new();
            build_map_icon(icon, &mut shape);
            shape
        });
        mesh::place(shape, center, scale, None, out);
    });
}

/// `icon`'s triangles, centered on the origin at its usual size.
fn build_map_icon(icon: MapIcon, out: &mut Vec<Vertex>) {
    let mut pen = Pen {
        center: Vec2::ZERO,
        scale: HEX_SIZE / DESIGN_HEX_RADIUS,
        out,
    };
    match icon {
        MapIcon::HorseHead => {
            pen.shape(
                &[
                    (-9.0, 15.0),
                    (-7.0, 1.0),
                    (-3.0, -9.0),
                    (0.0, -16.0),
                    (3.0, -10.0),
                    (13.0, -3.0),
                    (15.0, 3.0),
                    (10.0, 6.0),
                    (2.0, 2.0),
                    (4.0, 15.0),
                ],
                HORSE,
                3.0,
            );
            pen.shape(&ellipse((4.0, -5.0), 1.5, 1.5, 0.0), OUTLINE, 0.0);
            pen.line(&[(-3.0, -9.0), (-9.0, -1.0), (-10.0, 8.0)], 3.0, MANE);
        }
        MapIcon::Ingot => {
            let front = [(-15.0, 9.0), (-10.0, -3.0), (10.0, -3.0), (15.0, 9.0)];
            pen.shape(&front, STEEL, 3.0);
            let top = [(-10.0, -3.0), (-6.0, -10.0), (14.0, -10.0), (10.0, -3.0)];
            pen.shape(&top, STEEL_LIT, 3.0);
            let end = [(10.0, -3.0), (14.0, -10.0), (18.0, 2.0), (15.0, 9.0)];
            pen.shape(&end, STEEL_SHADE, 3.0);
        }
        MapIcon::Wheat => {
            let stem = [(0.0, 14.0), (0.0, -8.0)];
            pen.line(&stem, 5.5, OUTLINE);
            pen.line(&stem, 2.5, WHEAT);
            // Pairs of kernels up the head, leaning outward, and one on top.
            for y in [1.0, -6.0, -13.0] {
                for side in [-1.0, 1.0] {
                    let kernel = ellipse((3.5 * side, y), 3.2, 5.0, 30.0 * side);
                    pen.shape(&kernel, WHEAT, 2.5);
                }
            }
            pen.shape(&ellipse((0.0, -18.0), 3.2, 5.0, 0.0), WHEAT, 2.5);
        }
        MapIcon::OreCart => {
            let ore = [
                (-8.0, -4.0),
                (-3.0, -12.0),
                (3.0, -10.0),
                (7.0, -14.0),
                (11.0, -5.0),
            ];
            pen.shape(&ore, ORE, 3.0);
            pen.shape(
                &[(-14.0, -5.0), (14.0, -5.0), (10.0, 8.0), (-10.0, 8.0)],
                CART,
                3.0,
            );
            for x in [-6.0, 6.0] {
                pen.shape(&ellipse((x, 11.0), 3.5, 3.5, 0.0), WHEEL, 3.0);
            }
        }
        MapIcon::Fence => {
            let posts = [
                [(-10.0, 13.0), (-10.0, -12.0)],
                [(10.0, 13.0), (10.0, -12.0)],
            ];
            let rails = [[(-15.0, -6.0), (15.0, -6.0)], [(-15.0, 4.0), (15.0, 4.0)]];
            for line in posts.iter().chain(&rails) {
                pen.line(line, 7.0, OUTLINE);
            }
            for rail in &rails {
                pen.line(rail, 3.5, WEATHERED_WOOD);
            }
            for post in &posts {
                pen.line(post, 4.0, FRESH_WOOD);
            }
        }
        MapIcon::Logs => {
            // Back to front: the top log, then the two it rests on. Each runs
            // off to the right from its cut end.
            for (x, y) in [(-8.0, -5.0), (-12.0, 6.0), (-4.0, 6.0)] {
                let mut side = vec![(x, y - 5.5)];
                side.extend((0..=8).map(|i| {
                    let angle = (-90.0 + 180.0 * i as f32 / 8.0f32).to_radians();
                    (x + 12.0 + 2.5 * angle.cos(), y + 5.5 * angle.sin())
                }));
                side.push((x, y + 5.5));
                pen.shape(&side, BARK, 3.0);
                pen.shape(&ellipse((x, y), 3.0, 5.5, 0.0), FRESH_WOOD, 2.5);
                pen.ring(&ellipse((x, y), 1.2, 2.2, 0.0), 1.2, GROWTH_RING);
            }
        }
        // The small resource icons are flat, one color each, thinly edged,
        // so they read at text size.
        MapIcon::Food => {
            // A wheat ear with no edge: a thin stem and well-spaced kernels,
            // so the gaps between them survive at text size.
            pen.line(&[(0.0, 9.5), (0.0, -1.0)], 1.0, WHEAT);
            for y in [-0.5, -6.0] {
                for side in [-1.0, 1.0] {
                    let kernel = ellipse((3.3 * side, y), 1.6, 2.4, 25.0 * side);
                    pen.shape(&kernel, WHEAT, 0.0);
                }
            }
            pen.shape(&ellipse((0.0, -9.0), 1.6, 2.4, 0.0), WHEAT, 0.0);
        }
        MapIcon::Wood => {
            // A plain log, leaning right.
            let log = [(-3.5, 6.0), (3.5, -6.0)];
            pen.line(&log, 6.2, OUTLINE);
            pen.line(&log, 4.0, FRESH_WOOD);
        }
        MapIcon::Metal => {
            let bar = [(-8.5, 4.5), (-6.0, -3.5), (6.0, -3.5), (8.5, 4.5)];
            pen.shape(&bar, METAL, 1.2);
        }
        MapIcon::Clock => {
            // An open ring with two hands, all in one color.
            pen.ring(&ellipse((0.0, 0.0), 7.5, 7.5, 0.0), 1.0, CLOCK_FACE);
            pen.line(&[(0.0, 0.0), (0.0, -4.5)], 1.0, CLOCK_FACE);
            pen.line(&[(0.0, 0.0), (3.5, 0.0)], 1.0, CLOCK_FACE);
        }
        MapIcon::Ruins => {
            pen.shape(
                &[(-16.0, 8.0), (16.0, 8.0), (16.0, 13.0), (-16.0, 13.0)],
                STONE_SHADE,
                2.5,
            );
            // A whole column with its capital, and a broken one.
            pen.shape(
                &[(-12.0, -9.0), (-5.0, -9.0), (-5.0, 8.0), (-12.0, 8.0)],
                STONE,
                2.5,
            );
            pen.shape(
                &[(-14.0, -13.0), (-3.0, -13.0), (-3.0, -9.0), (-14.0, -9.0)],
                STONE_LIT,
                2.5,
            );
            pen.shape(
                &[
                    (4.0, -2.0),
                    (7.0, 1.0),
                    (9.0, -4.0),
                    (11.0, -1.0),
                    (11.0, 8.0),
                    (4.0, 8.0),
                ],
                STONE,
                2.5,
            );
        }
        MapIcon::FruitTree => {
            pen.shape(
                &[(-2.0, 2.0), (2.0, 2.0), (3.0, 13.0), (-3.0, 13.0)],
                BARK,
                2.5,
            );
            pen.shape(&ellipse((0.0, -5.0), 13.0, 10.0, 0.0), LEAVES, 2.5);
            for (x, y) in [(-6.0, -6.0), (5.0, -8.0), (1.0, -1.0), (7.0, -1.0)] {
                pen.shape(&ellipse((x, y), 2.4, 2.4, 0.0), FRUIT, 1.2);
            }
        }
        MapIcon::StoneBlocks => {
            for (x, y) in [(-14.0, 2.0), (1.0, 2.0), (-6.5, -9.0)] {
                pen.shape(
                    &[(x, y), (x + 13.0, y), (x + 13.0, y + 10.0), (x, y + 10.0)],
                    STONE,
                    2.5,
                );
                pen.shape(
                    &[(x, y), (x + 13.0, y), (x + 13.0, y + 3.0), (x, y + 3.0)],
                    STONE_LIT,
                    0.0,
                );
            }
        }
    }
}

/// Points around an ellipse centered on `center`, turned by `degrees`
/// (clockwise on screen, as SVG's `rotate` with Y down).
fn ellipse(center: (f32, f32), rx: f32, ry: f32, degrees: f32) -> Vec<(f32, f32)> {
    let turn = Vec2::from_angle(degrees.to_radians());
    (0..ROUND_SIDES)
        .map(|i| {
            let angle = std::f32::consts::TAU * i as f32 / ROUND_SIDES as f32;
            let p = turn.rotate(Vec2::new(rx * angle.cos(), ry * angle.sin()));
            (center.0 + p.x, center.1 + p.y)
        })
        .collect()
}

/// Draws shapes given in design coordinates, scaled onto the map.
struct Pen<'a> {
    center: Vec2,
    /// World units per design unit.
    scale: f32,
    out: &'a mut Vec<Vertex>,
}

impl Pen<'_> {
    fn at(&self, (x, y): (f32, f32)) -> Vec2 {
        self.center + Vec2::new(x, -y) * self.scale
    }

    /// A filled outline, edged in dark `edge` wide (half of it outside).
    fn shape(&mut self, points: &[(f32, f32)], fill: Color, edge: f32) {
        let points: Vec<Vec2> = points.iter().map(|&p| self.at(p)).collect();
        if edge > 0.0 {
            mesh::outline(&points, edge * self.scale, OUTLINE, self.out);
        }
        mesh::polygon(&points, fill, self.out);
    }

    /// Just the outline through `points`, `width` wide.
    fn ring(&mut self, points: &[(f32, f32)], width: f32, color: Color) {
        let points: Vec<Vec2> = points.iter().map(|&p| self.at(p)).collect();
        mesh::outline(&points, width * self.scale, color, self.out);
    }

    /// A line through `points` with rounded ends.
    fn line(&mut self, points: &[(f32, f32)], width: f32, color: Color) {
        let world: Vec<Vec2> = points.iter().map(|&p| self.at(p)).collect();
        mesh::polyline(&world, width * self.scale, color, self.out);
        for &end in [world.first(), world.last()].into_iter().flatten() {
            let radius = width / 2.0 * self.scale;
            mesh::regular_polygon(end, radius, ROUND_SIDES as u32, 0.0, color, self.out);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ICONS: [MapIcon; 9] = [
        MapIcon::HorseHead,
        MapIcon::Ingot,
        MapIcon::Wheat,
        MapIcon::OreCart,
        MapIcon::Fence,
        MapIcon::Logs,
        MapIcon::Ruins,
        MapIcon::FruitTree,
        MapIcon::StoneBlocks,
    ];

    /// Whether `p` is inside the flat-top hexagon of `radius` around the origin.
    fn inside_hex(p: Vec2, radius: f32) -> bool {
        let half_height = radius * 3f32.sqrt() / 2.0;
        p.y.abs() <= half_height && 3f32.sqrt() * p.x.abs() + p.y.abs() <= 2.0 * half_height
    }

    #[test]
    fn every_map_icon_stays_in_its_corner() {
        // The unit token's reach: radius 0.36 plus half its outline.
        let token = 0.39 * HEX_SIZE;
        for icon in ICONS {
            let (spot, scale) = match icon {
                MapIcon::HorseHead | MapIcon::Ingot => (RESOURCE_SPOT, 1.0),
                MapIcon::Ruins | MapIcon::FruitTree | MapIcon::StoneBlocks => {
                    (LANDMARK_SPOT, LANDMARK_SCALE)
                }
                _ => (IMPROVEMENT_SPOT, 1.0),
            };
            let mut out = Vec::new();
            push_map_icon_scaled(spot, icon, scale, &mut out);
            assert!(!out.is_empty(), "{icon:?} drew nothing");
            for vertex in out {
                let pos = Vec2::new(vertex.pos[0], vertex.pos[1]);
                assert!(
                    inside_hex(pos, HEX_SIZE),
                    "{icon:?} leaves its hex at {pos}"
                );
                assert!(pos.length() > token, "{icon:?} reaches the unit at {pos}");
                assert!(
                    pos.distance(spot) < 0.25 * scale,
                    "{icon:?} sprawls to {pos}"
                );
            }
        }
    }

    #[test]
    fn every_improvement_has_an_icon() {
        for label in ["FARM", "MINE", "PASTURE", "LUMBER MILL"] {
            assert!(MapIcon::improvement(label).is_some(), "{label}");
        }
    }
}
