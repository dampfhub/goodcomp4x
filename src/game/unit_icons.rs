//! Unit pictograms: the sea creature each unit is (see `docs/ocean-theme.md`),
//! a bold silhouette built from polygons, circles and tapered ribbons and
//! drawn dark on the unit's token, with eyes and a few details knocked out in
//! a contrasting color. Shapes are laid out on a token of radius 42 with Y
//! pointing down, the coordinates of the mockups they were designed in.

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
    /// Melee: a swordfish in profile, its long bill raised.
    Swordfish,
    /// Ranged: a round pufferfish bristling with spines.
    Pufferfish,
    /// Cavalry: a seahorse facing right, its tail curled.
    Seahorse,
    /// Siege: an octopus with big eyes and curling arms.
    Octopus,
    /// Scout: a dolphin leaping in an arc.
    Dolphin,
    /// Armored: a crab with both claws raised.
    Crab,
    /// Settler: a sea turtle swimming with a flag planted on its shell.
    /// (Named for the flag it had before the ocean theme; `mod.rs` uses it.)
    Flag,
    /// Worker: a curled shrimp. (Named for the shovel it had before the
    /// ocean theme; `draw.rs` uses it.)
    Shovel,
}

impl UnitIcon {
    pub(super) fn of(unit_type: UnitType) -> Self {
        match unit_type {
            UnitType::Melee => Self::Swordfish,
            UnitType::Ranged => Self::Pufferfish,
            UnitType::Cavalry => Self::Seahorse,
            UnitType::Siege => Self::Octopus,
            UnitType::Scout => Self::Dolphin,
            UnitType::Armored => Self::Crab,
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
        UnitIcon::Swordfish => swordfish(&mut pen),
        UnitIcon::Pufferfish => pufferfish(&mut pen),
        UnitIcon::Seahorse => seahorse(&mut pen),
        UnitIcon::Octopus => octopus(&mut pen),
        UnitIcon::Dolphin => dolphin(&mut pen),
        UnitIcon::Crab => crab(&mut pen),
        UnitIcon::Flag => sea_turtle(&mut pen),
        UnitIcon::Shovel => shrimp(&mut pen),
    }
}

fn swordfish(pen: &mut Pen) {
    pen.turn(-20.0);
    // The bill, then a spindle body from the head back to a narrow tail.
    pen.polygon(&[(12.0, -3.2), (34.0, -0.4), (34.0, 0.4), (12.0, 2.2)]);
    let mut body = vec![(15.0, -2.0)];
    body.extend(quadratic((15.0, -2.0), (6.0, -10.0), (-8.0, -8.5)));
    body.extend(quadratic((-8.0, -8.5), (-18.0, -6.5), (-23.0, -2.0)));
    body.push((-23.0, 2.0));
    body.extend(quadratic((-23.0, 2.0), (-16.0, 5.5), (-6.0, 7.5)));
    body.extend(quadratic((-6.0, 7.5), (8.0, 8.5), (15.0, 2.0)));
    pen.polygon(&body);
    // A crescent tail, a tall sickle dorsal fin and a small pectoral fin.
    pen.polygon(&[
        (-21.0, -2.0),
        (-26.0, -8.0),
        (-31.0, -14.5),
        (-28.5, -5.0),
        (-26.0, 0.0),
        (-28.5, 5.0),
        (-31.0, 14.5),
        (-26.0, 8.0),
        (-21.0, 2.0),
    ]);
    pen.polygon(&[
        (7.0, -6.5),
        (1.0, -17.0),
        (-3.0, -22.0),
        (-3.5, -15.0),
        (-6.0, -8.0),
    ]);
    pen.polygon(&[(6.0, 6.0), (-1.0, 11.0), (-5.0, 14.5), (-2.0, 6.5)]);
    pen.eye(8.5, -2.5, 2.6);
}

fn pufferfish(pen: &mut Pen) {
    let body = (0.0, 2.0);
    pen.circle(body.0, body.1, 19.0);
    // Spines all around, except where the tail and the mouth are.
    const SPINES: u32 = 14;
    const MOUTH: f32 = 0.3;
    for i in 0..SPINES {
        let angle = (i as f32 + 0.5) * TAU / SPINES as f32;
        let from_mouth = (angle - MOUTH + TAU / 2.0).rem_euclid(TAU) - TAU / 2.0;
        if angle.cos() < -0.8 || from_mouth.abs() < 0.3 {
            continue;
        }
        let point = |angle: f32, radius: f32| {
            (body.0 + radius * angle.cos(), body.1 + radius * angle.sin())
        };
        pen.polygon(&[
            point(angle - 0.16, 17.0),
            point(angle, 29.5),
            point(angle + 0.16, 17.0),
        ]);
    }
    // A fan tail, and a pouting mouth.
    pen.polygon(&[(-16.0, 2.0), (-29.0, -8.0), (-26.5, 2.0), (-29.0, 12.0)]);
    pen.circle(19.0, 7.5, 4.0);
    pen.dot(20.5, 7.5, 1.6);
    pen.eye(8.0, -4.0, 5.5);
}

fn seahorse(pen: &mut Pen) {
    // Head and body in one outline, clockwise from the crown.
    let mut body = vec![(-4.0, -31.0), (0.0, -33.0), (2.0, -29.0), (5.0, -25.0)];
    body.extend([(9.0, -23.5), (20.0, -22.5), (21.5, -19.0), (9.0, -17.0)]);
    body.extend([(5.0, -14.0), (3.0, -10.0), (4.0, -6.0)]);
    body.extend(quadratic((4.0, -6.0), (15.0, 3.0), (8.0, 14.0)));
    body.extend([(3.0, 19.5), (-7.5, 18.0)]);
    body.extend(quadratic((-7.5, 18.0), (-14.0, 7.0), (-10.0, -4.0)));
    body.extend([(-8.5, -12.0), (-11.5, -18.0), (-10.5, -25.0), (-7.5, -29.5)]);
    pen.polygon(&body);
    // The tail, curling forward under the belly.
    pen.ribbon(&[
        (-2.5, 16.0, 10.0),
        (-3.0, 23.5, 8.0),
        (0.0, 29.0, 6.5),
        (6.0, 30.5, 5.5),
        (11.0, 27.5, 4.5),
        (12.0, 22.5, 4.0),
        (9.0, 20.0, 3.0),
        (6.5, 22.5, 2.5),
    ]);
    // The fin on its back.
    pen.polygon(&[
        (-10.0, -1.0),
        (-17.5, -3.0),
        (-19.0, 3.5),
        (-17.0, 9.5),
        (-11.0, 9.0),
    ]);
    // Rings down the belly.
    for (from, to) in [
        ((-9.0, -2.0), (4.5, -3.0)),
        ((-10.0, 4.0), (8.0, 3.0)),
        ((-9.5, 10.0), (8.0, 9.0)),
    ] {
        pen.knock_line(from, to, 1.4);
    }
    pen.eye(2.0, -22.0, 2.8);
}

fn octopus(pen: &mut Pen) {
    // Eight arms would be a tangle: three a side and two tucked in the middle.
    for side in [-1.0, 1.0] {
        let arm = |points: &[(f32, f32, f32)]| -> Vec<(f32, f32, f32)> {
            points.iter().map(|&(x, y, w)| (side * x, y, w)).collect()
        };
        pen.ribbon(&arm(&[
            (10.0, -3.0, 8.0),
            (18.0, 2.0, 6.5),
            (25.0, 5.0, 5.0),
            (30.0, 3.0, 4.0),
            (31.0, -2.5, 3.0),
            (28.0, -5.0, 2.2),
        ]));
        pen.ribbon(&arm(&[
            (7.0, 0.0, 8.0),
            (12.0, 12.0, 6.5),
            (18.0, 20.0, 5.0),
            (24.5, 21.0, 4.0),
            (26.5, 15.5, 3.0),
            (23.5, 13.5, 2.2),
        ]));
        pen.ribbon(&arm(&[
            (3.0, 2.0, 7.5),
            (4.0, 16.0, 6.0),
            (7.0, 25.0, 5.0),
            (12.5, 29.0, 4.0),
            (16.0, 25.5, 3.0),
            (14.0, 22.5, 2.2),
        ]));
    }
    // The mantle, and two big eyes low on it.
    pen.ellipse((0.0, -13.0), 15.5, 17.0);
    pen.eye(-6.0, -7.5, 4.2);
    pen.eye(6.0, -7.5, 4.2);
}

fn dolphin(pen: &mut Pen) {
    // The body follows an arc over the top, from the tail at the lower
    // left to the head coming down at the right.
    let (center, radius) = ((0.0, 15.0), 25.0);
    let at = |degrees: f32, out: f32| {
        let (sin, cos) = degrees.to_radians().sin_cos();
        (
            center.0 + (radius + out) * cos,
            center.1 - (radius + out) * sin,
        )
    };
    let spine: Vec<(f32, f32, f32)> = [
        (170.0, 3.5),
        (155.0, 6.5),
        (135.0, 10.5),
        (115.0, 13.5),
        (95.0, 15.0),
        (75.0, 14.5),
        (58.0, 12.5),
        (46.0, 9.0),
        (38.0, 4.0),
        (30.0, 3.2),
        (24.0, 2.6),
    ]
    .iter()
    .map(|&(degrees, width)| {
        let (x, y) = at(degrees, 0.0);
        (x, y, width)
    })
    .collect();
    pen.ribbon(&spine);
    // A rounded melon over the beak.
    let melon = at(52.0, 1.5);
    pen.circle(melon.0, melon.1, 6.0);
    // Dorsal fin, curving back toward the tail.
    let (a, b) = (at(108.0, 5.0), at(88.0, 5.0));
    let tip = at(112.0, 15.0);
    let mut fin = vec![b];
    fin.extend(quadratic(b, at(100.0, 15.0), tip));
    fin.push(a);
    pen.polygon(&fin);
    // Flukes across the end of the tail, and a flipper.
    let tail = at(172.0, 0.0);
    pen.polygon(&[
        (tail.0 + 2.0, tail.1 - 2.0),
        (tail.0 - 9.0, tail.1 - 1.0),
        (tail.0 - 6.0, tail.1 + 2.0),
        (tail.0 - 1.0, tail.1 + 3.0),
        (tail.0 + 1.0, tail.1 + 9.0),
        (tail.0 + 4.0, tail.1 + 5.0),
        (tail.0 + 4.0, tail.1),
    ]);
    pen.polygon(&[at(84.0, -4.0), at(100.0, -16.0), at(68.0, -5.0)]);
    let eye = at(49.0, -1.0);
    pen.eye(eye.0, eye.1, 2.0);
}

fn crab(pen: &mut Pen) {
    for side in [-1.0, 1.0] {
        let mirror = |x: f32, y: f32, w: f32| (side * x, y, w);
        // Three walking legs, each bending down at its knee.
        for (root, knee, foot) in [
            ((12.0, 5.0), (22.0, 0.0), (29.0, 7.0)),
            ((14.0, 9.0), (25.0, 8.0), (29.5, 16.0)),
            ((12.0, 13.0), (21.0, 16.0), (23.5, 23.5)),
        ] {
            pen.ribbon(&[
                mirror(root.0, root.1, 4.5),
                mirror(knee.0, knee.1, 4.0),
                mirror(foot.0, foot.1, 2.2),
            ]);
        }
        // An arm raised to a big claw, its pincers open upward.
        pen.ribbon(&[mirror(9.0, 1.0, 6.0), mirror(16.0, -9.0, 5.5)]);
        pen.ellipse((side * 17.5, -15.0), 7.0, 6.5);
        pen.ribbon(&[
            mirror(20.0, -18.0, 7.0),
            mirror(21.5, -24.0, 5.0),
            mirror(17.5, -28.5, 2.2),
        ]);
        pen.ribbon(&[
            mirror(13.5, -18.5, 5.5),
            mirror(12.0, -24.0, 3.5),
            mirror(13.0, -27.5, 2.0),
        ]);
        // An eye on a stalk.
        pen.line((side * 4.0, -2.0), (side * 5.5, -9.0), 2.5);
        pen.circle(side * 5.5, -10.0, 3.4);
        pen.dot(side * 5.5, -10.5, 1.4);
    }
    // The shell over everything, with a smile.
    pen.ellipse((0.0, 7.0), 18.0, 11.5);
    pen.knock_curve(&quadratic_from((-5.0, 8.5), (0.0, 12.0), (5.0, 8.5)), 1.4);
}

fn sea_turtle(pen: &mut Pen) {
    // A flag planted on its back.
    pen.rect(-3.5, -31.0, 2.6, 26.0);
    pen.polygon(&[
        (-1.0, -31.0),
        (8.0, -29.0),
        (16.0, -26.5),
        (8.0, -24.5),
        (-1.0, -21.0),
    ]);
    // Flippers, the front one swept back, then the head on its neck.
    pen.polygon(&[
        (12.0, 8.0),
        (14.5, 12.0),
        (7.0, 19.0),
        (-3.0, 25.5),
        (-5.5, 23.5),
        (0.0, 16.0),
        (5.0, 10.0),
    ]);
    pen.polygon(&[
        (-13.0, 10.0),
        (-9.0, 12.0),
        (-15.0, 18.0),
        (-22.0, 18.5),
        (-22.0, 14.0),
    ]);
    pen.ribbon(&[(12.0, 5.0, 8.0), (18.0, 3.0, 7.0)]);
    pen.ellipse((22.5, 1.5), 6.5, 5.5);
    // A domed shell over a flat belly, the seam knocked out between them.
    let mut shell = vec![(-21.0, 8.0)];
    shell.extend(quadratic((-21.0, 8.0), (-18.0, -9.0), (-1.0, -9.0)));
    shell.extend(quadratic((-1.0, -9.0), (16.0, -9.0), (18.0, 8.0)));
    shell.extend(quadratic((18.0, 8.0), (-1.0, 16.0), (-21.0, 8.0)));
    shell.pop();
    pen.polygon(&shell);
    pen.knock_curve(
        &quadratic_from((-19.0, 6.5), (-1.0, 12.0), (16.5, 6.5)),
        1.5,
    );
    // Plates on the shell.
    pen.knock_curve(
        &quadratic_from((-12.0, 5.0), (-9.0, -2.0), (-4.0, -5.5)),
        1.3,
    );
    pen.knock_curve(&quadratic_from((9.0, 5.0), (6.0, -2.0), (1.0, -5.5)), 1.3);
    pen.eye(24.0, 0.0, 1.8);
}

fn shrimp(pen: &mut Pen) {
    // A body curled in a C from the head at the right, over the top and
    // down the left, to a fan tail under it.
    let (center, radius) = ((-1.0, 3.0), 15.0);
    let at = |degrees: f32, out: f32| {
        let (sin, cos) = degrees.to_radians().sin_cos();
        (
            center.0 + (radius + out) * cos,
            center.1 - (radius + out) * sin,
        )
    };
    // Antennae, sweeping back over the body.
    pen.curve(
        &quadratic_from((17.0, -8.0), (22.0, -27.0), (-6.0, -30.0)),
        1.6,
    );
    pen.curve(
        &quadratic_from((18.0, -6.0), (30.0, -16.0), (25.0, -24.0)),
        1.6,
    );
    // Legs under the head.
    for (from, to) in [
        ((9.0, 6.0), (10.0, 12.5)),
        ((13.0, 6.0), (15.0, 12.5)),
        ((17.0, 4.0), (20.5, 10.0)),
    ] {
        pen.line(from, to, 1.8);
    }
    let body: Vec<(f32, f32, f32)> = [
        (-5.0, 11.0),
        (20.0, 14.0),
        (50.0, 14.0),
        (90.0, 13.0),
        (130.0, 11.5),
        (170.0, 10.0),
        (210.0, 8.0),
        (245.0, 6.0),
        (265.0, 4.5),
    ]
    .iter()
    .map(|&(degrees, width)| {
        let (x, y) = at(degrees, 0.0);
        (x, y, width)
    })
    .collect();
    pen.ribbon(&body);
    // The head: a rounded shell over the front of the body.
    pen.ellipse((13.5, -0.5), 7.5, 9.0);
    // The pointed rostrum, and a fan tail.
    let snout = at(-5.0, 0.0);
    pen.polygon(&[
        (snout.0 - 2.0, snout.1 - 5.0),
        (snout.0 + 13.0, snout.1 - 4.0),
        (snout.0 + 1.0, snout.1 + 1.0),
    ]);
    let tail = at(268.0, 0.0);
    // Two paddles either side of a pointed middle.
    pen.polygon(&[
        (tail.0, tail.1 - 3.0),
        (tail.0 + 9.0, tail.1 - 8.0),
        (tail.0 + 7.0, tail.1 - 2.0),
        (tail.0 + 12.0, tail.1 + 0.5),
        (tail.0 + 7.0, tail.1 + 3.0),
        (tail.0 + 8.0, tail.1 + 9.0),
        (tail.0, tail.1 + 3.0),
    ]);
    // Shell segments across the back.
    for (degrees, reach) in [(95.0, 5.0), (130.0, 4.4), (165.0, 3.8), (200.0, 3.0)] {
        pen.knock_line(at(degrees, -reach), at(degrees, reach), 1.2);
    }
    pen.eye(10.5, -8.0, 2.4);
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

/// The whole quadratic Bézier curve from `a` to `b`, `a` included.
fn quadratic_from(a: (f32, f32), control: (f32, f32), b: (f32, f32)) -> Vec<(f32, f32)> {
    std::iter::once(a).chain(quadratic(a, control, b)).collect()
}

/// `(x, y, width)` points with more between each pair, on a Catmull-Rom
/// curve through them, so a ribbon's bends come out round.
fn smooth(points: &[(f32, f32, f32)]) -> Vec<(f32, f32, f32)> {
    const STEPS: usize = 4;
    let point = |i: usize| {
        let (x, y, w) = points[i.min(points.len() - 1)];
        glam::Vec3::new(x, y, w)
    };
    let mut smooth = vec![points[0]];
    for i in 0..points.len().saturating_sub(1) {
        let (p0, p1, p2, p3) = (
            point(i.saturating_sub(1)),
            point(i),
            point(i + 1),
            point(i + 2),
        );
        for step in 1..=STEPS {
            let t = step as f32 / STEPS as f32;
            let p = 0.5
                * (2.0 * p1
                    + (p2 - p0) * t
                    + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t * t
                    + (3.0 * p1 - p0 - 3.0 * p2 + p3) * t * t * t);
            smooth.push((p.x, p.y, p.z));
        }
    }
    smooth
}

/// A color that stands out on `color`: near white on a dark pictogram, near
/// black on a light one, for eyes and details inside the silhouette.
fn contrast(color: Color) -> Color {
    let luminance = 0.2126 * color[0] + 0.7152 * color[1] + 0.0722 * color[2];
    if luminance < 0.25 {
        [0.92, 0.95, 0.93, color[3]]
    } else {
        [0.03, 0.03, 0.04, color[3]]
    }
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
        const SIDES: u32 = 28;
        let points: Vec<(f32, f32)> = (0..SIDES)
            .map(|i| {
                let (sin, cos) = (TAU * i as f32 / SIDES as f32).sin_cos();
                (x + rx * cos, y + ry * sin)
            })
            .collect();
        self.polygon(&points);
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

    /// A stroke through `(x, y, width)` points whose width changes along it,
    /// as one outline, with a rounded end: tails, arms and bodies.
    fn ribbon(&mut self, points: &[(f32, f32, f32)]) {
        let points = smooth(points);
        let spine: Vec<Vec2> = points.iter().map(|&(x, y, _)| Vec2::new(x, y)).collect();
        let direction = |i: usize| (spine[i + 1] - spine[i]).normalize_or_zero();
        let last = spine.len() - 1;
        let sides: Vec<Vec2> = (0..spine.len())
            .map(|i| {
                let before = direction(i.saturating_sub(1).min(last - 1));
                let after = direction(i.min(last - 1));
                (before + after).normalize_or(after).perp() * (points[i].2 / 2.0)
            })
            .collect();
        let outline: Vec<(f32, f32)> = spine
            .iter()
            .zip(&sides)
            .map(|(&p, &side)| p + side)
            .chain(spine.iter().zip(&sides).rev().map(|(&p, &side)| p - side))
            .map(|p| (p.x, p.y))
            .collect();
        self.polygon(&outline);
        let (x, y, width) = points[last];
        self.circle(x, y, width / 2.0);
    }

    /// A round eye: a contrasting disc with a pupil looking forward.
    fn eye(&mut self, x: f32, y: f32, radius: f32) {
        self.dot(x, y, radius);
        self.circle(x + radius * 0.25, y, radius * 0.5);
    }

    /// A contrasting disc.
    fn dot(&mut self, x: f32, y: f32, radius: f32) {
        let color = self.color;
        self.color = contrast(color);
        self.circle(x, y, radius);
        self.color = color;
    }

    /// A contrasting line, for markings inside the silhouette.
    fn knock_line(&mut self, from: (f32, f32), to: (f32, f32), width: f32) {
        let color = self.color;
        self.color = contrast(color);
        self.line(from, to, width);
        self.color = color;
    }

    /// A contrasting curve, for markings inside the silhouette.
    fn knock_curve(&mut self, points: &[(f32, f32)], width: f32) {
        let color = self.color;
        self.color = contrast(color);
        self.curve(points, width);
        self.color = color;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ICONS: [UnitIcon; 8] = [
        UnitIcon::Swordfish,
        UnitIcon::Pufferfish,
        UnitIcon::Seahorse,
        UnitIcon::Octopus,
        UnitIcon::Dolphin,
        UnitIcon::Crab,
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
