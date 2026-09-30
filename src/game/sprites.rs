//! Bake repeated world artwork once; each occurrence then costs one quad.
//! The original vector art remains the source of truth (and serves ImGui).
use std::sync::LazyLock;

use glam::Vec2;

use super::{
    draw,
    map_icons::{self, MapIcon},
    mesh,
    terrain::{Feature, Terrain, Tile},
};
use crate::renderer::{Atlas, Vertex};

const CELL: usize = 256;
const PAD: usize = 16;
const CONTENT: usize = CELL - 2 * PAD;
const COLUMNS: usize = 8;
const MIPS: usize = 5;
const SAMPLES: usize = 2;
const TERRAINS: [Terrain; 10] = [
    Terrain::Grassland,
    Terrain::Plains,
    Terrain::Desert,
    Terrain::Tundra,
    Terrain::Snow,
    Terrain::Marsh,
    Terrain::Mountains,
    Terrain::Coast,
    Terrain::Ocean,
    Terrain::Lake,
];
const FEATURES: [Option<Feature>; 3] = [None, Some(Feature::Forest), Some(Feature::Jungle)];

struct Sprites {
    atlas: Atlas,
    quads: Vec<Vec<Vertex>>,
}

static SPRITES: LazyLock<Sprites> = LazyLock::new(build);

pub fn atlas() -> &'static Atlas {
    &SPRITES.atlas
}

pub(super) fn push_icon(center: Vec2, icon: MapIcon, scale: f32, out: &mut Vec<Vertex>) {
    mesh::place(&SPRITES.quads[icon as usize], center, scale, None, out);
}

fn tile_index(tile: Tile) -> usize {
    MapIcon::ALL.len()
        + tile.terrain as usize * 6
        + usize::from(tile.hills) * 3
        + tile.feature.map_or(0, |f| f as usize + 1)
}

pub(super) fn push_tile(center: Vec2, tile: Tile, out: &mut Vec<Vertex>) {
    mesh::place(&SPRITES.quads[tile_index(tile)], center, 1.0, None, out);
}

fn build() -> Sprites {
    let mut shapes = Vec::new();
    for icon in MapIcon::ALL {
        assert_eq!(icon as usize, shapes.len());
        let mut shape = Vec::new();
        map_icons::build_map_icon(icon, &mut shape);
        shapes.push(shape);
    }
    for terrain in TERRAINS {
        for hills in [false, true] {
            for feature in FEATURES {
                let mut shape = Vec::new();
                let tile = Tile {
                    terrain,
                    hills,
                    feature,
                };
                assert_eq!(tile_index(tile), shapes.len());
                draw::push_tile_symbols(Vec2::ZERO, tile, &mut shape);
                shapes.push(shape);
            }
        }
    }
    let width = COLUMNS * CELL;
    let height = shapes.len().div_ceil(COLUMNS) * CELL;
    let mut pixels = vec![0; width * height * 4];
    let mut quads = Vec::new();
    for (index, shape) in shapes.iter().enumerate() {
        if shape.is_empty() {
            quads.push(Vec::new());
            continue;
        }
        let (min, max) = bounds(shape);
        let scale = CONTENT as f32 / (max - min).max_element();
        let origin = min - Vec2::splat(PAD as f32 / scale);
        let cell = rasterize(shape, origin, scale);
        let x = index % COLUMNS * CELL;
        let y = index / COLUMNS * CELL;
        for row in 0..CELL {
            let dst = ((y + row) * width + x) * 4;
            pixels[dst..dst + CELL * 4]
                .copy_from_slice(&cell[row * CELL * 4..(row + 1) * CELL * 4]);
        }
        // Retain the transparent gutter in geometry, including the filtering
        // footprint at the coarsest mip. No neighbouring sprite can bleed in.
        let size = Vec2::splat(CELL as f32 / scale);
        let uv_min = Vec2::new(x as f32 / width as f32 + 2.0, y as f32 / height as f32);
        let uv_size = Vec2::new(CELL as f32 / width as f32, CELL as f32 / height as f32);
        let corners = [
            Vec2::ZERO,
            Vec2::X,
            Vec2::ONE,
            Vec2::ZERO,
            Vec2::ONE,
            Vec2::Y,
        ];
        quads.push(
            corners
                .into_iter()
                .map(|p| {
                    let at = origin + p * size;
                    Vertex {
                        pos: [at.x, at.y, 0.0],
                        color: [1.0; 4],
                        uv: (uv_min + p * uv_size).to_array(),
                    }
                })
                .collect(),
        );
    }
    Sprites {
        atlas: Atlas {
            channels: 4,
            width: width as u32,
            height: height as u32,
            levels: mip_chain(pixels, width, height),
        },
        quads,
    }
}

fn bounds(shape: &[Vertex]) -> (Vec2, Vec2) {
    shape.iter().fold(
        (Vec2::splat(f32::INFINITY), Vec2::splat(f32::NEG_INFINITY)),
        |(min, max), v| {
            let p = Vec2::new(v.pos[0], v.pos[1]);
            (min.min(p), max.max(p))
        },
    )
}

/// Supersample opaque vector artwork in painter order. Store premultiplied
/// linear color so both downsampling and GPU filtering preserve edge colors.
fn rasterize(shape: &[Vertex], origin: Vec2, scale: f32) -> Vec<u8> {
    let side = CELL * SAMPLES;
    let mut samples = vec![[0.0f32; 4]; side * side];
    for triangle in shape.as_chunks::<3>().0 {
        let [a, b, c] =
            triangle.map(|v| (Vec2::new(v.pos[0], v.pos[1]) - origin) * scale * SAMPLES as f32);
        let area = (b - a).perp_dot(c - a);
        if area.abs() < 1e-8 {
            continue;
        }
        // All source shapes are flat and opaque; writing a shared triangle
        // edge twice therefore cannot darken it.
        assert!(
            triangle
                .iter()
                .all(|v| v.color == triangle[0].color && v.color[3] == 1.0)
        );
        let min = a.min(b).min(c).floor().max(Vec2::ZERO);
        let max = a.max(b).max(c).ceil().min(Vec2::splat(side as f32));
        for y in min.y as usize..max.y as usize {
            for x in min.x as usize..max.x as usize {
                let p = Vec2::new(x as f32 + 0.5, y as f32 + 0.5);
                let u = (b - p).perp_dot(c - p) / area;
                let v = (c - p).perp_dot(a - p) / area;
                if u >= 0.0 && v >= 0.0 && u + v <= 1.0 {
                    samples[y * side + x] = triangle[0].color;
                }
            }
        }
    }
    let mut pixels = vec![0; CELL * CELL * 4];
    for y in 0..CELL {
        for x in 0..CELL {
            for channel in 0..4 {
                let mut sum = 0.0;
                for dy in 0..SAMPLES {
                    for dx in 0..SAMPLES {
                        sum += samples[(y * SAMPLES + dy) * side + x * SAMPLES + dx][channel];
                    }
                }
                pixels[(y * CELL + x) * 4 + channel] =
                    (sum * 255.0 / (SAMPLES * SAMPLES) as f32).round() as u8;
            }
        }
    }
    pixels
}

fn mip_chain(pixels: Vec<u8>, mut width: usize, mut height: usize) -> Vec<Vec<u8>> {
    let mut levels = vec![pixels];
    for _ in 1..MIPS {
        let previous = levels.last().unwrap();
        let (w, h) = (width / 2, height / 2);
        let mut next = vec![0; w * h * 4];
        for y in 0..h {
            for x in 0..w {
                for channel in 0..4 {
                    let at = |dx, dy| {
                        u32::from(previous[((y * 2 + dy) * width + x * 2 + dx) * 4 + channel])
                    };
                    next[(y * w + x) * 4 + channel] =
                        ((at(0, 0) + at(1, 0) + at(0, 1) + at(1, 1) + 2) / 4) as u8;
                }
            }
        }
        levels.push(next);
        (width, height) = (w, h);
    }
    levels
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sprite_quads_cover_original_art_and_use_padded_cells() {
        for icon in MapIcon::ALL {
            let mut original = Vec::new();
            map_icons::build_map_icon(icon, &mut original);
            let quad = &SPRITES.quads[icon as usize];
            assert_eq!(quad.len(), 6);
            let (min, max) = bounds(quad);
            let (art_min, art_max) = bounds(&original);
            assert!(min.cmplt(art_min).all() && max.cmpgt(art_max).all());
            assert!(
                quad.iter()
                    .all(|v| v.uv[0] >= 2.0 && v.uv[0] <= 3.0 && v.uv[1] >= 0.0 && v.uv[1] <= 1.0)
            );
        }
        for (level, pixels) in atlas().levels.iter().enumerate() {
            let width = atlas().width as usize >> level;
            let cell = CELL >> level;
            assert_eq!(pixels.len(), width * (atlas().height as usize >> level) * 4);
            // Every cell retains a transparent edge at every mip level.
            for (i, pixel) in pixels.as_chunks::<4>().0.iter().enumerate() {
                let (x, y) = (i % width, i / width);
                if x % cell == 0 || x % cell == cell - 1 || y % cell == 0 || y % cell == cell - 1 {
                    assert_eq!(pixel[3], 0, "gutter contaminated at mip {level}, {x},{y}");
                }
            }
        }
    }

    #[test]
    fn raster_preserves_painter_order_and_linear_edge_colors() {
        let mut shape = Vec::new();
        mesh::quad(
            Vec2::splat(16.5),
            Vec2::splat(32.5),
            [0.8, 0.4, 0.2, 1.0],
            &mut shape,
        );
        mesh::quad(
            Vec2::splat(20.0),
            Vec2::splat(28.0),
            [0.1, 0.2, 0.3, 1.0],
            &mut shape,
        );
        let pixels = rasterize(&shape, Vec2::ZERO, 1.0);
        let at = |x, y| &pixels[(y * CELL + x) * 4..(y * CELL + x) * 4 + 4];
        assert_eq!(at(24, 24), &[26, 51, 77, 255]);
        assert_eq!(at(16, 18), &[102, 51, 26, 128]);
        assert_eq!(at(0, 0), &[0, 0, 0, 0]);
    }
}
