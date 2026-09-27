//! The window and taskbar icon, drawn in code rather than loaded from a file:
//! a tropical fish in Blue's cyan, swimming left, with a dark rim like the
//! map's hex borders.

use glam::Vec2;
use winit::window::Icon;

type Rgba = [u8; 4];

/// Blue's reef cyan as it appears on screen, a pale pearl stripe, and the
/// deep-water rim and eye.
const FILL: Rgba = [64, 196, 232, 255];
const FIN: Rgba = [40, 150, 205, 255];
const STRIPE: Rgba = [232, 246, 244, 255];
const RIM: Rgba = [8, 30, 44, 255];
const EYE: Rgba = [8, 30, 44, 255];

/// The body: an ellipse, in the icon's [-1, 1] square.
const BODY_CENTER: Vec2 = Vec2::new(-0.14, 0.0);
const BODY_RADII: Vec2 = Vec2::new(0.62, 0.42);
/// The tail fans out behind the body.
const TAIL: [Vec2; 3] = [
    Vec2::new(0.30, 0.0),
    Vec2::new(0.90, 0.46),
    Vec2::new(0.90, -0.46),
];
/// A dorsal fin on the back.
const FIN_TOP: [Vec2; 3] = [
    Vec2::new(-0.36, 0.32),
    Vec2::new(0.02, 0.66),
    Vec2::new(0.16, 0.30),
];
/// Rim thickness around every part.
const RIM_WIDTH: f32 = 0.07;
/// The pale band behind the head, and the eye.
const STRIPE_X: f32 = -0.30;
const STRIPE_HALF_WIDTH: f32 = 0.07;
const EYE_CENTER: Vec2 = Vec2::new(-0.52, 0.09);
const EYE_RADIUS: f32 = 0.075;
/// Samples per pixel along each axis, for smooth edges.
const SUPERSAMPLE: u32 = 4;

/// The icon rendered at `size` by `size` pixels.
pub fn icon(size: u32) -> Icon {
    let mut rgba = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            rgba.extend(pixel(x, y, size));
        }
    }
    Icon::from_rgba(rgba, size, size).expect("icon pixels match its size")
}

/// Averages the shape over a grid of points inside pixel (`x`, `y`): color
/// from the points that hit something, alpha from how many did.
fn pixel(x: u32, y: u32, size: u32) -> Rgba {
    let mut color = [0.0f32; 3];
    let mut hits = 0;
    for sy in 0..SUPERSAMPLE {
        for sx in 0..SUPERSAMPLE {
            let offset = |i: u32| (i as f32 + 0.5) / SUPERSAMPLE as f32;
            let u = (x as f32 + offset(sx)) / size as f32;
            let v = (y as f32 + offset(sy)) / size as f32;
            // Rows run top to bottom; the shapes are defined with Y up.
            if let Some(sample) = shape_at(Vec2::new(u * 2.0 - 1.0, 1.0 - v * 2.0)) {
                for (sum, channel) in color.iter_mut().zip(sample) {
                    *sum += channel as f32;
                }
                hits += 1;
            }
        }
    }
    if hits == 0 {
        return [0; 4];
    }
    let coverage = hits as f32 / (SUPERSAMPLE * SUPERSAMPLE) as f32;
    let [r, g, b] = color.map(|sum| (sum / hits as f32).round() as u8);
    [r, g, b, (coverage * 255.0).round() as u8]
}

fn shape_at(p: Vec2) -> Option<Rgba> {
    if in_body(p, 0.0) {
        Some(if p.distance(EYE_CENTER) <= EYE_RADIUS {
            EYE
        } else if (p.x - STRIPE_X).abs() <= STRIPE_HALF_WIDTH {
            STRIPE
        } else {
            FILL
        })
    } else if in_triangle(p, TAIL) || in_triangle(p, FIN_TOP) {
        Some(FIN)
    } else if in_body(p, RIM_WIDTH)
        || near_triangle(p, TAIL, RIM_WIDTH)
        || near_triangle(p, FIN_TOP, RIM_WIDTH)
    {
        Some(RIM)
    } else {
        None
    }
}

/// Inside the body's ellipse, grown by `grow` on each radius.
fn in_body(p: Vec2, grow: f32) -> bool {
    let d = (p - BODY_CENTER) / (BODY_RADII + Vec2::splat(grow));
    d.length_squared() <= 1.0
}

fn in_triangle(p: Vec2, [a, b, c]: [Vec2; 3]) -> bool {
    let side = |from: Vec2, to: Vec2| (to - from).perp_dot(p - from);
    let (ab, bc, ca) = (side(a, b), side(b, c), side(c, a));
    (ab >= 0.0 && bc >= 0.0 && ca >= 0.0) || (ab <= 0.0 && bc <= 0.0 && ca <= 0.0)
}

/// Inside the triangle or within `distance` of one of its edges.
fn near_triangle(p: Vec2, corners: [Vec2; 3], distance: f32) -> bool {
    let [a, b, c] = corners;
    in_triangle(p, corners)
        || [(a, b), (b, c), (c, a)]
            .into_iter()
            .any(|(from, to)| segment_distance(p, from, to) <= distance)
}

fn segment_distance(p: Vec2, from: Vec2, to: Vec2) -> f32 {
    let along = to - from;
    let t = ((p - from).dot(along) / along.length_squared()).clamp(0.0, 1.0);
    p.distance(from + along * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corners_are_transparent_and_the_fish_fills_the_middle() {
        assert_eq!(pixel(0, 0, 32), [0; 4]);
        assert_eq!(pixel(31, 31, 32), [0; 4]);
        assert_eq!(pixel(16, 16, 32), FILL);
    }

    #[test]
    fn the_fish_has_an_eye_and_a_tail() {
        // Pixel centers at the eye and in the middle of the tail.
        let at = |p: Vec2, size: u32| {
            let x = ((p.x + 1.0) / 2.0 * size as f32) as u32;
            let y = ((1.0 - p.y) / 2.0 * size as f32) as u32;
            pixel(x, y, size)
        };
        assert_eq!(at(EYE_CENTER, 256), EYE);
        assert_eq!(at(Vec2::new(0.75, 0.0), 256), FIN);
    }
}
