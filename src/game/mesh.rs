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

/// Appends a line through `points`, `width` world units thick, as one ribbon
/// whose pieces meet exactly at each bend: no gaps, and no overlaps to darken
/// when the color is see-through.
pub fn polyline(points: &[Vec2], width: f32, color: [f32; 4], out: &mut Vec<Vertex>) {
    if points.len() < 2 {
        return;
    }
    let direction = |i: usize| (points[i + 1] - points[i]).normalize_or_zero();
    let sides: Vec<Vec2> = (0..points.len())
        .map(|i| {
            // At a bend, offset along the average of the two edges' normals,
            // stretched so the ribbon keeps its width through the corner.
            let before = if i > 0 {
                direction(i - 1)
            } else {
                direction(i)
            };
            let after = if i + 1 < points.len() {
                direction(i)
            } else {
                before
            };
            let normal = (before + after).normalize_or(after).perp();
            let stretch = normal.dot(after.perp()).max(0.5);
            normal * (width / 2.0 / stretch)
        })
        .collect();
    for i in 0..points.len().saturating_sub(1) {
        let (a, b) = (points[i], points[i + 1]);
        let (side_a, side_b) = (sides[i], sides[i + 1]);
        push_triangle(out, a - side_a, b - side_b, b + side_b, color);
        push_triangle(out, a - side_a, b + side_b, a + side_a, color);
    }
}

/// Appends a band `width` thick centered on the closed outline through
/// `points`: a polygon's edge, drawn before its fill so only the outer half
/// shows, like an SVG stroke painted under the fill.
pub fn outline(points: &[Vec2], width: f32, color: [f32; 4], out: &mut Vec<Vertex>) {
    let Some(&last) = points.last() else {
        return;
    };
    // The closing edge goes both first and last, so every corner is a bend
    // the ribbon miters; the open ends it leaves lie inside those corners.
    let mut around = vec![last];
    around.extend_from_slice(points);
    around.push(points[0]);
    polyline(&around, width, color, out);
}

/// Appends a filled triangle.
pub fn triangle(a: Vec2, b: Vec2, c: Vec2, color: [f32; 4], out: &mut Vec<Vertex>) {
    push_triangle(out, a, b, c, color);
}

/// Appends a filled simple polygon, convex or not, with its corners in either
/// winding order. It's cut into triangles by clipping ears: repeatedly
/// removing a corner that bulges outward with no other corner inside it.
pub fn polygon(points: &[Vec2], color: [f32; 4], out: &mut Vec<Vertex>) {
    // Twice the signed area: which way the corners wind.
    let winding: f32 = (0..points.len())
        .map(|i| points[i].perp_dot(points[(i + 1) % points.len()]))
        .sum();
    let mut left: Vec<usize> = (0..points.len()).collect();
    while left.len() >= 3 {
        let n = left.len();
        let corner = |i: usize| {
            (
                points[left[(i + n - 1) % n]],
                points[left[i]],
                points[left[(i + 1) % n]],
            )
        };
        let ear = (0..n).find(|&i| {
            let (a, b, c) = corner(i);
            let bulges = (b - a).perp_dot(c - b) * winding > 0.0;
            bulges
                && left
                    .iter()
                    .map(|&j| points[j])
                    .all(|p| p == a || p == b || p == c || !inside_triangle(p, a, b, c))
        });
        // Only a degenerate outline (crossing itself, or all in a line) has
        // no ear; stop with what's been cut so far.
        let Some(i) = ear else { break };
        let (a, b, c) = corner(i);
        push_triangle(out, a, b, c, color);
        left.remove(i);
    }
}

/// On or inside the triangle `a`, `b`, `c`, whichever way round it winds.
fn inside_triangle(p: Vec2, a: Vec2, b: Vec2, c: Vec2) -> bool {
    let sides = [
        (b - a).perp_dot(p - a),
        (c - b).perp_dot(p - b),
        (a - c).perp_dot(p - c),
    ];
    sides.iter().all(|&s| s >= 0.0) || sides.iter().all(|&s| s <= 0.0)
}

fn push_triangle(out: &mut Vec<Vertex>, a: Vec2, b: Vec2, c: Vec2, color: [f32; 4]) {
    out.extend([a, b, c].map(|p| Vertex {
        pos: [p.x, p.y, 0.0],
        color,
        uv: SOLID_UV,
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area(out: &[Vertex]) -> f32 {
        out.as_chunks::<3>()
            .0
            .iter()
            .map(|t| {
                let [a, b, c] = [0, 1, 2].map(|i| Vec2::new(t[i].pos[0], t[i].pos[1]));
                (b - a).perp_dot(c - a).abs() / 2.0
            })
            .sum()
    }

    #[test]
    fn concave_polygons_fill_exactly_their_area() {
        // An L shape of area 3, in both winding orders.
        let l = [
            (0.0, 0.0),
            (2.0, 0.0),
            (2.0, 1.0),
            (1.0, 1.0),
            (1.0, 2.0),
            (0.0, 2.0),
        ]
        .map(|(x, y)| Vec2::new(x, y));
        for points in [l.to_vec(), l.iter().rev().copied().collect()] {
            let mut out = Vec::new();
            polygon(&points, [1.0; 4], &mut out);
            assert_eq!(out.len(), 3 * (points.len() - 2));
            assert!((area(&out) - 3.0).abs() < 1e-5);
        }
    }

    #[test]
    fn an_outline_wraps_every_edge_and_leaves_the_middle_open() {
        let square = [(0.0, 0.0), (2.0, 0.0), (2.0, 2.0), (0.0, 2.0)].map(|(x, y)| Vec2::new(x, y));
        let mut out = Vec::new();
        outline(&square, 0.2, [1.0; 4], &mut out);
        let covered = |p: Vec2| {
            out.as_chunks::<3>().0.iter().any(|t| {
                let [a, b, c] = [0, 1, 2].map(|i| Vec2::new(t[i].pos[0], t[i].pos[1]));
                inside_triangle(p, a, b, c)
            })
        };
        // Just outside each edge's middle, and each corner's mitered tip.
        for p in [
            (1.0, -0.09),
            (2.09, 1.0),
            (1.0, 2.09),
            (-0.09, 1.0),
            (-0.09, -0.09),
            (2.09, 2.09),
        ] {
            assert!(covered(Vec2::new(p.0, p.1)), "{p:?} left bare");
        }
        assert!(!covered(Vec2::new(1.0, 1.0)));
    }
}
