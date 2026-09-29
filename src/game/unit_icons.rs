//! Unit pictograms: a small picture of what each unit is, built from a few
//! rectangles, triangles, circles and lines and drawn dark on the unit's
//! token. Shapes are laid out on a token of radius 42 with Y pointing down,
//! the coordinates of the mockups they were designed in.

use glam::Vec2;

use super::fast_hash::HashMap;
use super::mesh;
use super::unit::UnitType;
use crate::renderer::Vertex;

use super::mesh::Color;

/// Token radius the shapes are laid out on.
const DESIGN_RADIUS: f32 = 42.0;
/// Sides of the polygons that stand in for circles.
const CIRCLE_SIDES: u32 = 16;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(super) enum UnitIcon {
    /// Melee: an upright sword.
    Sword,
    /// Ranged: a bow with an arrow nocked, aimed up and to the right.
    Bow,
    /// Cavalry: a horse's head, facing right.
    HorseHead,
    /// Siege: a catapult on solid wheels.
    Catapult,
    /// Scout: a spyglass.
    Spyglass,
    /// Armored: a heater shield.
    Shield,
    /// Patrol galley: a narrow hull with oars.
    Galley,
    /// Landing craft: a broad troop transport.
    LandingCraft,
    /// Bombard ship: a hull carrying a cannon.
    BombardShip,
    /// Settler: a flag planted to found a city.
    Flag,
    /// Worker: a shovel.
    Shovel,
}

impl UnitIcon {
    pub(super) fn of(unit_type: UnitType) -> Self {
        match unit_type {
            UnitType::Melee => Self::Sword,
            UnitType::Ranged => Self::Bow,
            UnitType::Cavalry => Self::HorseHead,
            UnitType::Siege => Self::Catapult,
            UnitType::Scout => Self::Spyglass,
            UnitType::Armored => Self::Shield,
            UnitType::PatrolGalley => Self::Galley,
            UnitType::LandingCraft => Self::LandingCraft,
            UnitType::BombardShip => Self::BombardShip,
        }
    }
}

/// Draws `icon` in `color` centered on a token of `radius` at `center`.
/// Each pictogram's triangles are worked out once (`build_pictogram`),
/// then placed.
pub(super) fn push_pictogram(
    center: Vec2,
    radius: f32,
    icon: UnitIcon,
    color: Color,
    out: &mut Vec<Vertex>,
) {
    thread_local! {
        static MESHES: std::cell::RefCell<HashMap<UnitIcon, Vec<Vertex>>> = Default::default();
    }
    MESHES.with_borrow_mut(|meshes| {
        let shape = meshes.entry(icon).or_insert_with(|| {
            let mut shape = Vec::new();
            build_pictogram(icon, &mut shape);
            shape
        });
        mesh::place(shape, center, radius / DESIGN_RADIUS, Some(color), out);
    });
}

/// `icon`'s triangles on a token of `DESIGN_RADIUS` at the origin.
fn build_pictogram(icon: UnitIcon, out: &mut Vec<Vertex>) {
    let mut pen = Pen {
        center: Vec2::ZERO,
        scale: 1.0,
        turn: Vec2::X,
        color: [1.0; 4],
        out,
    };
    match icon {
        UnitIcon::Sword => {
            pen.polygon(&[(-5.0, -22.0), (0.0, -32.0), (5.0, -22.0)]);
            pen.rect(-5.0, -22.0, 10.0, 32.0);
            pen.rect(-15.0, 10.0, 30.0, 6.0);
            pen.rect(-3.0, 16.0, 6.0, 10.0);
            pen.circle(0.0, 29.0, 4.5);
        }
        UnitIcon::Bow => {
            pen.turn(-45.0);
            // The bow's limbs: an arc of radius 34 whose ends sit on the string.
            let (cx, r) = (-6.0 - (34.0f32 * 34.0 - 27.0 * 27.0).sqrt(), 34.0);
            let reach = (27.0f32 / r).asin();
            let limbs: Vec<(f32, f32)> = (0..=12)
                .map(|i| {
                    let angle = -reach + 2.0 * reach * i as f32 / 12.0;
                    (cx + r * angle.cos(), r * angle.sin())
                })
                .collect();
            pen.curve(&limbs, 5.5);
            pen.line((-6.0, -27.0), (-6.0, 27.0), 2.0);
            pen.line((-26.0, 0.0), (19.0, 0.0), 3.5);
            pen.polygon(&[(18.0, -7.5), (31.0, 0.0), (18.0, 7.5)]);
            pen.polygon(&[(-19.0, 0.0), (-26.0, -7.0), (-31.0, -7.0), (-24.0, 0.0)]);
            pen.polygon(&[(-19.0, 0.0), (-24.0, 0.0), (-31.0, 7.0), (-26.0, 7.0)]);
        }
        UnitIcon::HorseHead => pen.polygon(&[
            (-17.0, 29.0),
            (17.0, 29.0),
            (14.0, 19.0),
            (3.0, 8.0),
            (19.0, 5.0),
            (26.0, -3.0),
            (22.0, -11.0),
            (7.0, -21.0),
            (4.0, -30.0),
            (-1.0, -24.0),
            (-10.0, -26.0),
            (-21.0, -14.0),
            (-25.0, 4.0),
            (-22.0, 19.0),
        ]),
        UnitIcon::Catapult => {
            pen.rect(-26.0, 11.0, 50.0, 7.0);
            pen.line((16.0, 12.0), (-17.0, -20.0), 5.0);
            pen.rect(-5.0, -6.0, 7.0, 17.0);
            pen.circle(-18.5, -21.5, 6.5);
            pen.circle(-15.0, 22.0, 8.0);
            pen.circle(14.0, 22.0, 8.0);
        }
        UnitIcon::Spyglass => {
            pen.turn(-35.0);
            // Eyepiece to lens, each section wider than the last.
            pen.rect(-33.0, -4.0, 5.0, 8.0);
            pen.rect(-28.0, -5.0, 16.0, 10.0);
            pen.rect(-12.0, -6.5, 16.0, 13.0);
            pen.rect(4.0, -8.0, 18.0, 16.0);
            pen.rect(22.0, -10.0, 6.0, 20.0);
        }
        UnitIcon::Shield => {
            let mut outline = vec![(-22.0, -26.0), (22.0, -26.0), (22.0, -4.0)];
            outline.extend(quadratic((22.0, -4.0), (22.0, 18.0), (0.0, 31.0)));
            outline.extend(quadratic((0.0, 31.0), (-22.0, 18.0), (-22.0, -4.0)));
            outline.pop();
            pen.polygon(&outline);
        }
        UnitIcon::Galley => {
            pen.polygon(&[(-30.0, 8.0), (30.0, 8.0), (21.0, 21.0), (-21.0, 21.0)]);
            pen.rect(-2.0, -28.0, 4.0, 34.0);
            pen.polygon(&[(1.0, -25.0), (21.0, 3.0), (1.0, 3.0)]);
            pen.line((-22.0, 26.0), (22.0, 26.0), 2.0);
        }
        UnitIcon::LandingCraft => {
            pen.polygon(&[
                (-29.0, 3.0),
                (22.0, 3.0),
                (30.0, 10.0),
                (20.0, 22.0),
                (-22.0, 22.0),
            ]);
            pen.rect(-23.0, -3.0, 5.0, 7.0);
            for x in [-14.0, -4.0, 6.0, 16.0] {
                pen.circle(x, 7.0, 3.0);
            }
            pen.line((22.0, 3.0), (31.0, -3.0), 3.0);
        }
        UnitIcon::BombardShip => {
            pen.polygon(&[(-31.0, 9.0), (31.0, 9.0), (22.0, 23.0), (-22.0, 23.0)]);
            pen.rect(-14.0, -1.0, 18.0, 10.0);
            pen.circle(6.0, 0.0, 9.0);
            pen.line((10.0, -5.0), (26.0, -22.0), 5.0);
        }
        UnitIcon::Flag => {
            pen.rect(-10.0, -20.0, 4.0, 34.0);
            pen.polygon(&[(-6.0, -20.0), (16.0, -12.5), (-6.0, -5.0)]);
            pen.rect(-17.0, 14.0, 18.0, 4.0);
        }
        UnitIcon::Shovel => {
            pen.turn(-30.0);
            pen.rect(-8.0, -21.0, 16.0, 4.0);
            pen.rect(-2.0, -17.0, 4.0, 18.0);
            pen.polygon(&[
                (-8.0, 1.0),
                (8.0, 1.0),
                (8.0, 13.0),
                (0.0, 21.0),
                (-8.0, 13.0),
            ]);
        }
    }
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
}

#[cfg(test)]
mod tests {
    use super::*;

    const ICONS: [UnitIcon; 11] = [
        UnitIcon::Sword,
        UnitIcon::Bow,
        UnitIcon::HorseHead,
        UnitIcon::Catapult,
        UnitIcon::Spyglass,
        UnitIcon::Shield,
        UnitIcon::Galley,
        UnitIcon::LandingCraft,
        UnitIcon::BombardShip,
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
