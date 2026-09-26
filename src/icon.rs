//! The window and taskbar icon, drawn in code rather than loaded from a file:
//! a blue hex tile with a white triangle, like a melee unit on the map.

use glam::Vec2;
use winit::window::Icon;

type Rgba = [u8; 4];

/// Blue's team color as it appears on screen (after the sRGB swapchain's
/// encoding), with a dark rim like the map's hex borders.
const FILL: Rgba = [149, 196, 250, 255];
const RIM: Rgba = [30, 30, 38, 255];
const MARK: Rgba = [255, 255, 255, 255];

/// Radii, in the icon's [-1, 1] square, of the whole hex and its blue fill.
const HEX_RADIUS: f32 = 0.98;
const FILL_RADIUS: f32 = 0.80;
/// The triangle's corners, pointing up.
const TRIANGLE: [Vec2; 3] = [
    Vec2::new(0.0, 0.46),
    Vec2::new(-0.42, -0.30),
    Vec2::new(0.42, -0.30),
];
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
    if in_triangle(p) {
        Some(MARK)
    } else if in_hex(p, FILL_RADIUS) {
        Some(FILL)
    } else if in_hex(p, HEX_RADIUS) {
        Some(RIM)
    } else {
        None
    }
}

/// Inside a flat-topped hexagon centered on the origin.
fn in_hex(p: Vec2, radius: f32) -> bool {
    let (x, y) = (p.x.abs(), p.y.abs());
    let sqrt3 = 3f32.sqrt();
    y <= radius * sqrt3 / 2.0 && sqrt3 * x + y <= sqrt3 * radius
}

fn in_triangle(p: Vec2) -> bool {
    let [a, b, c] = TRIANGLE;
    let side = |from: Vec2, to: Vec2| (to - from).perp_dot(p - from);
    let (ab, bc, ca) = (side(a, b), side(b, c), side(c, a));
    (ab >= 0.0 && bc >= 0.0 && ca >= 0.0) || (ab <= 0.0 && bc <= 0.0 && ca <= 0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corners_are_transparent_and_the_center_is_the_mark() {
        assert_eq!(pixel(0, 0, 32), [0; 4]);
        assert_eq!(pixel(16, 16, 32), MARK);
    }
}
