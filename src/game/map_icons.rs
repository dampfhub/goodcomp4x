//! Map icons: small pictures of strategic resources and tile improvements,
//! drawn straight on the tile in the hex's top corners, and of food and
//! production in the yield rows. The world is an undersea one
//! (`docs/ocean-theme.md`): seahorses and pearls, kelp farms, coral quarries,
//! seahorse pens and sunken driftwood, fish and scallop shells. Each shape is
//! edged in dark so it reads on any sea floor. Shapes are laid out in the
//! coordinates of the mockups they were designed in: a hex of radius 100 with
//! Y pointing down, centered on the icon's spot.

use std::f32::consts::PI;

use glam::Vec2;

use super::hex::HEX_SIZE;
use super::mesh;
use super::terrain::Resource;
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

/// The edge around every shape, the same near-black as unit outlines.
const OUTLINE: Color = [0.03, 0.03, 0.04, 1.0];
// Colors are linear. Warm, saturated fills stand out from the sea floor's
// sand, seagrass, silt and slate; the dark edge does the rest.
const SEAHORSE: Color = [0.86, 0.36, 0.045, 1.0];
const SEAHORSE_RIDGE: Color = [0.30, 0.085, 0.01, 1.0];
const EYE: Color = [0.92, 0.88, 0.70, 1.0];
const OYSTER: Color = [0.16, 0.135, 0.115, 1.0];
const NACRE: Color = [0.60, 0.58, 0.74, 1.0];
const NACRE_SHADE: Color = [0.34, 0.32, 0.48, 1.0];
const PEARL: Color = [0.93, 0.91, 0.87, 1.0];
const SHINE: Color = [1.0, 1.0, 1.0, 1.0];
const KELP: Color = [0.36, 0.47, 0.035, 1.0];
const KELP_RIB: Color = [0.13, 0.17, 0.012, 1.0];
const KELP_STALK: Color = [0.19, 0.17, 0.02, 1.0];
const BLADDER: Color = [0.80, 0.55, 0.06, 1.0];
const CORAL: Color = [0.88, 0.22, 0.14, 1.0];
const CORAL_LIT: Color = [1.0, 0.50, 0.36, 1.0];
const CORAL_SHADE: Color = [0.42, 0.065, 0.045, 1.0];
const CORAL_PORE: Color = [0.24, 0.03, 0.02, 1.0];
const HANDLE: Color = [0.40, 0.19, 0.06, 1.0];
const STONE: Color = [0.55, 0.60, 0.66, 1.0];
const DRIFTWOOD: Color = [0.44, 0.38, 0.28, 1.0];
const DRIFTWOOD_END: Color = [0.74, 0.64, 0.45, 1.0];
const DRIFTWOOD_GRAIN: Color = [0.14, 0.11, 0.07, 1.0];
const BUBBLE: Color = [0.55, 0.88, 1.0, 1.0];
const FISH: Color = [1.0, 0.52, 0.05, 1.0];
const FISH_FIN: Color = [0.80, 0.26, 0.02, 1.0];
const SCALLOP: Color = [0.96, 0.76, 0.62, 1.0];
const SCALLOP_RIB: Color = [0.50, 0.24, 0.13, 1.0];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum MapIcon {
    /// Horses (seahorses): a seahorse's head, snout to the right.
    SeahorseHead,
    /// Iron (pearls): a pearl in an open oyster.
    Pearl,
    /// Farm (kelp farm): a kelp frond.
    Kelp,
    /// Mine (coral quarry): a cut block of coral with a pick.
    CoralQuarry,
    /// Pasture (seahorse pen): a ring of coral posts on a kelp rope.
    SeahorsePen,
    /// Lumber mill (driftwood): a sunken log, bubbles rising.
    Driftwood,
    /// Food, on a yield chip: a small fish.
    Food,
    /// Production, on a yield chip: a scallop shell.
    Production,
}

impl MapIcon {
    pub(super) fn resource(resource: Resource) -> Self {
        match resource {
            Resource::Horses => Self::SeahorseHead,
            Resource::Iron => Self::Pearl,
        }
    }

    /// An improvement's icon, by the label it's built with: the original
    /// label or its ocean-theme name, so either spelling draws the icon.
    pub(super) fn improvement(label: &str) -> Option<Self> {
        match label {
            "FARM" | "KELP FARM" => Some(Self::Kelp),
            "MINE" | "CORAL QUARRY" => Some(Self::CoralQuarry),
            "PASTURE" | "SEAHORSE PEN" => Some(Self::SeahorsePen),
            "LUMBER MILL" | "DRIFTWOOD" => Some(Self::Driftwood),
            _ => None,
        }
    }
}

/// Draws `icon` centered on `center`.
pub(super) fn push_map_icon(center: Vec2, icon: MapIcon, out: &mut Vec<Vertex>) {
    push_map_icon_scaled(center, icon, 1.0, out);
}

/// Draws `icon` centered on `center`, `scale` times its usual size.
pub(super) fn push_map_icon_scaled(center: Vec2, icon: MapIcon, scale: f32, out: &mut Vec<Vertex>) {
    let mut pen = Pen {
        center,
        scale: scale * HEX_SIZE / DESIGN_HEX_RADIUS,
        out,
    };
    match icon {
        MapIcon::SeahorseHead => seahorse_head(&mut pen),
        MapIcon::Pearl => pearl(&mut pen),
        MapIcon::Kelp => kelp(&mut pen),
        MapIcon::CoralQuarry => coral_quarry(&mut pen),
        MapIcon::SeahorsePen => seahorse_pen(&mut pen),
        MapIcon::Driftwood => driftwood(&mut pen),
        MapIcon::Food => fish(&mut pen),
        MapIcon::Production => scallop(&mut pen),
    }
}

/// A seahorse's head and neck in profile: a crown of spikes, a long tube
/// of a snout, and ridges ringing the neck.
fn seahorse_head(pen: &mut Pen) {
    pen.shape(
        &[
            (-7.0, 16.0),
            (-9.5, 10.0),
            (-10.0, 4.0),
            (-9.0, -2.0),
            (-7.0, -7.0),
            // The coronet.
            (-9.0, -12.5),
            (-4.5, -10.0),
            (-3.5, -16.0),
            (-0.5, -11.0),
            (2.5, -14.0),
            (3.5, -8.5),
            // The snout, flaring at its tip.
            (6.5, -6.5),
            (14.5, -5.5),
            (17.0, -7.5),
            (18.0, -3.0),
            (16.0, -1.5),
            (8.0, -1.5),
            (4.5, 2.0),
            // Throat, then the chest bulging forward.
            (1.0, 4.0),
            (0.5, 8.0),
            (3.5, 12.0),
            (3.0, 16.0),
        ],
        SEAHORSE,
        3.0,
    );
    for (back, front) in [
        ((-8.6, 5.5), (-1.0, 6.3)),
        ((-8.6, 10.0), (0.4, 10.4)),
        ((-7.2, 14.0), (1.4, 14.3)),
    ] {
        pen.line(&[back, front], 1.3, SEAHORSE_RIDGE);
    }
    pen.circle((0.5, -5.0), 2.3, EYE, 1.5);
    pen.circle((1.0, -5.0), 1.1, OUTLINE, 0.0);
}

/// A pearl in an open oyster: a rough lower shell, its lid tipped back
/// behind, both lined with nacre.
fn pearl(pen: &mut Pen) {
    // The lid, open and showing its inside.
    pen.shape(&blob((0.0, -7.0), 15.0, 8.5, 0.07, 5), OYSTER, 3.0);
    pen.shape(&ellipse((0.0, -6.5), 12.0, 6.0, 0.0), NACRE, 0.0);
    pen.shape(&ellipse((0.0, -4.5), 9.0, 3.0, 0.0), NACRE_SHADE, 0.0);
    // The cup: the far half of its rim, then its rough underside.
    let mut cup: Vec<(f32, f32)> = (0..=8)
        .map(|i| {
            let angle = PI + PI * i as f32 / 8.0;
            (16.0 * angle.cos(), 1.0 + 4.0 * angle.sin())
        })
        .collect();
    cup.extend([
        (15.0, 5.0),
        (11.0, 8.5),
        (6.0, 10.0),
        (0.0, 11.5),
        (-6.0, 10.5),
        (-11.0, 9.0),
        (-15.0, 5.0),
    ]);
    pen.shape(&cup, OYSTER, 3.0);
    pen.shape(&ellipse((0.0, 1.5), 13.0, 3.2, 0.0), NACRE, 0.0);
    // The pearl sits in the cup.
    pen.circle((0.0, -2.5), 5.5, PEARL, 2.5);
    pen.circle((-1.8, -4.3), 1.7, SHINE, 0.0);
}

/// A kelp frond: a wavy stalk with long ruffled blades streaming up off
/// alternate sides, a gas bladder at each blade's base.
fn kelp(pen: &mut Pen) {
    let stalk = [
        (0.0, 16.0),
        (-1.0, 10.0),
        (0.5, 4.0),
        (-0.5, -2.0),
        (0.5, -8.0),
        (0.0, -12.0),
    ];
    // Blades as (base, tip, half width, bend), bottom up so upper ones
    // overlap lower ones.
    let blades = [
        ((-1.0, 10.0), (-13.0, -4.0), 2.6, -3.0),
        ((0.5, 4.0), (12.5, -10.0), 2.6, 3.0),
        ((-0.5, -2.0), (-8.5, -17.0), 2.4, -2.5),
        ((0.5, -8.0), (5.0, -20.0), 2.2, 2.0),
    ];
    pen.line(&stalk, 5.5, OUTLINE);
    pen.line(&stalk, 2.5, KELP_STALK);
    for (base, tip, half_width, bend) in blades {
        pen.shape(&leaf(base, tip, half_width, bend), KELP, 2.5);
        let rib: Vec<_> = (1..=12)
            .map(|i| leaf_point(base, tip, bend, i as f32 / 16.0, 0.0))
            .collect();
        pen.line(&rib, 0.9, KELP_RIB);
    }
    for (base, _, _, _) in blades {
        pen.circle(base, 1.4, BLADDER, 1.4);
    }
}

/// A coral quarry: a block of coral cut square, pores showing and living
/// coral still branching from its top, with a pick leaning behind it.
fn coral_quarry(pen: &mut Pen) {
    // Staghorn coral still growing from the block's far corner.
    let branches: [&[(f32, f32)]; 3] = [
        &[(-9.0, -3.0), (-9.5, -9.0), (-12.5, -14.5)],
        &[(-9.3, -7.5), (-5.0, -12.5), (-5.5, -16.0)],
        &[(-10.2, -10.8), (-15.5, -11.5)],
    ];
    for branch in branches {
        pen.line(branch, 5.5, OUTLINE);
    }
    for branch in branches {
        pen.line(branch, 2.6, CORAL_LIT);
    }
    // The pick leans on the block's far side, which then hides its handle.
    let at = |(x, y): (f32, f32)| (x + 9.0, y - 12.0);
    let handle: Vec<_> = turned(&[(0.0, 0.0), (0.0, 15.0)], 45.0)
        .into_iter()
        .map(at)
        .collect();
    let head = [
        (-10.0, 4.5),
        (-5.0, -1.2),
        (0.0, -2.5),
        (5.0, -1.2),
        (10.0, 4.5),
        (5.0, 1.0),
        (0.0, 0.5),
        (-5.0, 1.0),
    ];
    let head: Vec<_> = turned(&head, 45.0).into_iter().map(at).collect();
    pen.line(&handle, 5.5, OUTLINE);
    pen.line(&handle, 2.8, HANDLE);
    pen.shape(&head, STONE, 2.5);
    // The block, lit from above and to the left.
    let front = [(-12.0, -1.0), (4.0, -1.0), (4.0, 12.0), (-12.0, 12.0)];
    let top = [(-12.0, -1.0), (-8.0, -6.0), (8.0, -6.0), (4.0, -1.0)];
    let side = [(4.0, -1.0), (8.0, -6.0), (8.0, 7.0), (4.0, 12.0)];
    pen.shape(&side, CORAL_SHADE, 3.0);
    pen.shape(&front, CORAL, 3.0);
    pen.shape(&top, CORAL_LIT, 3.0);
    for (x, y) in [
        (-9.0, 3.0),
        (-4.0, 7.5),
        (0.5, 2.5),
        (-8.0, 9.0),
        (1.0, 9.0),
        (-4.5, 2.0),
    ] {
        pen.shape(&ellipse((x, y), 1.1, 1.1, 0.0), CORAL_PORE, 0.0);
    }
    for (x, y) in [(-5.0, -3.5), (1.0, -4.0)] {
        pen.shape(&ellipse((x, y), 1.2, 0.7, 0.0), CORAL_SHADE, 0.0);
    }
}

/// A seahorse pen: a ring of coral posts seen from above and in front,
/// strung on a kelp rope. Drawn back to front.
fn seahorse_pen(pen: &mut Pen) {
    let (center, rx, ry) = ((0.0, 5.0), 13.5, 6.0);
    let height = |degrees: f32| if degrees > 180.0 { 9.0 } else { 11.0 };
    let base = |degrees: f32| {
        let angle = degrees.to_radians();
        (center.0 + rx * angle.cos(), center.1 + ry * angle.sin())
    };
    let rope = |from: f32, to: f32| -> Vec<(f32, f32)> {
        (0..=12)
            .map(|i| {
                let degrees = from + (to - from) * i as f32 / 12.0;
                let (x, y) = base(degrees);
                (x, y - 0.55 * height(degrees))
            })
            .collect()
    };
    let post = |pen: &mut Pen, degrees: f32| {
        let (x, y) = base(degrees);
        let top = (x, y - height(degrees));
        pen.line(&[(x, y), top], 7.0, OUTLINE);
        pen.line(&[(x, y), top], 4.0, CORAL);
        pen.circle(top, 2.6, CORAL_LIT, 1.5);
    };
    for degrees in [240.0, 300.0] {
        post(pen, degrees);
    }
    let back = rope(180.0, 360.0);
    pen.line(&back, 4.5, OUTLINE);
    pen.line(&back, 2.0, KELP);
    for degrees in [0.0, 180.0] {
        post(pen, degrees);
    }
    let front = rope(0.0, 180.0);
    pen.line(&front, 4.5, OUTLINE);
    pen.line(&front, 2.0, KELP);
    for degrees in [60.0, 120.0] {
        post(pen, degrees);
    }
}

/// Sunken driftwood: a bleached log lying aslant, one end sawn and the
/// other broken off, a snag of branch pointing up and bubbles rising.
fn driftwood(pen: &mut Pen) {
    // Along the log and across it (down-right), from its sawn end.
    let start = (-13.0, 7.0);
    let along = Vec2::new(25.0, -10.0).normalize();
    let across = along.perp();
    let at = |u: f32, v: f32| {
        let p = Vec2::from(start) + along * u + across * v;
        (p.x, p.y)
    };
    let branch = [at(15.0, -3.0), at(18.0, -10.0)];
    let twig = [at(17.0, -7.5), at(21.5, -9.5)];
    pen.line(&branch, 5.5, OUTLINE);
    pen.line(&twig, 4.5, OUTLINE);
    pen.line(&branch, 2.5, DRIFTWOOD);
    pen.line(&twig, 1.8, DRIFTWOOD);
    let log = [
        at(0.0, -5.0),
        at(9.0, -5.3),
        at(18.0, -4.5),
        at(25.0, -4.2),
        at(28.5, -2.5),
        at(26.0, -0.5),
        at(29.0, 1.5),
        at(25.5, 4.0),
        at(15.0, 4.5),
        at(6.0, 5.2),
        at(0.0, 5.0),
    ];
    pen.shape(&log, DRIFTWOOD, 3.0);
    for (v, from, to) in [(-2.0, 5.0, 22.0), (1.5, 8.0, 24.0)] {
        let grain: Vec<_> = (0..=6)
            .map(|i| {
                let u = from + (to - from) * i as f32 / 6.0;
                at(u, v + 0.5 * (u * 0.8).sin())
            })
            .collect();
        pen.line(&grain, 1.0, DRIFTWOOD_GRAIN);
    }
    let tilt = along.to_angle().to_degrees();
    pen.shape(&ellipse(start, 2.6, 5.0, tilt), DRIFTWOOD_END, 2.5);
    pen.ring(&ellipse(start, 1.1, 2.3, tilt), 1.0, DRIFTWOOD_GRAIN);
    for ((x, y), radius) in [((6.0, -10.0), 1.9), ((9.5, -15.0), 1.3)] {
        pen.ring(&ellipse((x, y), radius, radius, 0.0), 2.4, OUTLINE);
        pen.ring(&ellipse((x, y), radius, radius, 0.0), 1.0, BUBBLE);
    }
}

/// Food: a small fish swimming right.
fn fish(pen: &mut Pen) {
    pen.shape(
        &[(-1.5, 0.0), (-5.6, -3.4), (-4.4, 0.0), (-5.6, 3.4)],
        FISH_FIN,
        1.6,
    );
    pen.shape(&[(-0.8, -2.2), (1.2, -4.4), (3.2, -2.2)], FISH_FIN, 1.6);
    pen.shape(&ellipse((1.2, 0.0), 4.4, 3.0, 0.0), FISH, 1.6);
    pen.shape(&ellipse((3.3, -0.6), 0.8, 0.8, 0.0), OUTLINE, 0.0);
}

/// Production: a scallop shell, a ribbed fan above its two ears.
fn scallop(pen: &mut Pen) {
    let hinge = (0.0, 2.5);
    let ribs = 6;
    let rib_angle = |i: usize| (210.0 + 120.0 * i as f32 / ribs as f32).to_radians();
    let mut shell: Vec<(f32, f32)> = Vec::new();
    for i in 0..=ribs {
        let angle = rib_angle(i);
        shell.push((hinge.0 + 5.8 * angle.cos(), hinge.1 + 7.4 * angle.sin()));
        if i < ribs {
            let between = (rib_angle(i) + rib_angle(i + 1)) / 2.0;
            shell.push((hinge.0 + 5.1 * between.cos(), hinge.1 + 6.6 * between.sin()));
        }
    }
    shell.extend([
        (1.8, 2.5),
        (4.0, 3.0),
        (4.0, 5.3),
        (-4.0, 5.3),
        (-4.0, 3.0),
        (-1.8, 2.5),
    ]);
    pen.shape(&shell, SCALLOP, 1.6);
    for i in 1..ribs {
        let angle = rib_angle(i);
        let end = (hinge.0 + 5.0 * angle.cos(), hinge.1 + 6.0 * angle.sin());
        pen.line(&[(hinge.0, hinge.1 - 0.5), end], 0.7, SCALLOP_RIB);
    }
}

/// `points` turned by `degrees` about the icon's center (clockwise on
/// screen, as SVG's `rotate` with Y down).
fn turned(points: &[(f32, f32)], degrees: f32) -> Vec<(f32, f32)> {
    let turn = Vec2::from_angle(degrees.to_radians());
    points
        .iter()
        .map(|&(x, y)| {
            let p = turn.rotate(Vec2::new(x, y));
            (p.x, p.y)
        })
        .collect()
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

/// An ellipse with a wavy edge, `lobes` bumps each `wobble` of its radius
/// out and in: a rough shell's lip.
fn blob(center: (f32, f32), rx: f32, ry: f32, wobble: f32, lobes: u32) -> Vec<(f32, f32)> {
    let sides = 2 * ROUND_SIDES;
    (0..sides)
        .map(|i| {
            let angle = std::f32::consts::TAU * i as f32 / sides as f32;
            let r = 1.0 + wobble * (lobes as f32 * angle).sin();
            (
                center.0 + r * rx * angle.cos(),
                center.1 + r * ry * angle.sin(),
            )
        })
        .collect()
}

/// A pointed blade from `base` to `tip`, `half_width` at its widest, bowed
/// `bend` to one side, its edges ruffled like a kelp blade's.
fn leaf(base: (f32, f32), tip: (f32, f32), half_width: f32, bend: f32) -> Vec<(f32, f32)> {
    const STEPS: usize = 16;
    let point = |i: usize, side: f32| {
        let t = i as f32 / STEPS as f32;
        let ruffle = 1.0 + 0.3 * (6.0 * PI * t).sin() * side;
        leaf_point(base, tip, bend, t, side * half_width * ruffle)
    };
    let one_side = (0..=STEPS).map(|i| point(i, 1.0));
    let other_side = (1..STEPS).rev().map(|i| point(i, -1.0));
    one_side.chain(other_side).collect()
}

/// The point `t` of the way along a `leaf` from `base` to `tip`, `offset`
/// across it at its widest.
fn leaf_point(base: (f32, f32), tip: (f32, f32), bend: f32, t: f32, offset: f32) -> (f32, f32) {
    let (base, tip) = (Vec2::from(base), Vec2::from(tip));
    let side = (tip - base).perp().normalize();
    let bulge = (PI * t).sin().max(0.0);
    // Bowed, with a slight S so the blade seems to sway.
    let sway = bend * (bulge - 0.4 * (2.0 * PI * t).sin());
    let p = base.lerp(tip, t) + side * (sway + offset * bulge.powf(0.7));
    (p.x, p.y)
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

    /// A filled circle, edged like `shape`.
    fn circle(&mut self, center: (f32, f32), radius: f32, fill: Color, edge: f32) {
        self.shape(&ellipse(center, radius, radius, 0.0), fill, edge);
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

    const ICONS: [MapIcon; 6] = [
        MapIcon::SeahorseHead,
        MapIcon::Pearl,
        MapIcon::Kelp,
        MapIcon::CoralQuarry,
        MapIcon::SeahorsePen,
        MapIcon::Driftwood,
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
            let spot = match icon {
                MapIcon::SeahorseHead | MapIcon::Pearl => RESOURCE_SPOT,
                _ => IMPROVEMENT_SPOT,
            };
            let mut out = Vec::new();
            push_map_icon(spot, icon, &mut out);
            assert!(!out.is_empty(), "{icon:?} drew nothing");
            for vertex in out {
                let pos = Vec2::new(vertex.pos[0], vertex.pos[1]);
                assert!(
                    inside_hex(pos, HEX_SIZE),
                    "{icon:?} leaves its hex at {pos}"
                );
                assert!(pos.length() > token, "{icon:?} reaches the unit at {pos}");
                assert!(pos.distance(spot) < 0.25, "{icon:?} sprawls to {pos}");
            }
        }
    }

    #[test]
    fn yield_icons_fit_their_pips() {
        // `draw.rs` spaces pips as if an icon at 0.8 scale reached 0.05
        // across and 0.076 up; a little past that across is fine.
        let reach = Vec2::new(0.066, 0.095) * HEX_SIZE;
        for icon in [MapIcon::Food, MapIcon::Production] {
            let mut out = Vec::new();
            push_map_icon(Vec2::ZERO, icon, &mut out);
            assert!(!out.is_empty(), "{icon:?} drew nothing");
            for vertex in out {
                let pos = Vec2::new(vertex.pos[0], vertex.pos[1]);
                assert!(
                    pos.x.abs() <= reach.x && pos.y.abs() <= reach.y,
                    "{icon:?} reaches {pos}"
                );
            }
        }
    }

    #[test]
    fn every_improvement_has_an_icon() {
        for label in ["FARM", "MINE", "PASTURE", "LUMBER MILL"] {
            assert!(MapIcon::improvement(label).is_some(), "{label}");
        }
        for label in ["KELP FARM", "CORAL QUARRY", "SEAHORSE PEN", "DRIFTWOOD"] {
            assert!(MapIcon::improvement(label).is_some(), "{label}");
        }
    }
}
