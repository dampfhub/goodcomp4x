//! Pixel-art sprites (`docs/sprites.md`): PNG files named by a tag, such as
//! `terrain/grassland` or `unit/pikeman`, that the map draws instead of its
//! vector art when the Art setting is Pixel (`Settings::pixel_art`). Each is
//! loaded once, at startup: from an `assets/sprites/` folder in the working
//! directory or beside the executable if there is one, otherwise from the
//! copies compiled in (`build.rs` embeds whatever the repository's folder
//! holds). A tag with no file falls back to the vector art, so sprites can
//! come one at a time.
//!
//! Every sprite is drawn in a box of its tag's size times `TEXEL`, centered
//! on where its vector art would be, so all of them share one texel size.
//! A PNG of another size is scaled into that box. Texels are read one by one
//! (no filtering, no mips: `mesh.frag`), and pure magenta, in any shade, is
//! the team color: a unit's, city's or building's side color goes there.

use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use glam::Vec2;

use super::city::Building;
use super::draw::HEX_FILL_SCALE;
use super::fast_hash::HashMap;
use super::hex::HEX_SIZE;
use super::map_icons::MapIcon;
use super::mesh::Color;
use super::terrain::{Feature, Terrain};
use super::unit_icons::UnitIcon;
use crate::renderer::{Atlas, Vertex};

/// A terrain tile's size in texels: a flat-top hexagon 32 wide.
const TILE: (u32, u32) = (32, 28);
/// World units a texel spans: a terrain tile's width is a hex's fill.
pub(super) const TEXEL: f32 = 2.0 * HEX_SIZE * HEX_FILL_SCALE / TILE.0 as f32;
/// Where the game looks for sprite files, from the working directory and
/// from the executable's.
const FOLDER: &str = "assets/sprites";
/// What pixel-art texture coordinates are shifted by, so the shader tells
/// them from the others (`Vertex::uv`). Past the vector sprites' 2 to 3,
/// with a whole unit between, so the end of one range is never the other.
pub(super) const PIXEL_U: f32 = 4.0;

/// The shipped sprites, `(tag, PNG bytes)`, compiled in by `build.rs`.
const EMBEDDED: &[(&str, &[u8])] = include!(concat!(env!("OUT_DIR"), "/embedded_sprites.rs"));

/// Something the map can draw as a pixel sprite.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Sprite {
    /// A terrain tile, with hills or without.
    Ground(Terrain, bool),
    /// Forest or jungle, drawn over the ground.
    Feature(Feature),
    /// A unit's token, by its pictogram (a settler's flag, a worker's shovel).
    Unit(UnitIcon),
    City,
    Building(Building),
    Icon(MapIcon),
}

impl Sprite {
    /// Its file's name under `assets/sprites/`, without `.png`, or `None`
    /// for what never shows on the map (hills under water, an icon only
    /// text uses).
    pub(super) fn tag(self) -> Option<&'static str> {
        Some(match self {
            Sprite::Ground(terrain, false) => match terrain {
                Terrain::Grassland => "terrain/grassland",
                Terrain::Plains => "terrain/plains",
                Terrain::Desert => "terrain/desert",
                Terrain::Tundra => "terrain/tundra",
                Terrain::Snow => "terrain/snow",
                Terrain::Marsh => "terrain/marsh",
                Terrain::Mountains => "terrain/mountains",
                Terrain::Coast => "terrain/coast",
                Terrain::Ocean => "terrain/ocean",
                Terrain::Lake => "terrain/lake",
            },
            Sprite::Ground(terrain, true) => match terrain {
                Terrain::Grassland => "terrain/grassland_hills",
                Terrain::Plains => "terrain/plains_hills",
                Terrain::Desert => "terrain/desert_hills",
                Terrain::Tundra => "terrain/tundra_hills",
                Terrain::Snow => "terrain/snow_hills",
                Terrain::Marsh => "terrain/marsh_hills",
                Terrain::Mountains | Terrain::Coast | Terrain::Ocean | Terrain::Lake => {
                    return None;
                }
            },
            Sprite::Feature(Feature::Forest) => "feature/forest",
            Sprite::Feature(Feature::Jungle) => "feature/jungle",
            Sprite::Unit(icon) => match icon {
                UnitIcon::Sword => "unit/melee",
                UnitIcon::Bow => "unit/ranged",
                UnitIcon::HorseHead => "unit/cavalry",
                UnitIcon::Catapult => "unit/siege",
                UnitIcon::Spyglass => "unit/scout",
                UnitIcon::Shield => "unit/armored",
                UnitIcon::Pike => "unit/pikeman",
                UnitIcon::Galley => "unit/patrol_galley",
                UnitIcon::LandingCraft => "unit/landing_craft",
                UnitIcon::BombardShip => "unit/bombard_ship",
                UnitIcon::Flag => "unit/settler",
                UnitIcon::Shovel => "unit/worker",
                UnitIcon::WolfHead => "unit/wolf_pack",
                UnitIcon::BearHead => "unit/bear",
            },
            Sprite::City => "building/city",
            Sprite::Building(building) => match building {
                Building::Barracks => "building/barracks",
                Building::Mill => "building/mill",
                Building::Workshop => "building/workshop",
                Building::CanoeHouse => "building/canoe_house",
                Building::Forge => "building/forge",
                Building::Stable => "building/stable",
                Building::Watchpost => "building/watchpost",
                Building::FieldHospital => "building/field_hospital",
                Building::Cannery => "building/cannery",
                Building::WorkCamp => "building/work_camp",
                Building::Smelter => "building/smelter",
                Building::Railhead => "building/railhead",
                Building::Harbor => "building/harbor",
                Building::CoastalBattery => "building/coastal_battery",
            },
            Sprite::Icon(icon) => match icon {
                MapIcon::HorseHead => "icon/horse_head",
                MapIcon::Ingot => "icon/ingot",
                MapIcon::Wheat => "icon/wheat",
                MapIcon::OreCart => "icon/ore_cart",
                MapIcon::Fence => "icon/fence",
                MapIcon::Logs => "icon/logs",
                MapIcon::Ruins => "icon/ruins",
                MapIcon::FruitTree => "icon/fruit_tree",
                MapIcon::StoneBlocks => "icon/stone_blocks",
                MapIcon::Paw => "icon/paw",
                MapIcon::Food | MapIcon::Wood | MapIcon::Metal | MapIcon::Clock => return None,
            },
        })
    }

    /// Its size in texels: what its PNG should measure.
    pub(super) fn size(self) -> (u32, u32) {
        match self {
            Sprite::Ground(..) | Sprite::Feature(_) => TILE,
            Sprite::Unit(_) | Sprite::Building(_) => (14, 14),
            Sprite::City => (16, 16),
            Sprite::Icon(_) => (8, 8),
        }
    }

    /// Every sprite the map can draw, each with its own tag.
    pub(super) fn all() -> Vec<Sprite> {
        let terrains = [
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
        let units = [
            UnitIcon::Sword,
            UnitIcon::Bow,
            UnitIcon::HorseHead,
            UnitIcon::Catapult,
            UnitIcon::Spyglass,
            UnitIcon::Shield,
            UnitIcon::Pike,
            UnitIcon::Galley,
            UnitIcon::LandingCraft,
            UnitIcon::BombardShip,
            UnitIcon::Flag,
            UnitIcon::Shovel,
            UnitIcon::WolfHead,
            UnitIcon::BearHead,
        ];
        let grounds = [false, true]
            .into_iter()
            .flat_map(|hills| terrains.map(|t| Sprite::Ground(t, hills)));
        grounds
            .chain([Feature::Forest, Feature::Jungle].map(Sprite::Feature))
            .chain(units.map(Sprite::Unit))
            .chain([Sprite::City])
            .chain(Building::ALL.map(Sprite::Building))
            .chain(MapIcon::ALL.map(Sprite::Icon))
            .filter(|sprite| sprite.tag().is_some())
            .collect()
    }

    /// Terrain lines up on one texel grid across the map; things that move
    /// don't, so they glide smoothly.
    fn snaps(self) -> bool {
        matches!(self, Sprite::Ground(..) | Sprite::Feature(_))
    }
}

/// A sprite's place in the atlas, in texture coordinates.
#[derive(Clone, Copy, Debug)]
struct Placed {
    min: Vec2,
    max: Vec2,
}

struct PixelArt {
    atlas: Atlas,
    placed: HashMap<&'static str, Placed>,
}

static ART: LazyLock<PixelArt> = LazyLock::new(load);

/// The atlas of every pixel sprite found, for the renderer.
pub fn atlas() -> &'static Atlas {
    &ART.atlas
}

/// Whether `sprite` has a file.
pub(super) fn has(sprite: Sprite) -> bool {
    sprite.tag().is_some_and(|tag| ART.placed.contains_key(tag))
}

/// Draws `sprite` centered on `center`, `scale` times its usual size, its
/// team pixels in `color`'s RGB and all of it faded by `color`'s alpha.
/// Returns false, drawing nothing, if it has no file.
pub(super) fn push(
    sprite: Sprite,
    center: Vec2,
    scale: f32,
    color: Color,
    out: &mut Vec<Vertex>,
) -> bool {
    let Some(placed) = sprite.tag().and_then(|tag| ART.placed.get(tag)) else {
        return false;
    };
    let (w, h) = sprite.size();
    let size = Vec2::new(w as f32, h as f32) * TEXEL * scale;
    let mut min = center - size / 2.0;
    if sprite.snaps() {
        min = (min / TEXEL).round() * TEXEL;
    }
    // World Y is up and image rows go down: the bottom of the box takes
    // the bottom row of the image.
    for p in [
        Vec2::ZERO,
        Vec2::X,
        Vec2::ONE,
        Vec2::ZERO,
        Vec2::ONE,
        Vec2::Y,
    ] {
        let at = min + p * size;
        let uv = Vec2::new(
            placed.min.x + p.x * (placed.max.x - placed.min.x),
            placed.max.y - p.y * (placed.max.y - placed.min.y),
        );
        out.push(Vertex {
            pos: [at.x, at.y, 0.0],
            color,
            uv: [uv.x + PIXEL_U, uv.y],
        });
    }
    true
}

/// A decoded PNG: straight-alpha sRGB RGBA, rows top down.
struct Image {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

fn load() -> PixelArt {
    let (images, from) = match find_folder() {
        Some(folder) => (read_folder(&folder), folder.display().to_string()),
        None => (read_embedded(), "the copies built in".to_string()),
    };
    log::info!(
        "pixel art: {} of {} sprites, from {from}",
        images.len(),
        Sprite::all().len()
    );
    pack(images)
}

/// The first `assets/sprites` folder found: in the working directory, then
/// beside the executable.
fn find_folder() -> Option<PathBuf> {
    let from_exe = std::env::current_exe()
        .ok()
        .and_then(|exe| Some(exe.parent()?.join(FOLDER)));
    [Some(PathBuf::from(FOLDER)), from_exe]
        .into_iter()
        .flatten()
        .find(|folder| folder.is_dir())
}

fn read_folder(folder: &Path) -> Vec<(&'static str, Image)> {
    let sprites = Sprite::all();
    let mut images = Vec::new();
    for &sprite in &sprites {
        let tag = sprite.tag().expect("listed sprites have tags");
        let path = folder.join(format!("{tag}.png"));
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        match decode(&bytes) {
            Ok(image) => images.push((tag, checked(sprite, image, &path.display()))),
            Err(err) => log::warn!("couldn't read {}: {err}", path.display()),
        }
    }
    // A file the game doesn't look for is most likely a misspelled tag.
    for path in pngs_in(folder) {
        let known = path
            .strip_prefix(folder)
            .ok()
            .map(|p| p.with_extension("").to_string_lossy().replace('\\', "/"))
            .is_some_and(|tag| sprites.iter().any(|s| s.tag() == Some(tag.as_str())));
        if !known {
            log::warn!(
                "{} isn't a sprite the game uses (docs/sprites.md lists them)",
                path.display()
            );
        }
    }
    images
}

fn pngs_in(folder: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let Ok(entries) = std::fs::read_dir(folder) else {
        return found;
    };
    for path in entries.flatten().map(|entry| entry.path()) {
        if path.is_dir() {
            found.extend(pngs_in(&path));
        } else if path.extension().is_some_and(|ext| ext == "png") {
            found.push(path);
        }
    }
    found
}

fn read_embedded() -> Vec<(&'static str, Image)> {
    Sprite::all()
        .into_iter()
        .filter_map(|sprite| {
            let tag = sprite.tag()?;
            let (_, bytes) = EMBEDDED.iter().find(|(t, _)| *t == tag)?;
            let image = decode(bytes).expect("shipped sprites decode");
            Some((tag, checked(sprite, image, &tag)))
        })
        .collect()
}

/// Warns if `image` isn't the shape `sprite` is drawn in: it's stretched
/// to fit.
fn checked(sprite: Sprite, image: Image, name: &dyn std::fmt::Display) -> Image {
    let (w, h) = sprite.size();
    if image.width * h != image.height * w {
        log::warn!(
            "{name} is {}x{}, but is drawn {w}x{h}: it will look stretched",
            image.width,
            image.height
        );
    }
    image
}

/// Any PNG as 8-bit RGBA.
fn decode(bytes: &[u8]) -> Result<Image, String> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().map_err(|err| err.to_string())?;
    let size = reader.output_buffer_size().ok_or("the image is too big")?;
    let mut buffer = vec![0; size];
    let info = reader
        .next_frame(&mut buffer)
        .map_err(|err| err.to_string())?;
    buffer.truncate(info.buffer_size());
    let rgba = match info.color_type {
        png::ColorType::Rgba => buffer,
        png::ColorType::Rgb => buffer
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|&[r, g, b]| [r, g, b, 255])
            .collect(),
        png::ColorType::GrayscaleAlpha => buffer
            .as_chunks::<2>()
            .0
            .iter()
            .flat_map(|&[g, a]| [g, g, g, a])
            .collect(),
        png::ColorType::Grayscale => buffer.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::Indexed => return Err("unexpanded palette".into()),
    };
    if info.width == 0 || info.height == 0 {
        return Err("the image is empty".into());
    }
    Ok(Image {
        width: info.width,
        height: info.height,
        rgba,
    })
}

/// Lays the images out in rows, each with a transparent texel all round
/// so the shader's blend at a sprite's edge finds nothing of a neighbor.
fn pack(images: Vec<(&'static str, Image)>) -> PixelArt {
    const MIN_WIDTH: u32 = 256;
    let width = images
        .iter()
        .map(|(_, image)| image.width + 2)
        .max()
        .unwrap_or(1)
        .max(MIN_WIDTH);
    let mut spots = Vec::with_capacity(images.len());
    let (mut x, mut y, mut row) = (0, 0, 0);
    for (_, image) in &images {
        if x + image.width + 2 > width {
            (x, y, row) = (0, y + row, 0);
        }
        spots.push((x + 1, y + 1));
        x += image.width + 2;
        row = row.max(image.height + 2);
    }
    let height = (y + row).max(1);
    let mut pixels = vec![0; (width * height * 4) as usize];
    let mut placed = HashMap::default();
    for ((tag, image), (x, y)) in images.into_iter().zip(spots) {
        let row_bytes = (image.width * 4) as usize;
        for (row, line) in image.rgba.chunks_exact(row_bytes).enumerate() {
            let at = (((y + row as u32) * width + x) * 4) as usize;
            pixels[at..at + row_bytes].copy_from_slice(line);
        }
        let texture = Vec2::new(width as f32, height as f32);
        placed.insert(
            tag,
            Placed {
                min: Vec2::new(x as f32, y as f32) / texture,
                max: Vec2::new((x + image.width) as f32, (y + image.height) as f32) / texture,
            },
        );
    }
    PixelArt {
        atlas: Atlas {
            channels: 4,
            width,
            height,
            levels: vec![pixels],
            pixel_art: true,
        },
        placed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_sprite_has_its_own_tag_and_a_listing_in_the_docs() {
        let docs = include_str!("../../docs/sprites.md");
        let sprites = Sprite::all();
        for (i, a) in sprites.iter().enumerate() {
            let tag = a.tag().unwrap();
            assert!(
                sprites[i + 1..].iter().all(|b| b.tag() != Some(tag)),
                "{tag}"
            );
            let (w, h) = a.size();
            let row = docs
                .lines()
                .find(|line| line.contains(&format!("`{tag}`")))
                .unwrap_or_else(|| panic!("docs/sprites.md doesn't list {tag}"));
            assert!(row.contains(&format!("{w}x{h}")), "{tag} is {w}x{h}: {row}");
        }
    }

    #[test]
    fn shipped_sprites_are_tags_the_game_uses_at_their_size() {
        for (tag, bytes) in EMBEDDED {
            let sprite = Sprite::all()
                .into_iter()
                .find(|s| s.tag() == Some(tag))
                .unwrap_or_else(|| panic!("assets/sprites/{tag}.png isn't a tag the game uses"));
            let image = decode(bytes).unwrap();
            assert_eq!((image.width, image.height), sprite.size(), "{tag}");
        }
    }

    #[test]
    fn sprites_are_packed_apart_and_drawn_in_their_box() {
        let image = |width, height, value| Image {
            width,
            height,
            rgba: vec![value; (width * height * 4) as usize],
        };
        let art = pack(vec![
            ("terrain/plains", image(32, 28, 200)),
            ("unit/melee", image(14, 14, 100)),
            ("icon/paw", image(300, 2, 50)),
        ]);
        assert!(validate(&art.atlas));
        let (w, h) = (art.atlas.width as usize, art.atlas.height as usize);
        assert_eq!(w, 302, "as wide as the widest sprite and its border");
        let pixels = &art.atlas.levels[0];
        // Each sprite's own texels, and transparent texels all round it.
        for (tag, value) in [
            ("terrain/plains", 200),
            ("unit/melee", 100),
            ("icon/paw", 50),
        ] {
            let p = art.placed[tag];
            let (x0, y0) = ((p.min.x * w as f32) as usize, (p.min.y * h as f32) as usize);
            let (x1, y1) = ((p.max.x * w as f32) as usize, (p.max.y * h as f32) as usize);
            for y in y0 - 1..=y1 {
                for x in x0 - 1..=x1 {
                    let inside = (x0..x1).contains(&x) && (y0..y1).contains(&y);
                    let texel = pixels[(y * w + x) * 4];
                    assert_eq!(texel, if inside { value } else { 0 }, "{tag} at {x},{y}");
                }
            }
        }
    }

    #[test]
    fn a_sprite_draws_in_its_box_or_not_at_all() {
        let unit = Sprite::Unit(UnitIcon::Pike);
        assert!(has(unit), "the shipped art has a Pikeman");
        let mut out = Vec::new();
        assert!(push(unit, Vec2::new(3.0, 1.0), 2.0, [1.0; 4], &mut out));
        assert_eq!(out.len(), 6);
        let xs: Vec<f32> = out.iter().map(|v| v.pos[0]).collect();
        let width = xs.iter().cloned().fold(f32::MIN, f32::max)
            - xs.iter().cloned().fold(f32::MAX, f32::min);
        assert!((width - 14.0 * TEXEL * 2.0).abs() < 1e-5);
        assert!(
            out.iter()
                .all(|v| (PIXEL_U..PIXEL_U + 1.0).contains(&v.uv[0]))
        );
        // The box's top takes the image's top row.
        let top = out
            .iter()
            .max_by(|a, b| a.pos[1].total_cmp(&b.pos[1]))
            .unwrap();
        let bottom = out
            .iter()
            .min_by(|a, b| a.pos[1].total_cmp(&b.pos[1]))
            .unwrap();
        assert!(top.uv[1] < bottom.uv[1]);
        // Nothing for what has no sprite.
        assert!(!push(
            Sprite::Icon(MapIcon::Food),
            Vec2::ZERO,
            1.0,
            [1.0; 4],
            &mut out
        ));
        assert!(!push(
            Sprite::Ground(Terrain::Ocean, true),
            Vec2::ZERO,
            1.0,
            [1.0; 4],
            &mut out
        ));
        assert_eq!(out.len(), 6);
    }

    #[test]
    fn terrain_tiles_line_up_on_one_texel_grid() {
        let tile = Sprite::Ground(Terrain::Plains, false);
        for center in [Vec2::new(1.5, 0.866), Vec2::new(-4.5, 2.598)] {
            let mut out = Vec::new();
            push(tile, center, 1.0, [1.0; 4], &mut out);
            for v in &out {
                for coordinate in [v.pos[0], v.pos[1]] {
                    let texels = coordinate / TEXEL;
                    assert!((texels - texels.round()).abs() < 1e-3, "{coordinate}");
                }
            }
        }
    }

    #[test]
    fn pngs_of_any_color_type_decode_to_rgba() {
        let encode = |color, data: &[u8]| {
            let mut bytes = Vec::new();
            let mut encoder = png::Encoder::new(&mut bytes, 2, 1);
            encoder.set_color(color);
            encoder.set_depth(png::BitDepth::Eight);
            encoder
                .write_header()
                .unwrap()
                .write_image_data(data)
                .unwrap();
            bytes
        };
        let rgb = decode(&encode(png::ColorType::Rgb, &[1, 2, 3, 4, 5, 6])).unwrap();
        assert_eq!(rgb.rgba, [1, 2, 3, 255, 4, 5, 6, 255]);
        let gray = decode(&encode(png::ColorType::GrayscaleAlpha, &[9, 128, 7, 0])).unwrap();
        assert_eq!(gray.rgba, [9, 9, 9, 128, 7, 7, 7, 0]);
        assert!(decode(b"not a png").is_err());
    }

    fn validate(atlas: &Atlas) -> bool {
        atlas.levels.len() == 1
            && atlas.levels[0].len() == (atlas.width * atlas.height * 4) as usize
            && atlas.pixel_art
    }
}
