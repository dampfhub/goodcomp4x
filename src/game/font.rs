//! Text drawn with an embedded TrueType font (IBM Plex Mono SemiBold, SIL Open
//! Font License, see `assets/fonts/OFL.txt`). Every glyph is rasterized once,
//! on first use, into an atlas that the renderer samples. UI text uses
//! coverage glyphs rasterized at its exact pixel size so it stays crisp;
//! world text scales one set of signed distance fields, which keep a sharp
//! outline at any zoom, with mipmaps for when it's drawn small.

use std::sync::LazyLock;

use glam::Vec2;

use super::fast_hash::HashMap;
use super::map_icons;
use crate::renderer::{Atlas, Vertex};

type Color = [f32; 4];

/// Pixel sizes UI text can be drawn at.
pub const UI_SIZES: [u32; 3] = [15, 18, 22];
/// Pixel size of the glyphs that world text scales.
const WORLD_SIZE: u32 = 64;
/// Distance fields are measured on a raster this many times finer.
const FIELD_SUPERSAMPLE: usize = 4;
/// How many pixels a distance field reaches outside and inside the outline.
const FIELD_SPREAD: usize = 6;

const FONT_DATA: &[u8] = include_bytes!("../../assets/fonts/IBMPlexMono-SemiBold.ttf");

const ATLAS_WIDTH: usize = 1024;
const MIP_LEVELS: usize = 4;
/// Empty space around each glyph, enough that even the smallest mip level
/// doesn't blend neighboring glyphs together.
const PADDING: usize = 1 << (MIP_LEVELS - 1);

/// Printable ASCII, plus a little punctuation for UI text.
fn characters() -> impl Iterator<Item = char> {
    (' '..='~').chain(['·', '—', '×', '…'])
}

struct Glyph {
    /// The glyph's rectangle in the atlas. `uv_min` is the bitmap's top-left.
    uv_min: Vec2,
    uv_max: Vec2,
    /// Bitmap bottom-left relative to the pen on the baseline, Y up, in pixels.
    offset: Vec2,
    size: Vec2,
    advance: f32,
}

/// The font rasterized at one pixel size.
pub struct Face {
    glyphs: HashMap<char, Glyph>,
    /// Height of capital letters above the baseline, in pixels.
    pub cap_height: f32,
    /// Distance between the baselines of consecutive lines, in pixels.
    pub line_height: f32,
}

struct Font {
    ui: Vec<(u32, Face)>,
    world: Face,
    atlas: Atlas,
}

static FONT: LazyLock<Font> = LazyLock::new(build);

/// Every rasterized glyph, for the renderer to upload.
pub fn atlas() -> &'static Atlas {
    &FONT.atlas
}

/// The face for UI text `px` pixels tall, which must be one of `UI_SIZES`.
pub fn ui(px: u32) -> &'static Face {
    FONT.ui
        .iter()
        .find(|(size, _)| *size == px)
        .map(|(_, face)| face)
        .unwrap_or_else(|| panic!("{px}px is not one of the UI font sizes"))
}

impl Face {
    /// Horizontal space `text` takes up.
    pub fn width(&self, text: &str) -> f32 {
        text.chars()
            .map(|ch| {
                if map_icons::inline_icon(ch).is_some() {
                    self.icon_advance()
                } else {
                    self.glyph(ch).map_or(0.0, |glyph| glyph.advance)
                }
            })
            .sum()
    }

    /// Draws `text` at this face's own size in screen space, with its
    /// baseline's left end at `origin`. Glyphs snap to whole pixels. Icon
    /// characters (`map_icons::inline_icon`) draw their icon, dimmed in dim
    /// text.
    pub fn push(&self, origin: Vec2, text: &str, color: Color, out: &mut Vec<Vertex>) {
        let origin = origin.round();
        let mut pen = 0.0;
        let dim = color[..3].iter().all(|&c| c < 0.25);
        for ch in text.chars() {
            if map_icons::inline_icon(ch).is_some() {
                let advance = self.icon_advance();
                let center = origin + Vec2::new(pen + advance / 2.0, self.cap_height / 2.0);
                map_icons::push_inline_icon(center, self.icon_height(), ch, dim, out);
                pen += advance;
                continue;
            }
            let Some(glyph) = self.glyph(ch) else {
                continue;
            };
            let min = Vec2::new((origin.x + pen).round(), origin.y) + glyph.offset;
            push_glyph_quad(min, min + glyph.size, glyph, color, out);
            pen += glyph.advance;
        }
    }

    /// How tall an inline icon draws: a little taller than capital letters.
    pub fn icon_height(&self) -> f32 {
        self.cap_height * 1.45
    }

    /// The room an inline icon takes in a line.
    fn icon_advance(&self) -> f32 {
        self.icon_height() * 1.05
    }

    /// Characters the font lacks draw as '?'.
    fn glyph(&self, ch: char) -> Option<&Glyph> {
        self.glyphs.get(&ch).or_else(|| self.glyphs.get(&'?'))
    }
}

/// Draws `ch` in world space with capital letters `cap_height` tall, its ink
/// centered horizontally on `center` and its capital-letter box vertically.
pub fn push_glyph(center: Vec2, cap_height: f32, ch: char, color: Color, out: &mut Vec<Vertex>) {
    let face = &FONT.world;
    let Some(glyph) = face.glyph(ch) else { return };
    let scale = cap_height / face.cap_height;
    let min = Vec2::new(
        center.x - glyph.size.x * scale / 2.0,
        center.y - cap_height / 2.0 + glyph.offset.y * scale,
    );
    push_glyph_quad(min, min + glyph.size * scale, glyph, color, out);
}

/// Width of world-space `text` with capital letters `cap_height` tall.
pub fn world_text_width(text: &str, cap_height: f32) -> f32 {
    let face = &FONT.world;
    face.width(text) * cap_height / face.cap_height
}

/// How tall an inline icon draws in world text with capital letters
/// `cap_height` tall, and the room it takes: in step with UI text
/// (`Face::icon_height`).
fn world_icon_height(cap_height: f32) -> f32 {
    let face = &FONT.world;
    face.icon_height() * cap_height / face.cap_height
}

/// Draws a line of world-space text with capital letters `cap_height` tall
/// and its baseline's left end at `origin`.
pub fn push_text(origin: Vec2, cap_height: f32, text: &str, color: Color, out: &mut Vec<Vertex>) {
    let face = &FONT.world;
    let scale = cap_height / face.cap_height;
    let mut pen = 0.0;
    for ch in text.chars() {
        // Icon characters draw their icon, as in UI text.
        if map_icons::inline_icon(ch).is_some() {
            let advance = face.icon_advance();
            let center = origin + Vec2::new(pen + advance / 2.0, face.cap_height / 2.0) * scale;
            map_icons::push_inline_icon(center, world_icon_height(cap_height), ch, false, out);
            pen += advance;
            continue;
        }
        let Some(glyph) = face.glyph(ch) else {
            continue;
        };
        let min = origin + (Vec2::new(pen, 0.0) + glyph.offset) * scale;
        push_glyph_quad(min, min + glyph.size * scale, glyph, color, out);
        pen += glyph.advance;
    }
}

/// Draws a line of world-space text with capital letters `cap_height` tall,
/// centered on `center`: horizontally by its advance width, vertically by its
/// capital-letter box (digits are capital height too).
pub fn push_text_centered(
    center: Vec2,
    cap_height: f32,
    text: &str,
    color: Color,
    out: &mut Vec<Vertex>,
) {
    let width = world_text_width(text, cap_height);
    let origin = center - Vec2::new(width, cap_height) / 2.0;
    push_text(origin, cap_height, text, color, out);
}

fn push_glyph_quad(min: Vec2, max: Vec2, glyph: &Glyph, color: Color, out: &mut Vec<Vertex>) {
    if glyph.size.x == 0.0 {
        return;
    }
    // Bitmap rows run top to bottom, so the quad's bottom edge samples the
    // bitmap's last row.
    let corner = |x: f32, y: f32, u: f32, v: f32| Vertex {
        pos: [x, y, 0.0],
        color,
        uv: [u, v],
    };
    let bottom_left = corner(min.x, min.y, glyph.uv_min.x, glyph.uv_max.y);
    let bottom_right = corner(max.x, min.y, glyph.uv_max.x, glyph.uv_max.y);
    let top_right = corner(max.x, max.y, glyph.uv_max.x, glyph.uv_min.y);
    let top_left = corner(min.x, max.y, glyph.uv_min.x, glyph.uv_min.y);
    out.extend([
        bottom_left,
        bottom_right,
        top_right,
        bottom_left,
        top_right,
        top_left,
    ]);
}

/// A glyph's bitmap and where it sits relative to the pen, in pixels of the
/// size its face is drawn at.
struct Raster {
    width: usize,
    height: usize,
    /// Rows top to bottom.
    bitmap: Vec<u8>,
    /// Bitmap bottom-left relative to the pen on the baseline, Y up.
    offset: Vec2,
    advance: f32,
}

/// A glyph rasterized and placed in the atlas, before the atlas's final
/// size (and so its UV coordinates) is known.
struct Placed {
    ch: char,
    raster: Raster,
    x: usize,
    y: usize,
}

fn build() -> Font {
    let font = fontdue::Font::from_bytes(FONT_DATA, fontdue::FontSettings::default())
        .expect("the embedded font parses");

    // UI faces hold coverage; the world face, a distance field.
    let mut packer = Packer::new();
    let mut place = |px: u32, field: bool| {
        let placed = characters()
            .map(|ch| {
                let raster = if field {
                    distance_field(&font, ch, px)
                } else {
                    coverage(&font, ch, px)
                };
                let (x, y) = packer.place(raster.width, raster.height);
                Placed { ch, raster, x, y }
            })
            .collect::<Vec<_>>();
        (px, field, placed)
    };
    let mut faces: Vec<(u32, bool, Vec<Placed>)> =
        UI_SIZES.iter().map(|&px| place(px, false)).collect();
    faces.push(place(WORLD_SIZE, true));

    let (width, height) = (ATLAS_WIDTH, packer.height());
    let mut pixels = vec![0u8; width * height];
    for glyph in faces.iter().flat_map(|(_, _, placed)| placed) {
        let w = glyph.raster.width;
        for (row, src) in glyph.raster.bitmap.chunks_exact(w.max(1)).enumerate() {
            let start = (glyph.y + row) * width + glyph.x;
            pixels[start..start + w].copy_from_slice(src);
        }
    }

    let atlas_size = Vec2::new(width as f32, height as f32);
    let mut faces: Vec<(u32, Face)> = faces
        .into_iter()
        .map(|(px, field, placed)| {
            let line = font
                .horizontal_line_metrics(px as f32)
                .expect("the font has horizontal metrics");
            // The shader tells distance-field glyphs apart by u shifted up by 1.
            let shift = if field { Vec2::X } else { Vec2::ZERO };
            let glyphs = placed
                .into_iter()
                .map(|p| {
                    let min = Vec2::new(p.x as f32, p.y as f32);
                    let size = Vec2::new(p.raster.width as f32, p.raster.height as f32);
                    let glyph = Glyph {
                        uv_min: min / atlas_size + shift,
                        uv_max: (min + size) / atlas_size + shift,
                        offset: p.raster.offset,
                        size,
                        advance: p.raster.advance,
                    };
                    (p.ch, glyph)
                })
                .collect();
            let capital = font.metrics('H', px as f32);
            let face = Face {
                glyphs,
                // Scaled world text wants the exact height; pixel-snapped UI
                // text, the bitmap's.
                cap_height: if field {
                    capital.bounds.height
                } else {
                    capital.height as f32
                },
                line_height: line.new_line_size,
            };
            (px, face)
        })
        .collect();
    let (_, world) = faces.pop().expect("the world face was built last");

    Font {
        ui: faces,
        world,
        atlas: Atlas {
            width: width as u32,
            height: height as u32,
            levels: mip_chain(pixels, width, height),
        },
    }
}

/// `ch` as plain coverage at `px`, for drawing at exactly that size.
fn coverage(font: &fontdue::Font, ch: char, px: u32) -> Raster {
    let (metrics, bitmap) = font.rasterize(ch, px as f32);
    Raster {
        width: metrics.width,
        height: metrics.height,
        bitmap,
        offset: Vec2::new(metrics.xmin as f32, metrics.ymin as f32),
        advance: metrics.advance_width,
    }
}

/// `ch` as a signed distance field at `px`: each pixel holds how far its
/// center is from the outline, 0.5 on it, rising inside and falling outside
/// to reach 1 or 0 `FIELD_SPREAD` pixels away. The shader thresholds it at
/// 0.5, so the outline stays sharp however far the glyph is scaled up.
fn distance_field(font: &fontdue::Font, ch: char, px: u32) -> Raster {
    let ss = FIELD_SUPERSAMPLE;
    // Measure distances on a finer raster, then average them down.
    let (metrics, fine) = font.rasterize(ch, (px as usize * ss) as f32);
    let advance = metrics.advance_width / ss as f32;
    if metrics.width == 0 || metrics.height == 0 {
        return Raster {
            width: 0,
            height: 0,
            bitmap: Vec::new(),
            offset: Vec2::ZERO,
            advance,
        };
    }

    // The field reaches past the ink, so pad the raster by the spread.
    let pad = FIELD_SPREAD * ss;
    let fine_width = (metrics.width + 2 * pad).next_multiple_of(ss);
    let fine_height = (metrics.height + 2 * pad).next_multiple_of(ss);
    let inside: Vec<bool> = (0..fine_width * fine_height)
        .map(|i| {
            let (x, y) = (
                (i % fine_width).wrapping_sub(pad),
                (i / fine_width).wrapping_sub(pad),
            );
            x < metrics.width && y < metrics.height && fine[y * metrics.width + x] >= 128
        })
        .collect();
    let outside: Vec<bool> = inside.iter().map(|&i| !i).collect();
    let to_inside = squared_distances(&inside, fine_width, fine_height);
    let to_outside = squared_distances(&outside, fine_width, fine_height);
    // Positive inside. Pixel centers sit half a pixel from the outline at best.
    let signed: Vec<f32> = (0..inside.len())
        .map(|i| {
            if inside[i] {
                to_outside[i].sqrt() as f32 - 0.5
            } else {
                0.5 - to_inside[i].sqrt() as f32
            }
        })
        .collect();

    let (width, height) = (fine_width / ss, fine_height / ss);
    let range = (2 * FIELD_SPREAD * ss) as f32;
    let bitmap = (0..width * height)
        .map(|j| {
            let (x, y) = (j % width * ss, j / width * ss);
            let sum: f32 = (0..ss)
                .flat_map(|dy| (0..ss).map(move |dx| (dx, dy)))
                .map(|(dx, dy)| signed[(y + dy) * fine_width + x + dx])
                .sum();
            let mean = sum / (ss * ss) as f32;
            ((0.5 + mean / range).clamp(0.0, 1.0) * 255.0).round() as u8
        })
        .collect();

    // The padded raster's bottom row lies below the ink's by the padding
    // plus whatever rounding up to whole pixels added.
    let below = fine_height - pad - metrics.height;
    let offset = Vec2::new(
        metrics.xmin as f32 - pad as f32,
        metrics.ymin as f32 - below as f32,
    );
    Raster {
        width,
        height,
        bitmap,
        offset: offset / ss as f32,
        advance,
    }
}

/// The squared distance from each pixel to the nearest one marked in
/// `target`: the exact Euclidean distance transform of Felzenszwalb and
/// Huttenlocher, one dimension at a time.
fn squared_distances(target: &[bool], width: usize, height: usize) -> Vec<f64> {
    const FAR: f64 = 1e20;
    let mut grid: Vec<f64> = target.iter().map(|&t| if t { 0.0 } else { FAR }).collect();
    let mut line = vec![0.0; width.max(height)];
    let mut out = vec![0.0; width.max(height)];
    for x in 0..width {
        for y in 0..height {
            line[y] = grid[y * width + x];
        }
        transform_line(&line[..height], &mut out[..height]);
        for y in 0..height {
            grid[y * width + x] = out[y];
        }
    }
    for row in grid.chunks_exact_mut(width) {
        transform_line(row, &mut out[..width]);
        row.copy_from_slice(&out[..width]);
    }
    grid
}

/// `out[i]` = the least `f[j] + (i - j)²` over all `j`: the lower envelope
/// of parabolas rooted at each `f[j]`.
fn transform_line(f: &[f64], out: &mut [f64]) {
    let n = f.len();
    // Roots of the parabolas on the envelope, and where each takes over.
    let mut roots = vec![0usize; n];
    let mut starts = vec![0.0f64; n + 1];
    let mut k = 0;
    starts[0] = f64::NEG_INFINITY;
    starts[1] = f64::INFINITY;
    for q in 1..n {
        let meet = |p: usize| {
            let (p2, q2) = ((p * p) as f64, (q * q) as f64);
            ((f[q] + q2) - (f[p] + p2)) / (2.0 * (q - p) as f64)
        };
        let mut s = meet(roots[k]);
        while s <= starts[k] {
            k -= 1;
            s = meet(roots[k]);
        }
        k += 1;
        roots[k] = q;
        starts[k] = s;
        starts[k + 1] = f64::INFINITY;
    }
    k = 0;
    for (q, value) in out.iter_mut().enumerate() {
        while starts[k + 1] < q as f64 {
            k += 1;
        }
        let d = q as f64 - roots[k] as f64;
        *value = d * d + f[roots[k]];
    }
}

/// Places rectangles left to right in rows, starting a new row once one fills.
struct Packer {
    x: usize,
    y: usize,
    row_height: usize,
}

impl Packer {
    fn new() -> Self {
        Self {
            x: PADDING,
            y: PADDING,
            row_height: 0,
        }
    }

    fn place(&mut self, width: usize, height: usize) -> (usize, usize) {
        if self.x + width + PADDING > ATLAS_WIDTH {
            self.y += self.row_height + PADDING;
            self.x = PADDING;
            self.row_height = 0;
        }
        let pos = (self.x, self.y);
        self.x += width + PADDING;
        self.row_height = self.row_height.max(height);
        pos
    }

    /// Atlas height needed so far, rounded so every mip level halves evenly.
    fn height(&self) -> usize {
        (self.y + self.row_height + PADDING).next_multiple_of(1 << (MIP_LEVELS - 1))
    }
}

/// `level0` followed by successively halved copies, each pixel the average
/// of the 2x2 block above it.
fn mip_chain(level0: Vec<u8>, width: usize, height: usize) -> Vec<Vec<u8>> {
    let mut levels = vec![level0];
    for i in 1..MIP_LEVELS {
        let above = &levels[i - 1];
        let above_width = width >> (i - 1);
        let (w, h) = (width >> i, height >> i);
        let level = (0..w * h)
            .map(|j| {
                let (x, y) = (j % w * 2, j / w * 2);
                let sum: u32 = [
                    above[y * above_width + x],
                    above[y * above_width + x + 1],
                    above[(y + 1) * above_width + x],
                    above[(y + 1) * above_width + x + 1],
                ]
                .into_iter()
                .map(u32::from)
                .sum();
                ((sum + 2) / 4) as u8
            })
            .collect();
        levels.push(level);
    }
    levels
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atlas_levels_halve_in_size() {
        let atlas = atlas();
        assert_eq!(atlas.levels.len(), MIP_LEVELS);
        for (i, level) in atlas.levels.iter().enumerate() {
            let expected = (atlas.width >> i) as usize * (atlas.height >> i) as usize;
            assert_eq!(level.len(), expected);
        }
    }

    #[test]
    fn the_font_has_every_character() {
        let font = fontdue::Font::from_bytes(FONT_DATA, fontdue::FontSettings::default()).unwrap();
        for ch in characters().filter(|ch| *ch != ' ') {
            assert_ne!(font.lookup_glyph_index(ch), 0, "missing {ch:?}");
        }
    }

    #[test]
    fn distance_field_rises_inside_the_outline() {
        let font = fontdue::Font::from_bytes(FONT_DATA, fontdue::FontSettings::default()).unwrap();
        let field = distance_field(&font, 'I', WORLD_SIZE);
        let at = |x: usize, y: usize| field.bitmap[y * field.width + x];
        let middle = (field.width / 2, field.height / 2);
        // Deep inside the stem, and near the spread's end in the padding.
        assert!(at(middle.0, middle.1) > 160);
        assert!(at(0, middle.1) < 20);
        assert!(at(field.width - 1, 0) < 20);
        // Somewhere across the row the field crosses the outline.
        let row: Vec<u8> = (0..field.width).map(|x| at(x, middle.1)).collect();
        assert!(row.windows(2).any(|w| w[0] < 128 && w[1] >= 128));
        // The ink sits where the coverage glyph's does.
        let plain = coverage(&font, 'I', WORLD_SIZE);
        let center = |r: &Raster| r.offset + Vec2::new(r.width as f32, r.height as f32) / 2.0;
        assert!((center(&field) - center(&plain)).length() < 1.0);
        assert!((field.advance - plain.advance).abs() < 0.5);
    }

    #[test]
    fn centered_text_centers_on_its_point() {
        let center = Vec2::new(2.0, -1.0);
        let cap = 0.3;
        for text in ["1", "7", "12", "100"] {
            let mut out = Vec::new();
            push_text_centered(center, cap, text, [1.0; 4], &mut out);
            let (min, max) = out.iter().fold(
                (Vec2::splat(f32::MAX), Vec2::splat(f32::MIN)),
                |(min, max), v| {
                    let p = Vec2::new(v.pos[0], v.pos[1]);
                    (min.min(p), max.max(p))
                },
            );
            // The quads include the distance field's padding, the same on
            // every side give or take a fraction of a pixel.
            let off = (min + max) / 2.0 - center;
            assert!(off.x.abs() < 0.08 * cap, "{text} is off by {off}");
            assert!(off.y.abs() < 0.08 * cap, "{text} is off by {off}");
        }
    }

    #[test]
    fn every_ui_size_has_a_face() {
        for px in UI_SIZES {
            let face = ui(px);
            assert!(face.cap_height > 0.0 && face.line_height > face.cap_height);
            assert!(face.width("MOVE") > face.width("M"));
        }
    }
}
#[cfg(test)]
mod measure_tmp {
    #[test]
    fn measure() {
        let font =
            fontdue::Font::from_bytes(super::FONT_DATA, fontdue::FontSettings::default()).unwrap();
        let cap = font.metrics('H', 256.0).bounds.height;
        for ch in "MRCSAHWBTXI".chars() {
            let m = font.metrics(ch, 256.0);
            println!("{ch} width/cap {:.3}", m.bounds.width / cap);
        }
    }
}
