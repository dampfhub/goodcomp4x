use glam::Vec2;

use crate::renderer::Vertex;

/// Appends a filled regular polygon, as a triangle fan around its center.
pub fn regular_polygon(
    center: Vec2,
    radius: f32,
    sides: u32,
    rotation: f32,
    color: [f32; 4],
    out: &mut Vec<Vertex>,
) {
    let corner = |i: u32| {
        let angle = rotation + std::f32::consts::TAU * i as f32 / sides as f32;
        center + Vec2::from_angle(angle) * radius
    };
    for i in 0..sides {
        push_triangle(out, center, corner(i), corner(i + 1), color);
    }
}

/// Appends an axis-aligned filled rectangle spanning `min`..`max`.
pub fn quad(min: Vec2, max: Vec2, color: [f32; 4], out: &mut Vec<Vertex>) {
    let bottom_right = Vec2::new(max.x, min.y);
    let top_left = Vec2::new(min.x, max.y);
    push_triangle(out, min, bottom_right, max, color);
    push_triangle(out, min, max, top_left, color);
}

/// Appends a straight line from `a` to `b`, `width` world units thick.
pub fn segment(a: Vec2, b: Vec2, width: f32, color: [f32; 4], out: &mut Vec<Vertex>) {
    let side = (b - a).perp().normalize_or_zero() * (width / 2.0);
    push_triangle(out, a - side, b - side, b + side, color);
    push_triangle(out, a - side, b + side, a + side, color);
}

fn push_triangle(out: &mut Vec<Vertex>, a: Vec2, b: Vec2, c: Vec2, color: [f32; 4]) {
    out.extend([a, b, c].map(|p| Vertex {
        pos: [p.x, p.y, 0.0],
        color,
    }));
}
