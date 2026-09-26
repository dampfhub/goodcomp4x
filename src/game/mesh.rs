use glam::Vec2;

use crate::renderer::{SOLID_UV, Vertex};

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

/// Appends the outline of a regular polygon: a band `width` thick centered on
/// the polygon of `radius`, with mitered corners so the edges join cleanly.
pub fn polygon_outline(
    center: Vec2,
    radius: f32,
    width: f32,
    sides: u32,
    rotation: f32,
    color: [f32; 4],
    out: &mut Vec<Vertex>,
) {
    // Moving an edge `width / 2` along its normal moves the corners further,
    // since they're farther from the center than the edge's midpoint.
    let corner_offset = width / 2.0 / (std::f32::consts::PI / sides as f32).cos();
    let corner = |i: u32, radius: f32| {
        let angle = rotation + std::f32::consts::TAU * i as f32 / sides as f32;
        center + Vec2::from_angle(angle) * radius
    };
    let (outer, inner) = (radius + corner_offset, radius - corner_offset);
    for i in 0..sides {
        let (a, b) = (corner(i, outer), corner(i + 1, outer));
        let (c, d) = (corner(i, inner), corner(i + 1, inner));
        push_triangle(out, a, b, d, color);
        push_triangle(out, a, d, c, color);
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
        uv: SOLID_UV,
    }));
}
