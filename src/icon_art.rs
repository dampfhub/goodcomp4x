//! The game's icon, drawn in code rather than loaded from a file: a rusted
//! stop-sign plate with an amber gear on it and a green sprout growing up
//! out of the gear's hub, the salvage age's emblem. `icon.rs` hands it to the
//! window (title bar and taskbar), and `build.rs` includes this file to embed
//! it in the Windows executable as well (`windows_res`), which is where
//! Windows sometimes takes the taskbar icon from. So it uses nothing outside
//! `std`.

use std::f32::consts::TAU;

type Rgba = [u8; 4];

/// Colors as they appear on screen: a dark iron rim, a rust plate, a hazard
/// amber gear and a fresh green sprout.
const RIM: Rgba = [38, 31, 27, 255];
const PLATE: Rgba = [160, 70, 36, 255];
const GEAR: Rgba = [244, 170, 38, 255];
const SPROUT: Rgba = [170, 214, 92, 255];

/// Apothems, in the icon's [-1, 1] square, of the whole octagon and its
/// plate.
const RIM_APOTHEM: f32 = 0.97;
const PLATE_APOTHEM: f32 = 0.84;
/// The gear: its center, the radii of its hub hole, its body and its teeth's
/// tips, and its teeth's half width.
const GEAR_CENTER: (f32, f32) = (0.0, -0.16);
const GEAR_HOLE: f32 = 0.15;
const GEAR_ROOT: f32 = 0.40;
const GEAR_TIP: f32 = 0.53;
const GEAR_TEETH: u32 = 8;
const TOOTH_HALF_WIDTH: f32 = 0.1;
/// The sprout: its stem's half width and top, and its two leaves as
/// (center, half length, half width, tilt in degrees).
const STEM_HALF_WIDTH: f32 = 0.045;
const STEM_TOP: f32 = 0.5;
const LEAVES: [((f32, f32), f32, f32, f32); 2] = [
    ((-0.19, 0.5), 0.2, 0.09, -35.0),
    ((0.19, 0.6), 0.2, 0.09, 35.0),
];
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
    if in_sprout(p) {
        Some(SPROUT)
    } else if in_gear(p) {
        Some(GEAR)
    } else if in_octagon(p, PLATE_APOTHEM) {
        Some(PLATE)
    } else if in_octagon(p, RIM_APOTHEM) {
        Some(RIM)
    } else {
        None
    }
}

/// Inside a regular octagon centered on the origin with flat sides at the
/// top, bottom and sides, like a stop sign.
fn in_octagon((x, y): (f32, f32), apothem: f32) -> bool {
    let (x, y) = (x.abs(), y.abs());
    x <= apothem && y <= apothem && x + y <= apothem * 2f32.sqrt()
}

/// On the gear: its ring, or one of its teeth, which sit either side of the
/// top so the sprout's stem rises between two of them.
fn in_gear((x, y): (f32, f32)) -> bool {
    let (dx, dy) = (x - GEAR_CENTER.0, y - GEAR_CENTER.1);
    let r = (dx * dx + dy * dy).sqrt();
    if !(GEAR_HOLE..=GEAR_TIP).contains(&r) {
        return false;
    }
    if r <= GEAR_ROOT {
        return true;
    }
    let pitch = TAU / GEAR_TEETH as f32;
    let angle = dy.atan2(dx) - pitch / 2.0;
    let off = (angle / pitch - (angle / pitch).round()).abs() * pitch;
    off * r <= TOOTH_HALF_WIDTH
}

/// On the sprout: its stem, from the gear's hub up, or a leaf.
fn in_sprout((x, y): (f32, f32)) -> bool {
    if x.abs() <= STEM_HALF_WIDTH && y >= GEAR_CENTER.1 && y <= STEM_TOP {
        return true;
    }
    LEAVES.iter().any(|&((cx, cy), length, width, tilt)| {
        let (sin, cos) = tilt.to_radians().sin_cos();
        let (dx, dy) = (x - cx, y - cy);
        let (along, across) = (dx * cos + dy * sin, -dx * sin + dy * cos);
        (along / length).powi(2) + (across / width).powi(2) <= 1.0
    })
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
    fn corners_are_transparent_and_the_emblem_is_drawn() {
        assert_eq!(pixel(0, 0, 32), [0; 4]);
        // Left of the hub, on the gear's body.
        assert_eq!(pixel(22, 37, 64), GEAR);
        // The stem, above the hub.
        assert_eq!(pixel(31, 20, 64), SPROUT);
        // Low on the plate, below the gear.
        assert_eq!(pixel(32, 55, 64), PLATE);
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
