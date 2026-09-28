//! Map icons: small pictures of strategic resources and tile improvements,
//! drawn straight on the tile in the hex's top corners, of special tiles and
//! caches (the ruins of the rules) in its bottom-left corner, and of food
//! and production in the yield rows; city buildings on their sign plates;
//! and the marks of enclaves, garrisons and what workers build (watchfires,
//! bunkers, tyre walls and bus gates). They're the salvage age's: rust,
//! sheet metal, planks and hazard amber (`docs/apocalypse-theme.md`).
//!
//! Each shape is edged in dark so it reads on any terrain. Shapes are laid
//! out in the coordinates of the mockups they were designed in: a hex of
//! radius 100 with Y pointing down, centered on the icon's spot.

use glam::Vec2;

use super::city::Building;
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
/// Where a special tile's or cache's icon sits: the bottom-left corner,
/// drawn `LANDMARK_SCALE` times the usual size so it can be spotted from afar.
pub(super) const LANDMARK_SPOT: Vec2 = Vec2::new(-0.36 * HEX_SIZE, -0.5 * HEX_SIZE);
pub(super) const LANDMARK_SCALE: f32 = 1.3;

/// The edge around every shape, the same near-black as unit outlines.
const OUTLINE: Color = [0.03, 0.03, 0.04, 1.0];
/// Pictograms on building plates: a warm near-black, like stencil paint.
const INK: Color = [0.035, 0.03, 0.028, 1.0];
// The mockups' colors, converted from sRGB to linear.
const HORSE: Color = [0.34, 0.16, 0.055, 1.0];
const MANE: Color = [0.042, 0.018, 0.006, 1.0];
const MUZZLE: Color = [0.10, 0.045, 0.02, 1.0];
const RUST: Color = [0.40, 0.11, 0.028, 1.0];
const RUST_LIT: Color = [0.62, 0.22, 0.06, 1.0];
const RUST_DARK: Color = [0.15, 0.04, 0.012, 1.0];
const SHEET_LIT: Color = [0.50, 0.51, 0.48, 1.0];
const STEEL: Color = [0.34, 0.37, 0.41, 1.0];
const STEEL_LIT: Color = [0.62, 0.65, 0.69, 1.0];
const CONCRETE: Color = [0.38, 0.37, 0.34, 1.0];
const CONCRETE_LIT: Color = [0.62, 0.60, 0.55, 1.0];
const CONCRETE_SHADE: Color = [0.19, 0.185, 0.17, 1.0];
const PLANK: Color = [0.33, 0.20, 0.09, 1.0];
const PLANK_LIT: Color = [0.62, 0.45, 0.24, 1.0];
const BARK: Color = [0.20, 0.09, 0.03, 1.0];
const CUT_WOOD: Color = [0.66, 0.40, 0.16, 1.0];
const GROWTH_RING: Color = [0.25, 0.11, 0.03, 1.0];
const SOIL: Color = [0.12, 0.06, 0.025, 1.0];
const CROP: Color = [0.22, 0.42, 0.05, 1.0];
const LEAVES: Color = [0.07, 0.17, 0.03, 1.0];
const LEAVES_LIT: Color = [0.15, 0.29, 0.05, 1.0];
const FRUIT: Color = [0.88, 0.24, 0.03, 1.0];
const OLIVE_DRAB: Color = [0.16, 0.17, 0.06, 1.0];
const OLIVE_LIT: Color = [0.30, 0.31, 0.12, 1.0];
const OLIVE_SHADE: Color = [0.07, 0.075, 0.03, 1.0];
/// Hazard amber, the salvage age's accent (and a school bus's paint).
const AMBER: Color = [0.95, 0.48, 0.02, 1.0];
const FLAME: Color = [1.0, 0.25, 0.02, 1.0];
const FLAME_CORE: Color = [1.0, 0.78, 0.18, 1.0];
const BONE: Color = [0.80, 0.74, 0.60, 1.0];
const TARP: Color = [0.07, 0.18, 0.42, 1.0];
const SANDBAG: Color = [0.42, 0.34, 0.17, 1.0];
const TYRE: Color = [0.025, 0.025, 0.028, 1.0];
const TREAD: Color = [0.10, 0.10, 0.105, 1.0];
/// A tin can's label, on yield chips and the Cannery's plate.
const CAN_LABEL: Color = [0.78, 0.14, 0.03, 1.0];
const TIN: Color = [0.62, 0.63, 0.62, 1.0];
/// Scrap on yield chips and in text: a bright steel gear, so it reads small.
const METAL: Color = [0.46, 0.50, 0.56, 1.0];
const CLOCK_FACE: Color = [0.80, 0.80, 0.74, 1.0];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum MapIcon {
    /// Horses: a feral horse's head, facing right.
    HorseHead,
    /// Wrecks (the Iron deposit): a rusted car body with its windows out.
    CarWreck,
    /// Homestead (a farm): furrowed rows by a sheet-roofed shack.
    Homestead,
    /// Scrap dig (a mine): a pick stuck in a heap of scrap.
    ScrapDig,
    /// Corral (a pasture): two pallets stood up as a fence.
    Corral,
    /// Sawpit (a lumber mill): a log on trestles with a pit saw in it.
    Sawpit,
    /// Food, on a yield chip and in text: a tin can.
    Food,
    /// Wood, on a yield chip and in text: a log, its cut end up.
    Wood,
    /// Metal (scrap), on a yield chip and in text: a gear.
    Metal,
    /// Turns, in text: a clock face.
    Clock,
    /// Ruins, a cache in this theme: an olive supply crate with a hazard
    /// band.
    Ruins,
    /// Feral orchard: a gnarled fruit tree.
    FeralOrchard,
    /// Rubble pit (a quarry): stacked cinder blocks with rebar poking out.
    RubblePit,
}

impl MapIcon {
    pub(super) fn resource(resource: Resource) -> Self {
        match resource {
            Resource::Horses => Self::HorseHead,
            Resource::Iron => Self::CarWreck,
        }
    }

    pub(super) fn special(special: Special) -> Self {
        match special {
            Special::Orchard => Self::FeralOrchard,
            Special::Quarry => Self::RubblePit,
        }
    }

    /// An improvement's icon, by the label it's built with: the original
    /// label or its salvage-age name, so it keeps working whichever the
    /// sites carry.
    pub(super) fn improvement(label: &str) -> Option<Self> {
        match label {
            "FARM" | "HOMESTEAD" => Some(Self::Homestead),
            "MINE" | "SCRAP DIG" => Some(Self::ScrapDig),
            "PASTURE" | "CORRAL" => Some(Self::Corral),
            "LUMBER MILL" | "SAWPIT" => Some(Self::Sawpit),
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

/// Draws `icon` centered on `center`, `scale` times its usual size.
pub(super) fn push_map_icon_scaled(center: Vec2, icon: MapIcon, scale: f32, out: &mut Vec<Vertex>) {
    let mut pen = Pen::new(center, scale, out);
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
            // A dark muzzle, and a ragged mane down the neck: no one grooms
            // a feral herd.
            pen.shape(
                &[
                    (11.5, -4.0),
                    (13.0, -3.0),
                    (15.0, 3.0),
                    (10.0, 6.0),
                    (9.0, 0.5),
                ],
                MUZZLE,
                0.0,
            );
            pen.shape(
                &[
                    (0.0, -13.0),
                    (-5.0, -12.0),
                    (-4.5, -8.0),
                    (-9.5, -6.0),
                    (-8.0, -2.0),
                    (-12.0, 1.5),
                    (-9.5, 4.0),
                    (-12.5, 9.0),
                    (-9.0, 10.0),
                    (-7.0, 1.0),
                    (-3.0, -9.0),
                ],
                MANE,
                2.0,
            );
            pen.shape(&ellipse((4.0, -5.0), 1.6, 1.6, 0.0), OUTLINE, 0.0);
        }
        MapIcon::CarWreck => {
            // A sedan's body on its axles, rust through, windows gone.
            pen.shape(
                &[
                    (-18.0, 9.0),
                    (-18.0, 1.5),
                    (-14.0, -1.0),
                    (-8.0, -2.0),
                    (-4.0, -10.0),
                    (7.0, -10.0),
                    (11.0, -2.0),
                    (17.0, -1.0),
                    (18.5, 4.0),
                    (18.5, 9.0),
                ],
                RUST,
                3.0,
            );
            pen.line(&[(-3.0, -9.0), (6.0, -9.0)], 1.5, RUST_LIT);
            pen.line(&[(-16.0, 1.0), (16.0, 1.0)], 1.5, RUST_LIT);
            for window in [
                [(-6.5, -2.5), (-3.2, -8.3), (0.3, -8.3), (0.3, -2.5)],
                [(2.7, -2.5), (2.7, -8.3), (6.0, -8.3), (9.0, -2.5)],
            ] {
                pen.shape(&window, OUTLINE, 0.0);
            }
            pen.shape(&ellipse((-11.0, 4.5), 2.6, 1.8, 0.0), RUST_DARK, 0.0);
            pen.shape(&ellipse((12.5, 4.0), 2.0, 1.5, 0.0), RUST_DARK, 0.0);
            // Wheel wells, empty: it sits on the ground.
            for x in [-10.0, 10.0] {
                pen.shape(&arc((x, 9.0), 4.8, 4.8, 180.0, 360.0, 8), OUTLINE, 0.0);
            }
        }
        MapIcon::Homestead => {
            // Soil with three rows of crops, a lean-to shack behind.
            pen.shape(
                &[(-18.0, 13.0), (-15.0, 1.0), (5.0, 1.0), (2.0, 13.0)],
                SOIL,
                2.5,
            );
            for (y, from, to) in [(4.0, -14.0, 1.5), (7.5, -15.0, 0.5), (11.0, -16.0, -0.5)] {
                pen.line(&[(from, y), (to, y)], 2.2, CROP);
            }
            pen.shape(
                &[(4.0, -3.5), (17.0, -9.0), (17.0, 7.0), (4.0, 7.0)],
                PLANK,
                2.5,
            );
            pen.shape(&rect(8.5, 0.0, 12.5, 7.0), OUTLINE, 0.0);
            pen.shape(
                &[(1.5, -3.5), (19.0, -11.5), (20.0, -8.0), (2.5, -0.5)],
                RUST,
                2.5,
            );
        }
        MapIcon::ScrapDig => {
            // A heap of scrap: a wheel rim, a bent pipe, a plate.
            pen.shape(
                &[
                    (-18.0, 13.0),
                    (-15.0, 5.0),
                    (-9.0, 1.0),
                    (-6.0, -4.0),
                    (0.0, -5.0),
                    (5.0, -1.0),
                    (10.0, 0.0),
                    (14.0, 6.0),
                    (18.0, 13.0),
                ],
                RUST_DARK,
                2.5,
            );
            pen.ring(&ellipse((-8.5, 7.0), 3.6, 3.6, 0.0), 1.6, SHEET_LIT);
            pen.line(&[(-2.0, 1.0), (4.0, 4.0), (11.0, 4.0)], 1.8, SHEET_LIT);
            pen.shape(
                &[(-2.0, 8.0), (6.0, 6.5), (8.0, 11.5), (0.0, 12.5)],
                RUST,
                1.5,
            );
            // The pick, driven into the top of the heap.
            let handle = [(0.5, -1.0), (10.5, -17.0)];
            pen.line(&handle, 5.0, OUTLINE);
            pen.line(&handle, 2.4, PLANK_LIT);
            let head = [(4.0, -21.0), (10.8, -18.0), (17.5, -12.0)];
            pen.line(&head, 5.5, OUTLINE);
            pen.line(&head, 2.8, STEEL_LIT);
        }
        MapIcon::Corral => {
            // Two pallets on end: deck boards across, the stringers showing
            // dark between them; the right one leans, lashed to the left.
            for (left, lean) in [(-16.0, 0.0), (1.5, 0.06)] {
                let at = |x: f32, y: f32| (x - lean * y, y);
                for x in [left, left + 6.25, left + 12.5] {
                    let stringer = [
                        at(x, -13.0),
                        at(x + 3.5, -13.0),
                        at(x + 3.5, 13.0),
                        at(x, 13.0),
                    ];
                    pen.shape(&stringer, BARK, 2.0);
                }
                for y in [-13.0, -6.0, 1.0, 8.0] {
                    let board = [
                        at(left - 0.5, y),
                        at(left + 16.5, y),
                        at(left + 16.5, y + 4.5),
                        at(left - 0.5, y + 4.5),
                    ];
                    pen.shape(&board, PLANK_LIT, 2.0);
                }
            }
            pen.line(&[(-2.5, -11.0), (3.5, -8.0)], 1.4, BONE);
            pen.line(&[(-2.5, -8.0), (3.5, -11.0)], 1.4, BONE);
        }
        MapIcon::Sawpit => {
            // Two sawhorses, the log across them, the saw's blade down
            // through it.
            for x in [-10.0, 10.0] {
                for leg in [
                    [(x - 5.0, 14.0), (x + 3.0, -1.0)],
                    [(x + 5.0, 14.0), (x - 3.0, -1.0)],
                ] {
                    pen.line(&leg, 4.5, OUTLINE);
                    pen.line(&leg, 2.2, PLANK_LIT);
                }
            }
            let (y, half) = (-3.0, 5.0);
            let mut log = vec![(-16.0, y - half)];
            log.extend(arc((15.0, y), 2.5, half, -90.0, 90.0, 6));
            log.push((-16.0, y + half));
            pen.shape(&log, BARK, 3.0);
            pen.shape(&ellipse((-16.0, y), 3.0, half, 0.0), CUT_WOOD, 2.5);
            pen.ring(&ellipse((-16.0, y), 1.2, 2.2, 0.0), 1.2, GROWTH_RING);
            pen.shape(&rect(2.0, -16.0, 5.0, 9.0), STEEL_LIT, 2.0);
            for i in 0..4 {
                let ty = -12.0 + 5.5 * i as f32;
                pen.shape(
                    &[(5.0, ty), (7.0, ty + 1.5), (5.0, ty + 3.0)],
                    STEEL_LIT,
                    0.0,
                );
            }
            let grip = [(-2.0, -17.5), (9.0, -17.5)];
            pen.line(&grip, 5.0, OUTLINE);
            pen.line(&grip, 2.4, PLANK_LIT);
        }
        // The small resource icons are flat, a color or two each, thinly
        // edged, so they read at text size.
        MapIcon::Food => {
            // A tin can: a red label between two tin rims, its lid on top.
            pen.shape(&rect(-5.5, -7.0, 5.5, 8.0), CAN_LABEL, 1.2);
            pen.shape(&rect(-5.5, 5.5, 5.5, 8.0), TIN, 0.0);
            pen.shape(&rect(-5.5, -7.0, 5.5, -4.5), TIN, 0.0);
            pen.shape(&ellipse((0.0, -7.0), 5.5, 1.8, 0.0), TIN, 1.0);
        }
        MapIcon::Wood => {
            // A log leaning right, its cut end at the top.
            let log = [(-3.5, 6.0), (3.0, -5.0)];
            pen.line(&log, 6.2, OUTLINE);
            pen.line(&log, 4.0, PLANK);
            pen.shape(&ellipse((3.3, -5.5), 2.6, 2.6, 0.0), CUT_WOOD, 1.1);
        }
        MapIcon::Metal => {
            // A six-toothed gear with its axle hole.
            let teeth: Vec<(f32, f32)> = (0..24)
                .map(|k| {
                    let angle = (k as f32 * 15.0 - 7.5).to_radians();
                    let radius = if (k / 2) % 2 == 0 { 5.9 } else { 4.1 };
                    (radius * angle.cos(), radius * angle.sin())
                })
                .collect();
            pen.shape(&teeth, METAL, 1.2);
            pen.shape(&ellipse((0.0, 0.0), 1.7, 1.7, 0.0), OUTLINE, 0.0);
        }
        MapIcon::Clock => {
            // An open ring with two hands, all in one color.
            pen.ring(&ellipse((0.0, 0.0), 7.5, 7.5, 0.0), 1.0, CLOCK_FACE);
            pen.line(&[(0.0, 0.0), (0.0, -4.5)], 1.0, CLOCK_FACE);
            pen.line(&[(0.0, 0.0), (3.5, 0.0)], 1.0, CLOCK_FACE);
        }
        MapIcon::Ruins => {
            // A supply crate from before the fall, seen from above and to
            // the left, a hazard band round its front.
            pen.shape(
                &[(-15.0, -2.0), (-9.0, -8.0), (15.0, -8.0), (9.0, -2.0)],
                OLIVE_LIT,
                2.5,
            );
            pen.shape(
                &[(9.0, -2.0), (15.0, -8.0), (15.0, 6.0), (9.0, 12.0)],
                OLIVE_SHADE,
                2.5,
            );
            pen.shape(&rect(-15.0, -2.0, 9.0, 12.0), OLIVE_DRAB, 2.5);
            pen.shape(&rect(-15.0, 2.5, 9.0, 7.5), AMBER, 0.0);
            for x in [-14.0, -8.5, -3.0, 2.5] {
                let stripe = [(x, 7.5), (x + 2.5, 7.5), (x + 5.0, 2.5), (x + 2.5, 2.5)];
                pen.shape(&stripe, OUTLINE, 0.0);
            }
            pen.line(&[(-2.0, -7.0), (2.0, -3.0)], 1.5, OLIVE_SHADE);
            pen.line(&[(-9.0, -2.5), (9.0, -2.5)], 1.0, OLIVE_SHADE);
        }
        MapIcon::FeralOrchard => {
            // A lumpy crown, then the twisted trunk and its branches over
            // it, then the fruit.
            for (center, radius, fill) in [
                ((-9.0, -6.0), 6.5, LEAVES),
                ((7.0, -5.0), 6.0, LEAVES),
                ((-1.0, -11.0), 7.0, LEAVES_LIT),
            ] {
                pen.shape(&ellipse(center, radius, radius * 0.9, 0.0), fill, 2.5);
            }
            pen.shape(
                &[
                    (-4.5, 13.0),
                    (-2.0, 7.0),
                    (-4.5, 2.0),
                    (-10.0, -3.0),
                    (-8.0, -4.5),
                    (-2.5, -0.5),
                    (-1.0, -6.0),
                    (1.5, -5.5),
                    (1.0, -1.0),
                    (7.5, -4.0),
                    (8.5, -2.0),
                    (3.0, 2.0),
                    (2.0, 7.0),
                    (4.0, 13.0),
                ],
                BARK,
                2.5,
            );
            for (x, y) in [(-11.0, -8.0), (-3.0, -13.0), (4.0, -10.0), (9.0, -6.5)] {
                pen.shape(&ellipse((x, y), 2.3, 2.3, 0.0), FRUIT, 1.2);
            }
        }
        MapIcon::RubblePit => {
            // Cinder blocks in a pyramid, rebar bent out of the pile.
            let rebar = [(-3.0, -6.0), (-8.0, -14.0), (-4.0, -18.0)];
            pen.line(&rebar, 4.5, OUTLINE);
            pen.line(&rebar, 2.0, RUST_LIT);
            for (x, y) in [(-15.0, 2.0), (1.0, 2.0), (-7.0, -9.0)] {
                pen.shape(&rect(x, y, x + 14.0, y + 10.0), CONCRETE, 2.5);
                pen.shape(&rect(x, y, x + 14.0, y + 2.5), CONCRETE_LIT, 0.0);
                for hole in [x + 2.5, x + 8.5] {
                    pen.shape(
                        &rect(hole, y + 4.5, hole + 3.0, y + 8.0),
                        CONCRETE_SHADE,
                        0.0,
                    );
                }
            }
        }
    }
}

/// A city building's sign: a plate of sheet metal with cut corners, painted
/// in the building's color, with its pictogram in dark stencil on it,
/// centered on `center`. The Garrison (`Building::Barracks`) has its own
/// mark on the map (`push_garrison`), but draws a plate here too.
pub(super) fn push_building(center: Vec2, building: Building, out: &mut Vec<Vertex>) {
    let plate = plate_color(building);
    let mut pen = Pen::new(center, 1.0, out);
    let (half, cut) = (23.0, 5.5);
    pen.shape(
        &[
            (-half + cut, -half),
            (half - cut, -half),
            (half, -half + cut),
            (half, half - cut),
            (half - cut, half),
            (-half + cut, half),
            (-half, half - cut),
            (-half, -half + cut),
        ],
        plate,
        3.0,
    );
    // Rivets in the corners.
    for (x, y) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        let at = (x * (half - 5.0), y * (half - 5.0));
        pen.shape(&ellipse(at, 1.1, 1.1, 0.0), darker(plate, 0.45), 0.0);
    }
    building_pictogram(&mut pen, building, plate);
}

/// The plates' paint: the old letter badges' colors, weathered, so a
/// building keeps its color.
fn plate_color(building: Building) -> Color {
    match building {
        Building::Barracks => [0.50, 0.22, 0.10, 1.0],
        Building::Mill => [0.26, 0.44, 0.18, 1.0],
        Building::Workshop => [0.26, 0.34, 0.55, 1.0],
        Building::CanoeHouse => [0.20, 0.44, 0.55, 1.0],
        Building::Forge => [0.60, 0.28, 0.12, 1.0],
        Building::Stable => [0.48, 0.33, 0.17, 1.0],
        Building::Watchpost => [0.55, 0.52, 0.22, 1.0],
        Building::FieldHospital => [0.58, 0.19, 0.19, 1.0],
        Building::Cannery => [0.28, 0.50, 0.30, 1.0],
        Building::WorkCamp => [0.46, 0.37, 0.22, 1.0],
        Building::Smelter => [0.40, 0.14, 0.09, 1.0],
        Building::Railhead => [0.32, 0.47, 0.56, 1.0],
        Building::Harbor => [0.20, 0.40, 0.66, 1.0],
        Building::CoastalBattery => [0.62, 0.40, 0.16, 1.0],
    }
}

/// A building's pictogram, within 18 design units of the plate's center.
/// `plate` is the paint behind it, used to cut gaps between crossing parts.
fn building_pictogram(pen: &mut Pen, building: Building, plate: Color) {
    match building {
        Building::Barracks => {
            // A Quonset hut: an arched sheet-metal roof with a door.
            let hut = quonset(15.0, 12.0, 12.0);
            pen.shape(&hut, INK, 0.0);
            pen.shape(&rect(-3.5, 3.0, 3.5, 12.0), plate, 0.0);
            for x in [-9.0, 9.0] {
                pen.line(&[(x, -3.0), (x, 10.0)], 1.2, plate);
            }
        }
        Building::Mill => {
            // A wind pump: a lattice tower, a wheel of many blades, a vane.
            for leg in [[(-8.0, 17.0), (-1.5, -5.0)], [(8.0, 17.0), (1.5, -5.0)]] {
                pen.line(&leg, 2.2, INK);
            }
            pen.line(&[(-6.5, 12.0), (4.5, 3.0)], 1.2, INK);
            pen.line(&[(6.5, 12.0), (-4.5, 3.0)], 1.2, INK);
            let hub = (-1.0, -7.0);
            pen.line(&[hub, (13.0, -7.0)], 1.6, INK);
            pen.shape(
                &[(10.0, -11.5), (17.0, -10.5), (17.0, -3.5), (10.0, -2.5)],
                INK,
                0.0,
            );
            for i in 0..12 {
                let angle = (i as f32 * 30.0).to_radians();
                let dir = Vec2::from_angle(angle);
                let side = dir.perp();
                let point = |r: f32, w: f32| {
                    let p = dir * r + side * w;
                    (hub.0 + p.x, hub.1 + p.y)
                };
                pen.shape(
                    &[
                        point(3.0, -0.8),
                        point(10.5, -2.4),
                        point(10.5, 2.0),
                        point(3.0, 0.8),
                    ],
                    INK,
                    0.0,
                );
            }
            pen.ring(&ellipse(hub, 10.0, 10.0, 0.0), 1.2, INK);
            pen.shape(&ellipse(hub, 2.6, 2.6, 0.0), INK, 0.0);
        }
        Building::Workshop => {
            // A wrench and a hammer, crossed.
            let shank = [(-12.0, 12.0), (8.5, -8.5)];
            pen.line(&shank, 4.5, INK);
            pen.shape(&ellipse((11.0, -11.0), 6.2, 6.2, 0.0), INK, 0.0);
            let (d, p) = (Vec2::new(0.707, -0.707), Vec2::new(0.707, 0.707));
            let jaw = |along: f32, across: f32| {
                let v = Vec2::new(11.0, -11.0) + d * along + p * across;
                (v.x, v.y)
            };
            pen.shape(
                &[
                    jaw(-0.5, -2.3),
                    jaw(8.0, -2.3),
                    jaw(8.0, 2.3),
                    jaw(-0.5, 2.3),
                ],
                plate,
                0.0,
            );
            pen.shape(&ellipse((-12.5, 12.5), 4.2, 4.2, 0.0), INK, 0.0);
            pen.shape(&ellipse((-12.5, 12.5), 1.8, 1.8, 0.0), plate, 0.0);
            // The hammer over it, a gap of plate around it.
            let handle = [(13.0, 13.0), (-5.0, -5.0)];
            pen.line(&handle, 7.0, plate);
            pen.line(&handle, 3.4, INK);
            let head = |along: f32, across: f32| {
                let v = Vec2::new(-6.5, -6.5) + Vec2::new(-0.707, -0.707) * along + p * across;
                (v.x, v.y)
            };
            let head_shape = [
                head(-4.0, -8.0),
                head(4.0, -9.5),
                head(4.0, 7.5),
                head(-4.0, 6.0),
            ];
            pen.shape(&head_shape, INK, 0.0);
        }
        Building::CanoeHouse => {
            // A boat on its stocks, a plank line along the hull.
            for x in [-8.0, 8.0] {
                pen.line(&[(x - 4.5, 17.0), (x, 7.0), (x + 4.5, 17.0)], 2.2, INK);
            }
            let mut hull = vec![(-18.0, -8.0), (-12.0, -4.0), (12.0, -4.0), (18.0, -8.0)];
            hull.extend(arc((0.0, -4.0), 16.0, 12.0, 10.0, 170.0, 8));
            pen.shape(&hull, INK, 0.0);
            pen.line(&[(-12.5, 1.5), (12.5, 1.5)], 1.2, plate);
            pen.line(&[(0.0, -4.0), (0.0, -17.0)], 2.0, INK);
            pen.shape(&[(1.5, -17.0), (11.0, -7.5), (1.5, -7.5)], INK, 0.0);
        }
        Building::Forge => {
            // An anvil, sparks flying off it.
            for spark in [
                [(0.0, -10.5), (-6.0, -17.0)],
                [(3.0, -10.5), (3.5, -19.0)],
                [(6.0, -10.5), (12.0, -16.5)],
            ] {
                pen.line(&spark, 4.2, INK);
                pen.line(&spark, 2.0, FLAME_CORE);
            }
            pen.shape(
                &[
                    (-18.0, -7.0),
                    (-8.0, -8.5),
                    (15.0, -8.5),
                    (15.0, -3.0),
                    (7.0, -1.5),
                    (5.0, 4.0),
                    (10.0, 8.5),
                    (10.0, 13.0),
                    (-9.0, 13.0),
                    (-9.0, 8.5),
                    (-4.0, 4.0),
                    (-6.0, -1.5),
                    (-10.0, -3.0),
                ],
                INK,
                0.0,
            );
        }
        Building::Stable => {
            // A horseshoe, open side down, with its nail holes.
            let shoe = arc((0.0, 1.0), 11.0, 11.0, 130.0, 410.0, 14);
            pen.line(&shoe, 7.0, INK);
            for angle in [165.0f32, 205.0, 240.0, 300.0, 335.0, 375.0] {
                let a = angle.to_radians();
                pen.shape(
                    &ellipse((11.0 * a.cos(), 1.0 + 11.0 * a.sin()), 1.1, 1.1, 0.0),
                    plate,
                    0.0,
                );
            }
        }
        Building::Watchpost => {
            // An old lattice radio mast, a lookout platform rigged on it,
            // still sending.
            let leg_x = |y: f32| 1.5 + (y + 15.0) * 7.5 / 32.0;
            for side in [-1.0, 1.0] {
                pen.line(
                    &[(side * leg_x(17.0), 17.0), (side * leg_x(-15.0), -15.0)],
                    2.0,
                    INK,
                );
            }
            let levels = [17.0, 9.0, 2.0, -4.0, -10.0, -15.0];
            for pair in levels.windows(2) {
                let (low, high) = (pair[0], pair[1]);
                pen.line(&[(-leg_x(low), low), (leg_x(high), high)], 1.1, INK);
                pen.line(&[(leg_x(low), low), (-leg_x(high), high)], 1.1, INK);
            }
            pen.shape(&rect(-8.5, -6.0, 8.5, -2.5), INK, 0.0);
            pen.line(&[(0.0, -15.0), (0.0, -21.0)], 1.6, INK);
            for radius in [5.0, 8.5] {
                pen.line(&arc((0.0, -16.0), radius, radius, -40.0, 40.0, 5), 1.6, INK);
                pen.line(
                    &arc((0.0, -16.0), radius, radius, 140.0, 220.0, 5),
                    1.6,
                    INK,
                );
            }
        }
        Building::FieldHospital => {
            // A bone-white cross.
            let (arm, reach) = (4.8, 14.5);
            pen.shape(
                &[
                    (-arm, -reach),
                    (arm, -reach),
                    (arm, -arm),
                    (reach, -arm),
                    (reach, arm),
                    (arm, arm),
                    (arm, reach),
                    (-arm, reach),
                    (-arm, arm),
                    (-reach, arm),
                    (-reach, -arm),
                    (-arm, -arm),
                ],
                BONE,
                3.0,
            );
        }
        Building::Cannery => {
            // Three tins stacked.
            for (x, y) in [(-7.5, 3.0), (7.5, 3.0), (0.0, -11.0)] {
                tin_can(pen, (x, y), 6.5, 13.0);
            }
        }
        Building::WorkCamp => {
            // A tarp tent on a ridge pole, a shovel stuck in beside it.
            pen.line(&[(-3.0, -14.0), (-3.0, -17.0)], 2.0, INK);
            pen.shape(&[(-18.0, 15.0), (-3.0, -13.0), (12.0, 15.0)], TARP, 2.5);
            pen.shape(&[(-7.0, 15.0), (-3.0, 5.0), (1.0, 15.0)], INK, 0.0);
            let handle = [(15.0, 8.0), (15.0, -14.0)];
            pen.line(&handle, 6.0, plate);
            pen.line(&handle, 2.2, INK);
            pen.line(&[(12.0, -15.0), (18.0, -15.0)], 2.0, INK);
            pen.shape(
                &[
                    (11.5, 7.0),
                    (18.5, 7.0),
                    (18.0, 14.0),
                    (15.0, 17.0),
                    (12.0, 14.0),
                ],
                INK,
                0.0,
            );
        }
        Building::Smelter => {
            // A crucible pouring into an ingot mold.
            pen.shape(
                &[(-17.0, -9.0), (-8.0, -17.0), (0.0, -8.0), (-9.0, 0.0)],
                INK,
                0.0,
            );
            let pour = [(-1.0, -8.5), (3.5, -6.0), (5.0, 3.5)];
            pen.line(&pour, 5.0, INK);
            pen.line(&pour, 2.6, FLAME_CORE);
            pen.shape(
                &[(-7.0, 5.0), (17.0, 5.0), (14.0, 13.0), (-4.0, 13.0)],
                INK,
                0.0,
            );
            pen.shape(
                &[(-4.5, 6.8), (14.5, 6.8), (13.8, 9.0), (-3.8, 9.0)],
                FLAME,
                0.0,
            );
            pen.shape(
                &[(-10.0, -12.0), (-6.5, -15.0), (-4.0, -12.0), (-7.5, -9.0)],
                FLAME,
                0.0,
            );
        }
        Building::Railhead => {
            // A handcar on the rail: deck, wheels, and its pump lever.
            pen.line(&[(-19.0, 15.5), (19.0, 15.5)], 2.0, INK);
            pen.shape(&rect(-15.0, 1.5, 15.0, 6.0), INK, 0.0);
            for x in [-9.0, 9.0] {
                pen.shape(&ellipse((x, 10.0), 4.5, 4.5, 0.0), INK, 0.0);
                pen.shape(&ellipse((x, 10.0), 1.3, 1.3, 0.0), plate, 0.0);
            }
            pen.line(&[(0.0, 2.0), (0.0, -8.0)], 2.6, INK);
            let lever = [(-13.0, -13.0), (13.0, -4.0)];
            pen.line(&lever, 2.6, INK);
            pen.line(&[(-14.5, -9.0), (-11.5, -17.0)], 2.2, INK);
            pen.line(&[(11.5, -0.5), (14.5, -8.5)], 2.2, INK);
        }
        Building::Harbor => {
            // An anchor.
            pen.ring(&ellipse((0.0, -14.0), 3.5, 3.5, 0.0), 2.4, INK);
            pen.line(&[(0.0, -10.5), (0.0, 13.0)], 3.6, INK);
            pen.line(&[(-8.0, -7.0), (8.0, -7.0)], 3.0, INK);
            let arms = arc((0.0, 1.0), 13.0, 12.0, 15.0, 165.0, 10);
            pen.line(&arms, 3.6, INK);
            for side in [-1.0, 1.0] {
                pen.shape(
                    &[(side * 16.5, 1.0), (side * 9.5, 3.5), (side * 14.0, 8.5)],
                    INK,
                    0.0,
                );
            }
        }
        Building::CoastalBattery => {
            // A harpoon on its pivot mount, rope trailing from its butt.
            pen.shape(&[(-9.0, 17.0), (7.0, 17.0), (-1.0, 3.0)], INK, 0.0);
            let rope = [(-12.0, 12.0), (-17.0, 9.0), (-13.5, 5.5), (-17.5, 1.5)];
            pen.line(&rope, 3.6, INK);
            pen.line(&rope, 1.5, BONE);
            pen.line(&[(-13.0, 13.0), (10.0, -10.0)], 2.8, INK);
            let (d, p) = (Vec2::new(0.707, -0.707), Vec2::new(0.707, 0.707));
            let head = |along: f32, across: f32| {
                let v = Vec2::new(10.0, -10.0) + d * along + p * across;
                (v.x, v.y)
            };
            pen.shape(
                &[
                    head(9.5, 0.0),
                    head(-2.5, 5.5),
                    head(0.5, 0.0),
                    head(-2.5, -5.5),
                ],
                INK,
                0.0,
            );
        }
    }
}

/// A tin can centered on `center`, `half` wide each way and `height` tall,
/// its lid on top: tin rims round a red label.
fn tin_can(pen: &mut Pen, (x, y): (f32, f32), half: f32, height: f32) {
    let (top, bottom) = (y - height / 2.0, y + height / 2.0);
    pen.shape(&rect(x - half, top, x + half, bottom), TIN, 2.2);
    pen.shape(
        &rect(x - half, top + 3.0, x + half, bottom - 2.5),
        CAN_LABEL,
        0.0,
    );
    pen.shape(&ellipse((x, top), half, 2.0, 0.0), TIN, 2.0);
}

/// A Quonset hut's side: a long body with a rounded roof, `half` wide,
/// its floor at `floor` and `height` tall.
fn quonset(half: f32, floor: f32, height: f32) -> Vec<(f32, f32)> {
    let radius = height.min(half) * 0.8;
    let top = floor - height;
    let mut points = vec![(-half, floor), (half, floor)];
    points.extend(arc(
        (half - radius, top + radius),
        radius,
        radius,
        0.0,
        -90.0,
        5,
    ));
    points.extend(arc(
        (-half + radius, top + radius),
        radius,
        radius,
        -90.0,
        -180.0,
        5,
    ));
    points
}

/// A garrison on the map, `color` its side's: a Quonset hut of sheet metal
/// in that color, its ribs showing, a door between sandbag stacks. The same
/// size as the barracks house it replaces (0.76 wide, 0.72 tall).
pub(super) fn push_garrison(center: Vec2, color: Color, out: &mut Vec<Vertex>) {
    let mut pen = Pen::new(center, 1.0, out);
    pen.alpha = color[3];
    let body = quonset(38.0, 32.0, 50.0);
    pen.shape(&body, color, 6.0);
    // Corrugation ribs, from the floor up to the roof.
    let shade = darker(color, 0.55);
    for x in [-26.0, -13.0, 13.0, 26.0] {
        let top = roof_height(&body, x) + 2.5;
        pen.line(&[(x, 30.0), (x, top)], 2.2, shade);
    }
    pen.shape(&rect(-8.0, 10.0, 8.0, 32.0), OUTLINE, 0.0);
    pen.shape(&ellipse((0.0, -6.0), 5.5, 5.5, 0.0), OUTLINE, 0.0);
    pen.shape(&ellipse((0.0, -6.0), 3.0, 3.0, 0.0), shade, 0.0);
    for x in [-21.0, 21.0] {
        for (dx, y) in [(-5.0, 28.0), (5.0, 28.0), (0.0, 22.0)] {
            pen.shape(&ellipse((x + dx, y), 5.5, 3.4, 0.0), SANDBAG, 2.0);
        }
    }
}

/// The highest point of `outline` over `x`: where a vertical line at `x`
/// first meets it coming down from the top (design Y points down).
fn roof_height(outline: &[(f32, f32)], x: f32) -> f32 {
    let mut top = f32::MAX;
    for i in 0..outline.len() {
        let (a, b) = (outline[i], outline[(i + 1) % outline.len()]);
        let (lo, hi) = if a.0 < b.0 { (a, b) } else { (b, a) };
        if lo.0 <= x && x <= hi.0 && hi.0 > lo.0 {
            let t = (x - lo.0) / (hi.0 - lo.0);
            top = top.min(lo.1 + t * (hi.1 - lo.1));
        }
    }
    top
}

/// An enclave's mark, `color` its side's: a patched-up old tower block,
/// its left half standing, its right broken off with rebar showing and a
/// sheet-metal patch on it. `scale` times its size on the map (0.84 wide,
/// 0.8 tall), in whatever space `center` is in: the turn strip draws it too.
/// Its middle is left plain for the population drawn on it.
pub(super) fn push_enclave(center: Vec2, scale: f32, color: Color, out: &mut Vec<Vertex>) {
    let mut pen = Pen::new(center, scale / HEX_SIZE, out);
    pen.alpha = color[3];
    for rebar in [
        [(18.0, -26.0), (22.0, -37.0)],
        [(29.0, -19.0), (34.0, -28.0)],
    ] {
        pen.line(&rebar, 4.5, OUTLINE);
        pen.line(&rebar, 2.0, RUST_LIT);
    }
    pen.shape(
        &[
            (-42.0, 40.0),
            (42.0, 40.0),
            (42.0, -18.0),
            (35.0, -23.0),
            (27.0, -18.0),
            (20.0, -28.0),
            (13.0, -23.0),
            (8.0, -40.0),
            (-42.0, -40.0),
        ],
        color,
        10.0,
    );
    let shade = darker(color, 0.35);
    for x in [-35.0, -24.0, -13.0, -2.0] {
        for y in [-35.0, -25.0] {
            pen.shape(&rect(x, y, x + 6.0, y + 6.0), shade, 0.0);
        }
    }
    let patch = [(24.0, -12.0), (39.0, -13.0), (39.0, 3.0), (25.0, 4.0)];
    let sheet = mix(color, SHEET_LIT, 0.55);
    pen.shape(&patch, sheet, 2.0);
    pen.line(&[(26.0, -4.5), (37.0, -5.0)], 1.5, darker(sheet, 0.6));
}

/// A watchfire on a tile, in its side's `color`: a fire in an oil drum, on
/// a platform of that color up on scaffold legs.
pub(super) fn push_watchfire(center: Vec2, color: Color, out: &mut Vec<Vertex>) {
    let mut pen = Pen::new(center, 1.0, out);
    for leg in [[(-16.0, 36.0), (-10.0, -2.0)], [(16.0, 36.0), (10.0, -2.0)]] {
        pen.line(&leg, 7.0, OUTLINE);
        pen.line(&leg, 3.5, STEEL);
    }
    for brace in [[(-14.0, 26.0), (11.5, 8.0)], [(14.0, 26.0), (-11.5, 8.0)]] {
        pen.line(&brace, 4.5, OUTLINE);
        pen.line(&brace, 1.8, STEEL);
    }
    pen.shape(
        &[
            (-11.0, -26.0),
            (-13.0, -34.0),
            (-7.5, -38.0),
            (-6.0, -46.0),
            (-1.0, -40.0),
            (2.0, -50.0),
            (6.5, -39.0),
            (10.5, -43.0),
            (13.0, -33.0),
            (11.0, -26.0),
        ],
        FLAME,
        3.0,
    );
    pen.shape(
        &[
            (-6.5, -26.0),
            (-6.5, -32.0),
            (-2.5, -35.0),
            (1.0, -42.0),
            (4.0, -34.0),
            (6.5, -31.0),
            (6.5, -26.0),
        ],
        FLAME_CORE,
        0.0,
    );
    pen.shape(&rect(-10.5, -27.0, 10.5, -6.0), RUST, 3.0);
    for y in [-20.0, -13.0] {
        pen.line(&[(-10.5, y), (10.5, y)], 1.8, RUST_DARK);
    }
    pen.shape(&rect(-10.5, -27.0, 10.5, -24.5), RUST_LIT, 0.0);
    pen.shape(&rect(-20.0, -6.0, 20.0, 1.0), color, 3.0);
}

/// A bunker around a tile, in its side's `color`: a ring of that color
/// inside a ring of sandbags, concrete blocks at the hex's corners.
pub(super) fn push_bunker(center: Vec2, color: Color, out: &mut Vec<Vertex>) {
    mesh::polygon_outline(center, 0.56 * HEX_SIZE, 0.05 * HEX_SIZE, 6, 0.0, color, out);
    let mut pen = Pen::new(center, 1.0, out);
    const BAGS: usize = 18;
    for i in 0..BAGS {
        let degrees = 360.0 * (i as f32 + 0.5) / BAGS as f32;
        let dir = Vec2::from_angle(degrees.to_radians()) * 64.0;
        pen.shape(
            &ellipse((dir.x, dir.y), 11.5, 6.0, degrees + 90.0),
            SANDBAG,
            2.5,
        );
    }
    for i in 0..6 {
        let degrees = 60.0 * i as f32;
        let dir = Vec2::from_angle(degrees.to_radians());
        let side = dir.perp();
        let corner = |along: f32, across: f32| {
            let p = dir * (64.0 + along) + side * across;
            (p.x, p.y)
        };
        let block = [
            corner(-6.5, -6.5),
            corner(6.5, -6.5),
            corner(6.5, 6.5),
            corner(-6.5, 6.5),
        ];
        pen.shape(&block, CONCRETE, 2.5);
        let top = [
            corner(-6.5, -6.5),
            corner(-2.5, -6.5),
            corner(-2.5, 6.5),
            corner(-6.5, 6.5),
        ];
        pen.shape(&top, CONCRETE_LIT, 0.0);
    }
}

/// A tyre wall or bus gate along a hex edge from `start` to `end`, `width`
/// thick, with posts in its side's `color` at both ends: a sheet-metal
/// fence with tyres stacked against it, or, for a gate, a school bus rolled
/// across the gap with a stripe of its side's color on its roof.
pub(super) fn push_barrier(
    start: Vec2,
    end: Vec2,
    width: f32,
    gate: bool,
    color: Color,
    out: &mut Vec<Vertex>,
) {
    let length = start.distance(end);
    let mut pen = Pen::new((start + end) / 2.0, length / HEX_SIZE, out);
    pen.axis = (end - start).normalize_or_zero();
    // Design units across: the wall's `width` in the edge's 100 units.
    let half = width / length * 50.0;
    pen.shape(&rect(-50.0, -half * 0.7, 50.0, half * 0.7), RUST, 4.0);
    for i in 0..4 {
        let x = -37.5 + 25.0 * i as f32;
        pen.line(&[(x, -half * 0.7), (x, half * 0.7)], 1.2, RUST_DARK);
    }
    let tyres: &[f32] = if gate {
        &[-34.0, 34.0]
    } else {
        &[-30.0, -10.0, 10.0, 30.0]
    };
    for &x in tyres {
        pen.shape(&ellipse((x, 0.0), half * 0.95, half * 0.95, 0.0), TYRE, 2.5);
        pen.ring(
            &ellipse((x, 0.0), half * 0.65, half * 0.65, 0.0),
            1.4,
            TREAD,
        );
        pen.shape(
            &ellipse((x, 0.0), half * 0.3, half * 0.3, 0.0),
            RUST_DARK,
            0.0,
        );
    }
    if gate {
        let bus = half * 1.3;
        pen.shape(&rect(-22.0, -bus, 22.0, bus), AMBER, 3.0);
        for row in [-1.0, 1.0] {
            for i in 0..5 {
                let x = -16.0 + 7.0 * i as f32;
                let y = row * bus * 0.68;
                pen.shape(
                    &rect(x, y - bus * 0.18, x + 4.5, y + bus * 0.18),
                    OUTLINE,
                    0.0,
                );
            }
        }
        pen.shape(&rect(-22.0, -bus * 0.2, 22.0, bus * 0.2), color, 0.0);
    }
    for x in [-50.0, 50.0] {
        let post = half * 1.24;
        pen.shape(&rect(x - post, -post, x + post, post), color, 4.0);
    }
}

/// `color` darkened to `factor` of its brightness.
fn darker([r, g, b, a]: Color, factor: f32) -> Color {
    [r * factor, g * factor, b * factor, a]
}

/// `a` mixed toward `b` by `t`, keeping `a`'s alpha.
fn mix(a: Color, b: Color, t: f32) -> Color {
    let lerp = |x: f32, y: f32| x + (y - x) * t;
    [lerp(a[0], b[0]), lerp(a[1], b[1]), lerp(a[2], b[2]), a[3]]
}

/// The corners of the rectangle from (`x0`, `y0`) to (`x1`, `y1`).
fn rect(x0: f32, y0: f32, x1: f32, y1: f32) -> [(f32, f32); 4] {
    [(x0, y0), (x1, y0), (x1, y1), (x0, y1)]
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

/// `steps + 1` points along an ellipse's arc from `from` to `to` degrees
/// (clockwise on screen, 0 pointing right, with Y down).
fn arc(center: (f32, f32), rx: f32, ry: f32, from: f32, to: f32, steps: usize) -> Vec<(f32, f32)> {
    (0..=steps)
        .map(|i| {
            let angle = (from + (to - from) * i as f32 / steps as f32).to_radians();
            (center.0 + rx * angle.cos(), center.1 + ry * angle.sin())
        })
        .collect()
}

/// Draws shapes given in design coordinates, scaled onto the map.
struct Pen<'a> {
    center: Vec2,
    /// World units per design unit.
    scale: f32,
    /// Where the design's X axis points on the map (its Y is a quarter turn
    /// clockwise from it on screen).
    axis: Vec2,
    /// Every color's alpha is multiplied by this.
    alpha: f32,
    out: &'a mut Vec<Vertex>,
}

impl<'a> Pen<'a> {
    /// A pen for a hex's design, `scale` times its usual size, centered on
    /// `center`.
    fn new(center: Vec2, scale: f32, out: &'a mut Vec<Vertex>) -> Self {
        Self {
            center,
            scale: scale * HEX_SIZE / DESIGN_HEX_RADIUS,
            axis: Vec2::X,
            alpha: 1.0,
            out,
        }
    }

    fn at(&self, (x, y): (f32, f32)) -> Vec2 {
        self.center + (self.axis * x - self.axis.perp() * y) * self.scale
    }

    fn paint(&self, [r, g, b, a]: Color) -> Color {
        [r, g, b, a * self.alpha]
    }

    /// A filled outline, edged in dark `edge` wide (half of it outside).
    fn shape(&mut self, points: &[(f32, f32)], fill: Color, edge: f32) {
        let points: Vec<Vec2> = points.iter().map(|&p| self.at(p)).collect();
        if edge > 0.0 {
            let outline = self.paint(OUTLINE);
            mesh::outline(&points, edge * self.scale, outline, self.out);
        }
        mesh::polygon(&points, self.paint(fill), self.out);
    }

    /// Just the outline through `points`, `width` wide.
    fn ring(&mut self, points: &[(f32, f32)], width: f32, color: Color) {
        let points: Vec<Vec2> = points.iter().map(|&p| self.at(p)).collect();
        mesh::outline(&points, width * self.scale, self.paint(color), self.out);
    }

    /// A line through `points` with rounded ends.
    fn line(&mut self, points: &[(f32, f32)], width: f32, color: Color) {
        let world: Vec<Vec2> = points.iter().map(|&p| self.at(p)).collect();
        let color = self.paint(color);
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
        MapIcon::CarWreck,
        MapIcon::Homestead,
        MapIcon::ScrapDig,
        MapIcon::Corral,
        MapIcon::Sawpit,
        MapIcon::Ruins,
        MapIcon::FeralOrchard,
        MapIcon::RubblePit,
    ];

    /// Whether `p` is inside the flat-top hexagon of `radius` around the origin.
    fn inside_hex(p: Vec2, radius: f32) -> bool {
        let half_height = radius * 3f32.sqrt() / 2.0;
        p.y.abs() <= half_height && 3f32.sqrt() * p.x.abs() + p.y.abs() <= 2.0 * half_height
    }

    fn positions(out: &[Vertex]) -> impl Iterator<Item = Vec2> + '_ {
        out.iter().map(|v| Vec2::new(v.pos[0], v.pos[1]))
    }

    #[test]
    fn every_map_icon_stays_in_its_corner() {
        // The unit token's reach: radius 0.36 plus half its outline.
        let token = 0.39 * HEX_SIZE;
        for icon in ICONS {
            let (spot, scale) = match icon {
                MapIcon::HorseHead | MapIcon::CarWreck => (RESOURCE_SPOT, 1.0),
                MapIcon::Ruins | MapIcon::FeralOrchard | MapIcon::RubblePit => {
                    (LANDMARK_SPOT, LANDMARK_SCALE)
                }
                _ => (IMPROVEMENT_SPOT, 1.0),
            };
            let mut out = Vec::new();
            push_map_icon_scaled(spot, icon, scale, &mut out);
            assert!(!out.is_empty(), "{icon:?} drew nothing");
            for pos in positions(&out) {
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
    fn yield_icons_fit_their_pips() {
        // `draw.rs` spaces pips as if an icon at 0.8 scale reached 0.05
        // across and 0.076 up; a little past that across is fine.
        let reach = Vec2::new(0.066, 0.095) * HEX_SIZE;
        for icon in [MapIcon::Food, MapIcon::Wood, MapIcon::Metal] {
            let mut out = Vec::new();
            push_map_icon(Vec2::ZERO, icon, &mut out);
            assert!(!out.is_empty(), "{icon:?} drew nothing");
            for pos in positions(&out) {
                assert!(
                    pos.x.abs() <= reach.x && pos.y.abs() <= reach.y,
                    "{icon:?} reaches {pos}"
                );
            }
        }
    }

    #[test]
    fn every_building_plate_stays_inside_the_old_badge() {
        // The letter badges were squares 0.44 across; a plate's edge may
        // reach a little past.
        for building in Building::ALL {
            let mut out = Vec::new();
            push_building(Vec2::ZERO, building, &mut out);
            assert!(!out.is_empty(), "{building:?} drew nothing");
            for pos in positions(&out) {
                assert!(
                    pos.x.abs() <= 0.25 * HEX_SIZE && pos.y.abs() <= 0.25 * HEX_SIZE,
                    "{building:?} reaches {pos}"
                );
            }
        }
    }

    #[test]
    fn map_marks_stay_inside_their_hex() {
        const TEAM: Color = [0.3, 0.55, 0.95, 1.0];
        type Mark = fn(&mut Vec<Vertex>);
        let marks: [(&str, Mark); 4] = [
            ("garrison", |out| push_garrison(Vec2::ZERO, TEAM, out)),
            ("enclave", |out| push_enclave(Vec2::ZERO, 1.0, TEAM, out)),
            ("watchfire", |out| push_watchfire(Vec2::ZERO, TEAM, out)),
            ("bunker", |out| push_bunker(Vec2::ZERO, TEAM, out)),
        ];
        for (name, draw) in marks {
            let mut out = Vec::new();
            draw(&mut out);
            assert!(!out.is_empty(), "the {name} drew nothing");
            for pos in positions(&out) {
                assert!(inside_hex(pos, 0.95 * HEX_SIZE), "the {name} reaches {pos}");
            }
        }
    }

    #[test]
    fn the_enclave_keeps_the_towers_size_for_the_turn_strip() {
        // `ui/paint.rs` sizes the turn strip's chip by the old tower: 0.84
        // wide and 0.8 tall, plus its outline.
        let mut out = Vec::new();
        push_enclave(Vec2::ZERO, 1.0, [1.0; 4], &mut out);
        for pos in positions(&out) {
            assert!(pos.x.abs() <= 0.48 && pos.y.abs() <= 0.46, "{pos}");
        }
    }

    #[test]
    fn a_gate_is_drawn_apart_from_a_wall_and_both_follow_their_edge() {
        let (start, end) = (Vec2::new(0.5, 0.0), Vec2::new(0.0, 0.866));
        let mut wall = Vec::new();
        push_barrier(start, end, 0.16, false, [0.3, 0.55, 0.95, 1.0], &mut wall);
        let mut gate = Vec::new();
        push_barrier(start, end, 0.16, true, [0.3, 0.55, 0.95, 1.0], &mut gate);
        assert!(gate.iter().any(|v| v.color == AMBER), "a gate has its bus");
        assert!(wall.iter().all(|v| v.color != AMBER), "a wall has none");
        // Nothing strays far from the edge's line.
        let along = (end - start).normalize();
        for pos in positions(&wall).chain(positions(&gate)) {
            let off = (pos - start).perp_dot(along).abs();
            assert!(off < 0.15, "{pos} is {off} off the edge");
        }
    }

    #[test]
    fn every_improvement_has_an_icon() {
        for label in ["FARM", "MINE", "PASTURE", "LUMBER MILL"] {
            assert!(MapIcon::improvement(label).is_some(), "{label}");
        }
        for label in ["HOMESTEAD", "SCRAP DIG", "CORRAL", "SAWPIT"] {
            assert!(MapIcon::improvement(label).is_some(), "{label}");
        }
    }
}
