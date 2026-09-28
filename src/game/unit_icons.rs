//! Unit pictograms: a small picture of what each unit is in the salvage age
//! (see `docs/apocalypse-theme.md`), a bold silhouette built from polygons,
//! circles, rings and lines and drawn dark on the unit's token. Details are
//! gaps left between shapes, so the token shows through them. Shapes are laid
//! out on a token of radius 42 with Y pointing down, the coordinates of the
//! mockups they were designed in.

use std::f32::consts::TAU;

use glam::Vec2;

use super::mesh;
use super::unit::UnitType;
use crate::renderer::Vertex;

type Color = [f32; 4];

/// Token radius the shapes are laid out on.
const DESIGN_RADIUS: f32 = 42.0;
/// Sides of the polygons that stand in for circles.
const CIRCLE_SIDES: u32 = 16;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum UnitIcon {
    /// Melee (Scrapper): a fighter's head over a stop-sign shield, a rebar
    /// spear held upright beside it.
    Scrapper,
    /// Ranged (Bowman): a salvaged compound bow, drawn, aimed up and to the
    /// right.
    Bowman,
    /// Cavalry (Outrider): a rider on a galloping horse, a scarf flying.
    Outrider,
    /// Siege (Trebuchet): a trebuchet of crane lattice with a car tyre in
    /// its sling.
    Trebuchet,
    /// Scout (Cyclist): a bicycle.
    Cyclist,
    /// Armored (Riot Guard): a riot helmet with its visor down, behind a riot
    /// shield.
    RiotGuard,
    /// Patrol galley (Skiff): a rowing boat seen from above, oars out.
    Skiff,
    /// Landing craft (Barge): a flat river barge with crates and a shack.
    Barge,
    /// Bombard ship (Rust Hulk): a rusted, broken hull with a catapult on
    /// its deck.
    RustHulk,
    /// Settler (Caravan): a handcart piled with bundles, flying a flag.
    /// (Named for the flag it was before the theme; `mod.rs` uses it.)
    Flag,
    /// Worker (Salvager): a hard hat with a crowbar behind it. (Named for
    /// the shovel it was before the theme; `draw.rs` uses it.)
    Shovel,
}

impl UnitIcon {
    pub(super) fn of(unit_type: UnitType) -> Self {
        match unit_type {
            UnitType::Melee => Self::Scrapper,
            UnitType::Ranged => Self::Bowman,
            UnitType::Cavalry => Self::Outrider,
            UnitType::Siege => Self::Trebuchet,
            UnitType::Scout => Self::Cyclist,
            UnitType::Armored => Self::RiotGuard,
            UnitType::PatrolGalley => Self::Skiff,
            UnitType::LandingCraft => Self::Barge,
            UnitType::BombardShip => Self::RustHulk,
        }
    }
}

/// Draws `icon` in `color` centered on a token of `radius` at `center`.
pub(super) fn push_pictogram(
    center: Vec2,
    radius: f32,
    icon: UnitIcon,
    color: Color,
    out: &mut Vec<Vertex>,
) {
    let mut pen = Pen {
        center,
        scale: radius / DESIGN_RADIUS,
        turn: Vec2::X,
        color,
        out,
    };
    match icon {
        UnitIcon::Scrapper => scrapper(&mut pen),
        UnitIcon::Bowman => bowman(&mut pen),
        UnitIcon::Outrider => outrider(&mut pen),
        UnitIcon::Trebuchet => trebuchet(&mut pen),
        UnitIcon::Cyclist => cyclist(&mut pen),
        UnitIcon::RiotGuard => riot_guard(&mut pen),
        UnitIcon::Skiff => skiff(&mut pen),
        UnitIcon::Barge => barge(&mut pen),
        UnitIcon::RustHulk => rust_hulk(&mut pen),
        UnitIcon::Flag => caravan(&mut pen),
        UnitIcon::Shovel => salvager(&mut pen),
    }
}

/// A head and shoulders over an octagonal stop-sign shield, its border
/// parted from its face by a gap, and a sharpened rebar spear.
fn scrapper(pen: &mut Pen) {
    let shield = (-5.0, 7.0);
    pen.outline(&octagon(shield, 17.2), 3.6);
    pen.polygon(&octagon(shield, 12.8));
    // Head and shoulders, the shoulders hidden behind the shield's rim.
    pen.circle(-5.0, -20.5, 6.5);
    pen.polygon(&[(-15.0, -9.5), (-12.0, -14.0), (2.0, -14.0), (5.0, -9.5)]);
    // The spear, beside the shield, and the fist holding it.
    pen.line((20.0, 26.0), (13.0, -22.0), 4.0);
    pen.polygon(&[(8.5, -21.0), (12.0, -32.0), (17.5, -22.5)]);
    pen.circle(16.5, 1.0, 4.5);
}

/// A compound bow at full draw: a short riser, stiff limbs with a cam at
/// each tip, the string pulled back to the arrow's nock.
fn bowman(pen: &mut Pen) {
    pen.turn(-45.0);
    // String and cables, from each cam back to the nock.
    pen.line((1.0, -24.0), (-22.0, 0.0), 2.6);
    pen.line((1.0, 24.0), (-22.0, 0.0), 2.6);
    // Riser, limbs and cams.
    pen.curve(&[(9.0, -13.0), (13.0, -5.0), (13.0, 5.0), (9.0, 13.0)], 7.0);
    pen.line((9.0, -13.0), (1.0, -24.0), 5.5);
    pen.line((9.0, 13.0), (1.0, 24.0), 5.5);
    pen.circle(1.0, -24.0, 5.0);
    pen.circle(1.0, 24.0, 5.0);
    // The arrow: shaft, head and fletching.
    pen.line((-25.0, 0.0), (21.0, 0.0), 3.6);
    pen.polygon(&[(19.0, -6.5), (31.0, 0.0), (19.0, 6.5)]);
    pen.polygon(&[(-18.0, 0.0), (-24.0, -6.0), (-29.0, -6.0), (-23.0, 0.0)]);
    pen.polygon(&[(-18.0, 0.0), (-23.0, 0.0), (-29.0, 6.0), (-24.0, 6.0)]);
}

/// A horse at full gallop, facing right, with a rider leaning into the
/// run and a scarf streaming behind.
fn outrider(pen: &mut Pen) {
    // The horse: body, neck and head, legs reaching out, tail.
    pen.ellipse((-4.0, 3.0), 17.0, 8.0);
    pen.polygon(&[
        (3.0, -3.0),
        (11.0, -14.0),
        (14.0, -22.0),
        (17.0, -19.0),
        (21.0, -18.0),
        (28.0, -11.0),
        (26.0, -6.5),
        (20.0, -9.0),
        (15.5, -5.0),
        (13.0, 6.0),
    ]);
    pen.curve(&[(9.0, 6.0), (17.0, 11.0), (25.0, 9.0)], 4.5);
    pen.curve(&[(7.0, 8.0), (12.0, 15.0), (10.0, 23.0)], 4.5);
    pen.curve(&[(-15.0, 6.0), (-20.0, 14.0), (-27.0, 18.0)], 4.5);
    pen.curve(&[(-11.0, 8.0), (-9.0, 16.0), (-13.0, 23.0)], 4.5);
    pen.curve(&[(-19.0, 0.0), (-26.0, -1.0), (-30.0, 5.0)], 4.0);
    // The rider: body leaning forward, an arm to the reins, head.
    pen.polygon(&[
        (-8.0, -3.0),
        (1.0, -3.0),
        (3.0, -12.0),
        (1.0, -19.0),
        (-5.0, -19.5),
        (-8.0, -12.0),
    ]);
    pen.curve(&[(-1.0, -15.0), (6.0, -11.0), (11.0, -13.0)], 3.0);
    pen.circle(0.0, -24.5, 4.8);
    // The scarf, knotted at the neck and streaming back.
    pen.polygon(&[
        (-2.0, -21.0),
        (-10.0, -25.0),
        (-17.0, -22.0),
        (-25.0, -24.5),
        (-22.0, -18.5),
        (-15.0, -17.0),
        (-8.0, -18.0),
        (-2.0, -17.0),
    ]);
}

/// A trebuchet built from crane parts: a lattice A-frame on a beam, the
/// throwing arm with an engine-block counterweight, and a car tyre hanging
/// in the sling.
fn trebuchet(pen: &mut Pen) {
    let pivot = (5.0, -7.0);
    pen.rect(-24.0, 17.0, 48.0, 5.0);
    // The A-frame's legs and the lattice bracing between them.
    pen.line((-8.0, 18.0), pivot, 4.8);
    pen.line((18.0, 18.0), pivot, 4.8);
    pen.line((-4.0, 10.0), (14.0, 10.0), 2.6);
    pen.line((-4.0, 10.0), (11.0, 2.0), 2.4);
    pen.line((14.0, 10.0), (-1.0, 2.0), 2.4);
    // The arm, long end up and to the left, and its counterweight.
    pen.line((-23.0, -22.0), (15.0, 0.0), 5.5);
    pen.rect(9.0, 1.0, 13.0, 10.0);
    pen.circle(pivot.0, pivot.1, 3.5);
    // The sling and the tyre in it.
    pen.line((-23.0, -22.0), (-21.0, -10.0), 2.4);
    pen.ring((-20.0, -2.0), 6.5, 5.5);
}

/// A bicycle side on: two wheels, a diamond frame, saddle, handlebars and
/// chainring.
fn cyclist(pen: &mut Pen) {
    let (rear, front) = ((-16.0, 8.0), (16.0, 8.0));
    let (bracket, seat, head) = ((-1.0, 8.0), (-6.0, -8.0), (10.0, -8.0));
    pen.ring(rear, 11.0, 3.5);
    pen.ring(front, 11.0, 3.5);
    pen.circle(rear.0, rear.1, 2.2);
    pen.circle(front.0, front.1, 2.2);
    for (from, to) in [
        (rear, bracket),
        (rear, seat),
        (seat, bracket),
        (seat, head),
        (head, bracket),
        (head, front),
    ] {
        pen.line(from, to, 3.0);
    }
    pen.circle(bracket.0, bracket.1, 4.0);
    // Seat post and saddle; stem and handlebars.
    pen.line(seat, (-7.0, -12.0), 3.0);
    pen.polygon(&[(-13.0, -14.0), (-3.0, -14.5), (-3.0, -11.5), (-12.0, -11.0)]);
    pen.line(head, (8.0, -15.0), 3.0);
    pen.curve(&[(4.0, -15.5), (9.0, -15.5), (13.0, -13.0)], 3.0);
}

/// A riot helmet peering over a riot shield twice its width: the helmet's
/// dome with the visor pulled down under it, a gap between them, and the
/// shield's viewing window a hole the token shows through.
fn riot_guard(pen: &mut Pen) {
    pen.polygon(&dome((0.0, -15.0), 10.5, 11.5, -10.5, 10.5));
    pen.polygon(&[(-11.5, -13.0), (11.5, -13.0), (11.0, -5.5), (-11.0, -5.5)]);
    // The shield, its top arched, built around its window.
    let (left, right, bottom) = (-21.0, 21.0, 27.0);
    let (window_top, window_bottom, window_side) = (2.0, 8.0, 13.0);
    let mut upper = vec![(left, window_top), (right, window_top), (right, -1.0)];
    upper.extend(quadratic((right, -1.0), (0.0, -6.0), (left, -1.0)));
    pen.polygon(&upper);
    let height = window_bottom - window_top;
    pen.rect(left, window_top, left.abs() - window_side, height);
    pen.rect(window_side, window_top, right - window_side, height);
    let mut lower = vec![(right, window_bottom)];
    lower.extend(quadratic(
        (right, window_bottom),
        (right, bottom),
        (right - 7.0, bottom),
    ));
    lower.push((left + 7.0, bottom));
    lower.extend(quadratic(
        (left + 7.0, bottom),
        (left, bottom),
        (left, window_bottom),
    ));
    pen.polygon(&lower);
}

/// A rowing boat from above, bow up: the hull's gunwale, two thwarts and a
/// pair of oars out to the sides.
fn skiff(pen: &mut Pen) {
    let mut hull = vec![(0.0, -28.0)];
    hull.extend(quadratic((0.0, -28.0), (12.0, -18.0), (11.0, 2.0)));
    hull.extend(quadratic((11.0, 2.0), (10.0, 14.0), (7.0, 23.0)));
    hull.extend(quadratic((7.0, 23.0), (0.0, 24.0), (-7.0, 23.0)));
    hull.extend(quadratic((-7.0, 23.0), (-10.0, 14.0), (-11.0, 2.0)));
    hull.extend(quadratic((-11.0, 2.0), (-12.0, -18.0), (0.0, -28.0)));
    hull.pop();
    pen.outline(&hull, 5.5);
    pen.line((-10.0, -6.0), (10.0, -6.0), 4.2);
    pen.line((-10.0, 10.0), (10.0, 10.0), 4.2);
    for side in [-1.0f32, 1.0] {
        pen.line((6.0 * side, -6.0), (24.0 * side, 8.0), 3.2);
        pen.polygon(&[
            (21.0 * side, 4.0),
            (29.0 * side, 10.0),
            (27.0 * side, 14.0),
            (19.0 * side, 9.0),
        ]);
    }
    pen.circle(0.0, 2.0, 4.5);
}

/// A flat river barge side on: a long, low hull with raked ends, crates on
/// deck, a plank shack with a stovepipe at the stern, and the waterline.
fn barge(pen: &mut Pen) {
    pen.polygon(&[(-31.0, 3.0), (31.0, 3.0), (25.0, 14.0), (-25.0, 14.0)]);
    pen.rect(-24.0, -8.0, 11.0, 9.0);
    pen.rect(-11.0, -4.0, 9.0, 5.0);
    pen.rect(-22.0, -17.0, 7.0, 7.0);
    // The shack, its roof overhanging, and its stovepipe.
    pen.rect(6.0, -10.0, 15.0, 11.0);
    pen.polygon(&[(3.0, -10.0), (24.0, -10.0), (22.0, -14.0), (5.0, -14.0)]);
    pen.rect(16.0, -21.0, 3.0, 8.0);
    pen.line((-22.0, 20.0), (22.0, 20.0), 2.8);
}

/// A rusted-out steamer hull with a torn gunwale, its plates parted by a
/// gap, and a snapped funnel; a catapult on the foredeck with a stone in
/// its cup.
fn rust_hulk(pen: &mut Pen) {
    pen.polygon(&[
        (-29.0, 5.0),
        (-20.0, 5.0),
        (-17.0, 1.5),
        (-15.0, 6.0),
        (-11.0, 2.5),
        (-9.0, 5.0),
        (24.0, 5.0),
        (30.0, -1.0),
        (26.5, 10.0),
        (-27.0, 10.0),
    ]);
    pen.polygon(&[(-26.5, 12.5), (25.5, 12.5), (21.0, 22.0), (-23.0, 22.0)]);
    // The snapped funnel, jagged at the top.
    pen.polygon(&[
        (-26.0, 5.0),
        (-26.0, -9.0),
        (-24.0, -12.0),
        (-22.0, -8.0),
        (-20.0, -11.0),
        (-19.0, 5.0),
    ]);
    // The catapult: frame, arm and the stone in its cup.
    pen.polygon(&[(-2.0, 5.0), (3.0, -7.0), (8.0, 5.0)]);
    pen.line((-6.0, 3.0), (17.0, -18.0), 5.0);
    pen.circle(18.5, -20.5, 6.0);
}

/// A handcart facing left, where it's pulled from: the bed on one big
/// wheel, bundles piled on it and a flag flying back from a pole.
fn caravan(pen: &mut Pen) {
    pen.polygon(&[(-15.0, 3.0), (16.0, 3.0), (14.0, 11.0), (-12.0, 11.0)]);
    pen.line((-14.0, 5.0), (-29.0, 11.0), 3.0);
    pen.ring((3.0, 17.0), 7.0, 3.0);
    pen.circle(3.0, 17.0, 2.2);
    // The pole, then the bundles piled up on the bed.
    pen.line((10.0, 3.0), (10.0, -27.0), 3.0);
    pen.polygon(&[(11.0, -28.0), (24.0, -23.5), (11.0, -18.0)]);
    pen.ellipse((-7.0, -3.5), 8.0, 5.0);
    pen.ellipse((7.0, -3.5), 7.5, 5.0);
    // A sack on top, its neck tied off in a tuft.
    pen.ellipse((-2.0, -12.0), 7.0, 5.5);
    pen.polygon(&[(-4.0, -16.5), (-7.0, -21.0), (3.0, -21.0), (0.0, -16.5)]);
}

/// A hard hat with a raised ridge and a wide brim, a crowbar behind it,
/// its claw end up and to the right.
fn salvager(pen: &mut Pen) {
    // The crowbar: a straight bar, a heel at the bottom, a curled claw.
    pen.line((-18.0, 24.0), (13.0, -17.0), 4.5);
    pen.curve(&[(-18.0, 24.0), (-22.0, 24.5), (-24.0, 21.0)], 4.0);
    pen.curve(
        &[(13.0, -17.0), (16.5, -22.5), (21.5, -23.5), (24.0, -19.5)],
        4.5,
    );
    // The hat: shell in three panels, the middle one raised, and brim.
    let (center, rx, ry) = ((0.0, 8.0), 17.0, 17.0);
    pen.polygon(&dome(center, rx, ry, -17.0, -5.0));
    pen.polygon(&dome(center, rx, ry + 3.0, -3.0, 3.0));
    pen.polygon(&dome(center, rx, ry, 5.0, 17.0));
    pen.polygon(&[(-25.0, 8.0), (25.0, 8.0), (22.0, 13.0), (-22.0, 13.0)]);
}

/// A flat-topped octagon, like a stop sign, around `center` with its corners
/// on a circle of `radius`.
fn octagon(center: (f32, f32), radius: f32) -> Vec<(f32, f32)> {
    (0..8)
        .map(|i| {
            let angle = TAU * (i as f32 + 0.5) / 8.0;
            (
                center.0 + radius * angle.cos(),
                center.1 + radius * angle.sin(),
            )
        })
        .collect()
}

/// The slice from `x0` to `x1` of a dome: the top half of the ellipse at
/// `center` with radii `rx` and `ry`, closed along its base.
fn dome(center: (f32, f32), rx: f32, ry: f32, x0: f32, x1: f32) -> Vec<(f32, f32)> {
    const STEPS: usize = 8;
    let top = |x: f32| {
        let t = ((x - center.0) / rx).clamp(-1.0, 1.0);
        center.1 - ry * (1.0 - t * t).sqrt()
    };
    let mut points = vec![(x0, center.1)];
    points.extend((0..=STEPS).map(|i| {
        let x = x0 + (x1 - x0) * i as f32 / STEPS as f32;
        (x, top(x))
    }));
    points.push((x1, center.1));
    points
}

/// Points along the quadratic Bézier curve from `a` to `b` bent toward
/// `control`, excluding `a`.
fn quadratic(
    a: (f32, f32),
    control: (f32, f32),
    b: (f32, f32),
) -> impl Iterator<Item = (f32, f32)> {
    const STEPS: usize = 8;
    (1..=STEPS).map(move |i| {
        let t = i as f32 / STEPS as f32;
        let (u, v, w) = ((1.0 - t) * (1.0 - t), 2.0 * (1.0 - t) * t, t * t);
        (
            u * a.0 + v * control.0 + w * b.0,
            u * a.1 + v * control.1 + w * b.1,
        )
    })
}

/// Draws shapes given in design coordinates, turned and scaled onto the
/// token.
struct Pen<'a> {
    center: Vec2,
    /// World units per design unit.
    scale: f32,
    /// The current turn as (cos, sin), applied before flipping Y up.
    turn: Vec2,
    color: Color,
    out: &'a mut Vec<Vertex>,
}

impl Pen<'_> {
    /// Turns everything drawn after by `degrees`, counterclockwise on screen
    /// when negative (as SVG's `rotate` would with Y down).
    fn turn(&mut self, degrees: f32) {
        self.turn = Vec2::from_angle(degrees.to_radians());
    }

    fn at(&self, (x, y): (f32, f32)) -> Vec2 {
        let (cos, sin) = (self.turn.x, self.turn.y);
        let turned = Vec2::new(x * cos - y * sin, x * sin + y * cos);
        self.center + Vec2::new(turned.x, -turned.y) * self.scale
    }

    fn polygon(&mut self, points: &[(f32, f32)]) {
        let points: Vec<Vec2> = points.iter().map(|&p| self.at(p)).collect();
        mesh::polygon(&points, self.color, self.out);
    }

    fn rect(&mut self, x: f32, y: f32, width: f32, height: f32) {
        self.polygon(&[
            (x, y),
            (x + width, y),
            (x + width, y + height),
            (x, y + height),
        ]);
    }

    fn circle(&mut self, x: f32, y: f32, radius: f32) {
        let center = self.at((x, y));
        mesh::regular_polygon(
            center,
            radius * self.scale,
            CIRCLE_SIDES,
            0.0,
            self.color,
            self.out,
        );
    }

    fn ellipse(&mut self, (x, y): (f32, f32), rx: f32, ry: f32) {
        let points: Vec<(f32, f32)> = (0..CIRCLE_SIDES)
            .map(|i| {
                let angle = TAU * i as f32 / CIRCLE_SIDES as f32;
                (x + rx * angle.cos(), y + ry * angle.sin())
            })
            .collect();
        self.polygon(&points);
    }

    /// A band `width` thick along the circle of `radius` around `center`:
    /// a wheel or a tyre.
    fn ring(&mut self, center: (f32, f32), radius: f32, width: f32) {
        mesh::polygon_outline(
            self.at(center),
            radius * self.scale,
            width * self.scale,
            CIRCLE_SIDES + 4,
            0.0,
            self.color,
            self.out,
        );
    }

    fn line(&mut self, from: (f32, f32), to: (f32, f32), width: f32) {
        let (a, b) = (self.at(from), self.at(to));
        mesh::segment(a, b, width * self.scale, self.color, self.out);
    }

    /// A line through `points` with rounded ends.
    fn curve(&mut self, points: &[(f32, f32)], width: f32) {
        let world: Vec<Vec2> = points.iter().map(|&p| self.at(p)).collect();
        mesh::polyline(&world, width * self.scale, self.color, self.out);
        for &end in [points.first(), points.last()].iter().flatten() {
            self.circle(end.0, end.1, width / 2.0);
        }
    }

    /// A band `width` thick along the closed outline through `points`.
    fn outline(&mut self, points: &[(f32, f32)], width: f32) {
        let world: Vec<Vec2> = points.iter().map(|&p| self.at(p)).collect();
        mesh::outline(&world, width * self.scale, self.color, self.out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ICONS: [UnitIcon; 11] = [
        UnitIcon::Scrapper,
        UnitIcon::Bowman,
        UnitIcon::Outrider,
        UnitIcon::Trebuchet,
        UnitIcon::Cyclist,
        UnitIcon::RiotGuard,
        UnitIcon::Skiff,
        UnitIcon::Barge,
        UnitIcon::RustHulk,
        UnitIcon::Flag,
        UnitIcon::Shovel,
    ];

    #[test]
    fn every_pictogram_stays_inside_its_token() {
        let (center, radius) = (Vec2::new(3.0, -2.0), 0.5);
        for icon in ICONS {
            let mut out = Vec::new();
            push_pictogram(center, radius, icon, [0.0; 4], &mut out);
            assert!(!out.is_empty(), "{icon:?} drew nothing");
            for vertex in out {
                let pos = Vec2::new(vertex.pos[0], vertex.pos[1]);
                // Inside the token, clear of its outline.
                assert!(
                    pos.distance(center) < radius * 0.85,
                    "{icon:?} reaches {}",
                    pos.distance(center) / radius
                );
            }
        }
    }
}
