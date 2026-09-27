//! The game's icon, drawn in code rather than loaded from a file: a blue hex
//! tile with a white triangle, like a melee unit on the map. `icon.rs` hands
//! it to the window (title bar and taskbar), and `build.rs` includes this file
//! to embed it in the Windows executable as well (`windows_res`), which is
//! where Windows sometimes takes the taskbar icon from. So it uses nothing
//! outside `std`.

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
const TRIANGLE: [(f32, f32); 3] = [(0.0, 0.46), (-0.42, -0.30), (0.42, -0.30)];
/// Samples per pixel along each axis, for smooth edges.
const SUPERSAMPLE: u32 = 4;

/// The icon rendered at `size` by `size` pixels, as RGBA rows top to bottom.
pub fn rgba(size: u32) -> Vec<u8> {
    let mut rgba = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            rgba.extend(pixel(x, y, size));
        }
    }
    rgba
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
            if let Some(sample) = shape_at((u * 2.0 - 1.0, 1.0 - v * 2.0)) {
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

fn shape_at(p: (f32, f32)) -> Option<Rgba> {
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
fn in_hex((x, y): (f32, f32), radius: f32) -> bool {
    let (x, y) = (x.abs(), y.abs());
    let sqrt3 = 3f32.sqrt();
    y <= radius * sqrt3 / 2.0 && sqrt3 * x + y <= sqrt3 * radius
}

fn in_triangle(p: (f32, f32)) -> bool {
    let [a, b, c] = TRIANGLE;
    // Which side of the edge from `from` to `to` the point is on.
    let side = |from: (f32, f32), to: (f32, f32)| {
        (to.0 - from.0) * (p.1 - from.1) - (to.1 - from.1) * (p.0 - from.0)
    };
    let (ab, bc, ca) = (side(a, b), side(b, c), side(c, a));
    (ab >= 0.0 && bc >= 0.0 && ca >= 0.0) || (ab <= 0.0 && bc <= 0.0 && ca <= 0.0)
}

/// Resource type ids in a Windows `.res` file.
const RT_ICON: u16 = 3;
const RT_GROUP_ICON: u16 = 14;
/// Moveable, pure and discardable, as resource compilers mark icons.
const ICON_MEMORY_FLAGS: u16 = 0x1030;
/// English (United States).
const LANGUAGE: u16 = 0x0409;

/// A compiled Windows resource file (`.res`) holding the icon at each of
/// `sizes` as the application's icon group (id 1), which the MSVC linker
/// embeds in the executable. Used by `build.rs`.
#[allow(dead_code)]
pub fn windows_res(sizes: &[u32]) -> Vec<u8> {
    let mut out = Vec::new();
    // Every .res file starts with an empty resource.
    push_resource(&mut out, 0, 0, 0, 0, &[]);
    let mut group = Vec::new();
    group.extend(0u16.to_le_bytes()); // reserved
    group.extend(1u16.to_le_bytes()); // icons (not cursors)
    group.extend((sizes.len() as u16).to_le_bytes());
    for (index, &size) in sizes.iter().enumerate() {
        let id = index as u16 + 1;
        let image = icon_bitmap(size);
        push_resource(&mut out, RT_ICON, id, ICON_MEMORY_FLAGS, LANGUAGE, &image);
        // Width and height of 256 are written as 0.
        let side = if size >= 256 { 0 } else { size as u8 };
        group.extend([side, side, 0, 0]); // colors in a palette, reserved
        group.extend(1u16.to_le_bytes()); // planes
        group.extend(32u16.to_le_bytes()); // bits per pixel
        group.extend((image.len() as u32).to_le_bytes());
        group.extend(id.to_le_bytes());
    }
    push_resource(
        &mut out,
        RT_GROUP_ICON,
        1,
        ICON_MEMORY_FLAGS,
        LANGUAGE,
        &group,
    );
    out
}

/// One resource: its header (type and name as numeric ids), then its data,
/// padded to a multiple of 4 bytes.
fn push_resource(out: &mut Vec<u8>, kind: u16, name: u16, flags: u16, language: u16, data: &[u8]) {
    out.extend((data.len() as u32).to_le_bytes());
    out.extend(32u32.to_le_bytes()); // header size
    out.extend([0xFF, 0xFF]);
    out.extend(kind.to_le_bytes());
    out.extend([0xFF, 0xFF]);
    out.extend(name.to_le_bytes());
    out.extend(0u32.to_le_bytes()); // data version
    out.extend(flags.to_le_bytes());
    out.extend(language.to_le_bytes());
    out.extend(0u32.to_le_bytes()); // version
    out.extend(0u32.to_le_bytes()); // characteristics
    out.extend(data);
    out.resize(out.len().next_multiple_of(4), 0);
}

/// The icon at `size` as an icon image: a 32-bit bitmap header, its pixels
/// bottom row first in BGRA, then the 1-bit mask of transparent pixels.
fn icon_bitmap(size: u32) -> Vec<u8> {
    let pixels = rgba(size);
    let mask_stride = size.div_ceil(32) * 4;
    let mut out = Vec::new();
    out.extend(40u32.to_le_bytes()); // header size
    out.extend((size as i32).to_le_bytes());
    out.extend((size as i32 * 2).to_le_bytes()); // image and mask, stacked
    out.extend(1u16.to_le_bytes()); // planes
    out.extend(32u16.to_le_bytes()); // bits per pixel
    out.extend(0u32.to_le_bytes()); // no compression
    out.extend((size * size * 4 + mask_stride * size).to_le_bytes());
    out.extend([0u8; 16]); // resolution and palette: unused
    for y in (0..size).rev() {
        for x in 0..size {
            let at = ((y * size + x) * 4) as usize;
            let [r, g, b, a] = [pixels[at], pixels[at + 1], pixels[at + 2], pixels[at + 3]];
            out.extend([b, g, r, a]);
        }
    }
    for y in (0..size).rev() {
        let mut row = vec![0u8; mask_stride as usize];
        for x in 0..size {
            if pixels[((y * size + x) * 4 + 3) as usize] == 0 {
                row[(x / 8) as usize] |= 0x80 >> (x % 8);
            }
        }
        out.extend(row);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corners_are_transparent_and_the_center_is_the_mark() {
        assert_eq!(pixel(0, 0, 32), [0; 4]);
        assert_eq!(pixel(16, 16, 32), MARK);
    }

    #[test]
    fn the_windows_resource_holds_one_icon_per_size_and_a_group() {
        let res = windows_res(&[16, 256]);
        let u32_at = |at: usize| u32::from_le_bytes(res[at..at + 4].try_into().unwrap());
        let u16_at = |at: usize| u16::from_le_bytes(res[at..at + 2].try_into().unwrap());
        // Walk the resources: the empty one, two icons, then the group.
        let mut at = 0;
        let mut kinds = Vec::new();
        while at < res.len() {
            let (data, header) = (u32_at(at) as usize, u32_at(at + 4) as usize);
            assert_eq!(header, 32);
            kinds.push((u16_at(at + 10), u16_at(at + 14), data));
            at = (at + header + data).next_multiple_of(4);
        }
        assert_eq!(at, res.len());
        let icon_16 = 40 + 16 * 16 * 4 + 4 * 16;
        let icon_256 = 40 + 256 * 256 * 4 + 32 * 256;
        assert_eq!(
            kinds,
            [
                (0, 0, 0),
                (RT_ICON, 1, icon_16),
                (RT_ICON, 2, icon_256),
                (RT_GROUP_ICON, 1, 6 + 2 * 14),
            ]
        );
    }
}
