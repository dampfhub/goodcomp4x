//! Text drawn with an embedded TrueType font (Hack). Every glyph is rasterized
//! once, on first use, into a coverage atlas that the renderer samples. UI
//! text uses glyphs rasterized at its exact pixel size so it stays crisp;
//! world text scales one large set, with mipmaps for when it's drawn small.

use std::collections::HashMap;
use std::sync::LazyLock;

use glam::Vec2;

use crate::renderer::{Atlas, Vertex};

type Color = [f32; 4];

/// Pixel sizes UI text can be drawn at.
pub const UI_SIZES: [u32; 3] = [15, 18, 22];
/// Pixel size of the glyphs that world text scales.
const WORLD_SIZE: u32 = 64;

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
            .filter_map(|ch| self.glyph(ch))
            .map(|glyph| glyph.advance)
            .sum()
    }

    /// Draws `text` at this face's own size in screen space, with its
    /// baseline's left end at `origin`. Glyphs snap to whole pixels.
    pub fn push(&self, origin: Vec2, text: &str, color: Color, out: &mut Vec<Vertex>) {
        let origin = origin.round();
        let mut pen = 0.0;
        for glyph in text.chars().filter_map(|ch| self.glyph(ch)) {
            let min = Vec2::new((origin.x + pen).round(), origin.y) + glyph.offset;
            push_glyph_quad(min, min + glyph.size, glyph, color, out);
            pen += glyph.advance;
        }
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

/// Draws a line of world-space text with capital letters `cap_height` tall
/// and its baseline's left end at `origin`.
pub fn push_text(origin: Vec2, cap_height: f32, text: &str, color: Color, out: &mut Vec<Vertex>) {
    let face = &FONT.world;
    let scale = cap_height / face.cap_height;
    let mut pen = 0.0;
    for glyph in text.chars().filter_map(|ch| face.glyph(ch)) {
        let min = origin + (Vec2::new(pen, 0.0) + glyph.offset) * scale;
        push_glyph_quad(min, min + glyph.size * scale, glyph, color, out);
        pen += glyph.advance;
    }
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

/// A glyph rasterized and placed in the atlas, before the atlas's final
/// size (and so its UV coordinates) is known.
struct Placed {
    ch: char,
    metrics: fontdue::Metrics,
    x: usize,
    y: usize,
    bitmap: Vec<u8>,
}

fn build() -> Font {
    let font = fontdue::Font::from_bytes(
        epaint_default_fonts::HACK_REGULAR,
        fontdue::FontSettings::default(),
    )
    .expect("the embedded font parses");

    let mut packer = Packer::new();
    let faces: Vec<(u32, Vec<Placed>)> = UI_SIZES
        .iter()
        .chain([&WORLD_SIZE])
        .map(|&px| {
            let placed = characters()
                .map(|ch| {
                    let (metrics, bitmap) = font.rasterize(ch, px as f32);
                    let (x, y) = packer.place(metrics.width, metrics.height);
                    Placed {
                        ch,
                        metrics,
                        x,
                        y,
                        bitmap,
                    }
                })
                .collect();
            (px, placed)
        })
        .collect();

    let (width, height) = (ATLAS_WIDTH, packer.height());
    let mut pixels = vec![0u8; width * height];
    for glyph in faces.iter().flat_map(|(_, placed)| placed) {
        let w = glyph.metrics.width;
        for (row, src) in glyph.bitmap.chunks_exact(w.max(1)).enumerate() {
            let start = (glyph.y + row) * width + glyph.x;
            pixels[start..start + w].copy_from_slice(src);
        }
    }

    let atlas_size = Vec2::new(width as f32, height as f32);
    let mut faces: Vec<(u32, Face)> = faces
        .into_iter()
        .map(|(px, placed)| {
            let line = font
                .horizontal_line_metrics(px as f32)
                .expect("the font has horizontal metrics");
            let glyphs = placed
                .into_iter()
                .map(|p| {
                    let min = Vec2::new(p.x as f32, p.y as f32);
                    let size = Vec2::new(p.metrics.width as f32, p.metrics.height as f32);
                    let glyph = Glyph {
                        uv_min: min / atlas_size,
                        uv_max: (min + size) / atlas_size,
                        offset: Vec2::new(p.metrics.xmin as f32, p.metrics.ymin as f32),
                        size,
                        advance: p.metrics.advance_width,
                    };
                    (p.ch, glyph)
                })
                .collect();
            let face = Face {
                glyphs,
                cap_height: font.metrics('H', px as f32).height as f32,
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
    fn every_ui_size_has_a_face() {
        for px in UI_SIZES {
            let face = ui(px);
            assert!(face.cap_height > 0.0 && face.line_height > face.cap_height);
            assert!(face.width("MOVE") > face.width("M"));
        }
    }
}
