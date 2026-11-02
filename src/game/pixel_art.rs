//! Pixel-art sprites (`docs/sprites.md`): PNG files named by a tag, such as
//! `terrain/grassland` or `unit/pikeman`, that the map and the UI draw
//! instead of their vector art when the Art setting is Pixel
//! (`Settings::pixel_art`). Each is loaded once, at startup: from an
//! `assets/sprites/` folder in the working directory or beside the
//! executable if there is one, otherwise from the copies compiled in
//! (`build.rs` embeds whatever the repository's folder holds). A tag with no
//! file falls back to the vector art, so sprites can come one at a time.
//!
//! On the map every sprite is drawn in a box of its tag's size times
//! `TEXEL`, centered on where its vector art would be, so all of them share
//! one texel size; a PNG of another size is scaled into that box. In the UI
//! a sprite is drawn a whole number of screen pixels a texel (`push_ui`).
//! Texels are read one by one (no filtering, no mips: `mesh.frag`), and
//! pure magenta, in any shade, is the team color: a unit's, city's or
//! building's side color goes there. The map's shader paints it; for the UI
//! (ImGui can't) the atlas also holds a copy of each such sprite already
//! painted in each side's color.

use std::cell::Cell;
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
use super::unit::Team;
use super::unit_icons::UnitIcon;
use crate::renderer::{Atlas, Vertex};

/// A terrain tile's size in texels: a flat-top hexagon 64 wide.
const TILE: (u32, u32) = (64, 56);
/// World units a texel spans: a terrain tile's width is a hex's fill.
pub(super) const TEXEL: f32 = 2.0 * HEX_SIZE * HEX_FILL_SCALE / TILE.0 as f32;
/// Where the game looks for sprite files, from the working directory and
/// from the executable's.
const FOLDER: &str = "assets/sprites";
/// What pixel-art texture coordinates are shifted by, so the shader tells
/// them from the others (`Vertex::uv`). Past the vector sprites' 2 to 3,
/// with a whole unit between, so the end of one range is never the other.
pub(super) const PIXEL_U: f32 = 4.0;
/// The cloud tiles and puffs there are (`Sprite::CloudTile`,
/// `Sprite::CloudPuff`).
pub(super) const CLOUD_TILES: u8 = 4;
pub(super) const CLOUD_PUFFS: u8 = 3;

/// The shipped sprites, `(tag, PNG bytes)`, compiled in by `build.rs`.
const EMBEDDED: &[(&str, &[u8])] = include!(concat!(env!("OUT_DIR"), "/embedded_sprites.rs"));

/// An order or ability button's picture in the UI (`ui/action_icons.rs`
/// maps its buttons to these).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Order {
    Move,
    Attack,
    Swap,
    ShieldWall,
    Volley,
    Charge,
    Deploy,
    Lookout,
    Hold,
    Guard,
    Alert,
    Clear,
    Disband,
    Settle,
}

impl Order {
    const ALL: [Order; 14] = [
        Order::Move,
        Order::Attack,
        Order::Swap,
        Order::ShieldWall,
        Order::Volley,
        Order::Charge,
        Order::Deploy,
        Order::Lookout,
        Order::Hold,
        Order::Guard,
        Order::Alert,
        Order::Clear,
        Order::Disband,
        Order::Settle,
    ];
}

/// Something drawn as a pixel sprite.
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
    /// An order or ability button's picture (UI only).
    Order(Order),
    /// Cloud cover over an unexplored hex, one of `CLOUD_TILES`.
    CloudTile(u8),
    /// A cloud puff, by size, one of `CLOUD_PUFFS`.
    CloudPuff(u8),
}

impl Sprite {
    /// Its file's name under `assets/sprites/`, without `.png`, or `None`
    /// for what never shows (hills under water).
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
                MapIcon::Food => "icon/food",
                MapIcon::Wood => "icon/wood",
                MapIcon::Metal => "icon/metal",
                MapIcon::Clock => "icon/clock",
            },
            Sprite::Order(order) => match order {
                Order::Move => "ui/move",
                Order::Attack => "ui/attack",
                Order::Swap => "ui/swap",
                Order::ShieldWall => "ui/shield_wall",
                Order::Volley => "ui/volley",
                Order::Charge => "ui/charge",
                Order::Deploy => "ui/deploy",
                Order::Lookout => "ui/lookout",
                Order::Hold => "ui/hold",
                Order::Guard => "ui/guard",
                Order::Alert => "ui/alert",
                Order::Clear => "ui/clear",
                Order::Disband => "ui/disband",
                Order::Settle => "ui/settle",
            },
            Sprite::CloudTile(i) => match i {
                0 => "cloud/tile_1",
                1 => "cloud/tile_2",
                2 => "cloud/tile_3",
                3 => "cloud/tile_4",
                _ => return None,
            },
            Sprite::CloudPuff(i) => match i {
                0 => "cloud/puff_small",
                1 => "cloud/puff_medium",
                2 => "cloud/puff_large",
                _ => return None,
            },
        })
    }

    /// Its size in texels: what its PNG should measure.
    pub(super) fn size(self) -> (u32, u32) {
        match self {
            Sprite::Ground(..) | Sprite::Feature(_) | Sprite::CloudTile(_) => TILE,
            Sprite::Unit(_) | Sprite::Building(_) => (28, 28),
            Sprite::City => (32, 32),
            Sprite::Icon(_) | Sprite::Order(_) => (16, 16),
            Sprite::CloudPuff(i) => match i {
                0 => (48, 32),
                1 => (80, 56),
                _ => (112, 80),
            },
        }
    }

    /// Every sprite there is, each with its own tag.
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
            .chain(Order::ALL.map(Sprite::Order))
            .chain((0..CLOUD_TILES).map(Sprite::CloudTile))
            .chain((0..CLOUD_PUFFS).map(Sprite::CloudPuff))
            .filter(|sprite| sprite.tag().is_some())
            .collect()
    }

    /// Terrain lines up on one texel grid across the map; things that move
    /// don't, so they glide smoothly.
    fn snaps(self) -> bool {
        matches!(
            self,
            Sprite::Ground(..) | Sprite::Feature(_) | Sprite::CloudTile(_)
        )
    }
}

thread_local! {
    /// Whether what this thread is drawing now is in pixel art: the map
    /// and UI set it from the Art setting as they start (`set_active`), so
    /// text deep inside them (the icon characters, `map_icons.rs`) follows.
    static ACTIVE: Cell<bool> = const { Cell::new(false) };
    /// The player's side, whose color the UI paints its sprites in.
    static PLAYER: Cell<Team> = const { Cell::new(Team::Blue) };
}

/// Draw what follows in pixel art, or in vector art, for `player`.
pub(super) fn set_active(pixel: bool, player: Team) {
    ACTIVE.set(pixel);
    PLAYER.set(player);
}

/// The side `set_active` was given.
pub(super) fn player() -> Team {
    PLAYER.get()
}

/// The side whose color `color` is, if one's is.
pub(super) fn team_of(color: Color) -> Option<Team> {
    painted_teams().find(|t| t.color()[..3] == color[..3])
}

/// Whether to draw in pixel art (`set_active`).
pub(super) fn active() -> bool {
    ACTIVE.get()
}

/// A sprite's place in the atlas, in texture coordinates.
#[derive(Clone, Copy, Debug)]
struct Placed {
    min: Vec2,
    max: Vec2,
}

/// Which copy of a sprite: as drawn (0), or painted in a side's color
/// (1 and on, `team_variant`).
type Variant = u8;

struct PixelArt {
    atlas: Atlas,
    placed: HashMap<(&'static str, Variant), Placed>,
}

static ART: LazyLock<PixelArt> = LazyLock::new(load);

/// The sides a sprite is painted for, the animals' owner last.
fn painted_teams() -> impl Iterator<Item = Team> {
    Team::ALL.into_iter().chain([Team::Wild])
}

fn team_variant(team: Team) -> Variant {
    painted_teams().position(|t| t == team).unwrap_or(0) as Variant + 1
}

/// The atlas of every pixel sprite found, for the renderer.
pub fn atlas() -> &'static Atlas {
    &ART.atlas
}

/// Whether `sprite` has a file.
pub(super) fn has(sprite: Sprite) -> bool {
    sprite
        .tag()
        .is_some_and(|tag| ART.placed.contains_key(&(tag, 0)))
}

/// Draws `sprite` on the map, centered on `center`, `scale` times its usual
/// size, its team pixels in `color`'s RGB and all of it faded by `color`'s
/// alpha. Returns false, drawing nothing, if it has no file.
pub(super) fn push(
    sprite: Sprite,
    center: Vec2,
    scale: f32,
    color: Color,
    out: &mut Vec<Vertex>,
) -> bool {
    let (w, h) = sprite.size();
    let size = Vec2::new(w as f32, h as f32) * TEXEL * scale;
    let mut min = center - size / 2.0;
    if sprite.snaps() {
        min = (min / TEXEL).round() * TEXEL;
    }
    push_box(sprite, 0, min, size, color, out)
}

/// Draws `sprite` `height` tall (as wide as its shape makes it), centered
/// on `center`, in any space: world text's icons.
pub(super) fn push_sized(
    sprite: Sprite,
    center: Vec2,
    height: f32,
    color: Color,
    out: &mut Vec<Vertex>,
) -> bool {
    let (w, h) = sprite.size();
    let size = Vec2::new(w as f32 / h as f32 * height, height);
    push_box(sprite, 0, center - size / 2.0, size, color, out)
}

/// Draws `sprite` in the UI (pixels, Y up), centered on `center`: as big
/// as fits in `room` pixels across and down, a whole number of pixels a
/// texel (at least one) so it stays crisp, its corner on a whole pixel. A
/// team-colored sprite comes in `team`'s color (painted in the atlas, as
/// ImGui can't paint it); `alpha` fades it. Returns false, drawing nothing,
/// if it has no file.
pub(super) fn push_ui(
    sprite: Sprite,
    center: Vec2,
    room: f32,
    team: Option<Team>,
    alpha: f32,
    out: &mut Vec<Vertex>,
) -> bool {
    let (w, h) = sprite.size();
    let scale = (room / w.max(h) as f32).floor().max(1.0);
    let size = Vec2::new(w as f32, h as f32) * scale;
    let min = (center - size / 2.0).round();
    let Some(tag) = sprite.tag() else {
        return false;
    };
    let variant = team
        .map(team_variant)
        .filter(|&v| ART.placed.contains_key(&(tag, v)))
        .unwrap_or(0);
    push_box(sprite, variant, min, size, [1.0, 1.0, 1.0, alpha], out)
}

fn push_box(
    sprite: Sprite,
    variant: Variant,
    min: Vec2,
    size: Vec2,
    color: Color,
    out: &mut Vec<Vertex>,
) -> bool {
    let Some(placed) = sprite.tag().and_then(|tag| ART.placed.get(&(tag, variant))) else {
        return false;
    };
    // Y is up and image rows go down: the bottom of the box takes the
    // bottom row of the image.
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

/// Whether `vertex` is a pixel sprite's (`push`, `push_ui`), and where in
/// the atlas, from 0 to 1, if so: ImGui draws those as images.
pub(super) fn atlas_uv(vertex: &Vertex) -> Option<[f32; 2]> {
    (PIXEL_U..PIXEL_U + 1.0)
        .contains(&vertex.uv[0])
        .then(|| [vertex.uv[0] - PIXEL_U, vertex.uv[1]])
}

/// A decoded PNG: straight-alpha sRGB RGBA, rows top down.
#[derive(Clone)]
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
    pack(with_team_variants(images))
}

/// Each image, then a copy of each with team pixels for every side.
fn with_team_variants(images: Vec<(&'static str, Image)>) -> Vec<(&'static str, Variant, Image)> {
    let mut all = Vec::new();
    for (tag, image) in images {
        for team in painted_teams() {
            if let Some(painted) = paint_team(&image, team) {
                all.push((tag, team_variant(team), painted));
            }
        }
        all.push((tag, 0, image));
    }
    all
}

/// Whether an sRGB texel is a team pixel: pure magenta, any shade.
fn is_team_pixel([r, g, b, a]: [u8; 4]) -> bool {
    a > 0 && g == 0 && r == b && r > 0
}

/// `image` with its team pixels painted in `team`'s color as the shader
/// would paint them (in linear light), or `None` if it has none.
fn paint_team(image: &Image, team: Team) -> Option<Image> {
    let to_linear = |c: u8| {
        let c = c as f32 / 255.0;
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    let to_srgb = |c: f32| {
        let c = c.clamp(0.0, 1.0);
        let s = if c <= 0.003_130_8 {
            c * 12.92
        } else {
            1.055 * c.powf(1.0 / 2.4) - 0.055
        };
        (s * 255.0).round() as u8
    };
    let color = team.color();
    let mut painted = image.clone();
    let mut any = false;
    for texel in painted.rgba.as_chunks_mut::<4>().0 {
        if is_team_pixel(*texel) {
            any = true;
            let shade = to_linear(texel[0]);
            for (channel, c) in texel[..3].iter_mut().zip(color) {
                *channel = to_srgb(c * shade);
            }
        }
    }
    any.then_some(painted)
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

/// Lays the images out in rows, tallest first, each with a transparent
/// texel all round so the shader's blend at a sprite's edge finds nothing
/// of a neighbor.
fn pack(mut images: Vec<(&'static str, Variant, Image)>) -> PixelArt {
    const MIN_WIDTH: u32 = 1024;
    images.sort_by_key(|(tag, variant, image)| (std::cmp::Reverse(image.height), *tag, *variant));
    let width = images
        .iter()
        .map(|(.., image)| image.width + 2)
        .max()
        .unwrap_or(1)
        .max(MIN_WIDTH);
    let mut spots = Vec::with_capacity(images.len());
    let (mut x, mut y, mut row) = (0, 0, 0);
    for (.., image) in &images {
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
    let texture = Vec2::new(width as f32, height as f32);
    for ((tag, variant, image), (x, y)) in images.into_iter().zip(spots) {
        let row_bytes = (image.width * 4) as usize;
        for (row, line) in image.rgba.chunks_exact(row_bytes).enumerate() {
            let at = (((y + row as u32) * width + x) * 4) as usize;
            pixels[at..at + row_bytes].copy_from_slice(line);
        }
        placed.insert(
            (tag, variant),
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
                .find(|line| line.starts_with(&format!("| `{tag}` |")))
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
            ("terrain/plains", 0, image(64, 56, 200)),
            ("unit/melee", 0, image(28, 28, 100)),
            ("icon/paw", 0, image(1300, 2, 50)),
        ]);
        assert!(validate(&art.atlas));
        let (w, h) = (art.atlas.width as usize, art.atlas.height as usize);
        assert_eq!(w, 1302, "as wide as the widest sprite and its border");
        let pixels = &art.atlas.levels[0];
        // Each sprite's own texels, and transparent texels all round it.
        for (tag, value) in [
            ("terrain/plains", 200),
            ("unit/melee", 100),
            ("icon/paw", 50),
        ] {
            let p = art.placed[&(tag, 0)];
            let (x0, y0) = ((p.min.x * w as f32) as usize, (p.min.y * h as f32) as usize);
            let (x1, y1) = (
                (p.max.x * w as f32).round() as usize,
                (p.max.y * h as f32).round() as usize,
            );
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
    fn team_pixels_are_painted_for_every_side_as_the_shader_paints_them() {
        let image = Image {
            width: 3,
            height: 1,
            rgba: vec![255, 0, 255, 255, 128, 0, 128, 255, 10, 20, 30, 255],
        };
        let red = paint_team(&image, Team::Red).unwrap();
        // Full magenta is the side's color, encoded as sRGB.
        let [r, g, b, _] = Team::Red.color().map(|c| {
            let s = 1.055 * c.powf(1.0 / 2.4) - 0.055;
            (s * 255.0).round() as u8
        });
        assert_eq!(&red.rgba[..3], &[r, g, b]);
        // A darker magenta is a darker red; other pixels stay.
        assert!(red.rgba[4] < r && red.rgba[5] < g.max(1));
        assert_eq!(&red.rgba[8..], &[10, 20, 30, 255]);
        let plain = Image {
            width: 1,
            height: 1,
            rgba: vec![10, 20, 30, 255],
        };
        assert!(paint_team(&plain, Team::Red).is_none());
        // The shipped Pikeman has a painted copy for every side.
        for team in painted_teams() {
            assert!(
                ART.placed
                    .contains_key(&("unit/pikeman", team_variant(team)))
            );
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
        assert!((width - 28.0 * TEXEL * 2.0).abs() < 1e-5);
        assert!(out.iter().all(|v| atlas_uv(v).is_some()));
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
        let sunk = Sprite::Ground(Terrain::Ocean, true);
        assert!(!push(sunk, Vec2::ZERO, 1.0, [1.0; 4], &mut out));
        assert_eq!(out.len(), 6);
    }

    #[test]
    fn a_ui_sprite_is_a_whole_number_of_pixels_a_texel_on_whole_pixels() {
        let unit = Sprite::Unit(UnitIcon::Sword);
        for (room, side) in [(10.0, 28.0), (28.0, 28.0), (40.0, 28.0), (60.0, 56.0)] {
            let mut out = Vec::new();
            assert!(push_ui(
                unit,
                Vec2::new(100.3, 50.6),
                room,
                Some(Team::Blue),
                1.0,
                &mut out
            ));
            let xs = out.iter().map(|v| v.pos[0]);
            let (min, max) = xs.fold((f32::MAX, f32::MIN), |(a, b), x| (a.min(x), b.max(x)));
            assert_eq!(max - min, side, "room {room}");
            assert_eq!(min, min.round());
        }
        // The painted copy, so its color needs no shader.
        let mut painted = Vec::new();
        push_ui(unit, Vec2::ZERO, 28.0, Some(Team::Red), 0.5, &mut painted);
        let mut plain = Vec::new();
        push_ui(unit, Vec2::ZERO, 28.0, None, 0.5, &mut plain);
        assert_ne!(painted[0].uv, plain[0].uv);
        assert_eq!(painted[0].color, [1.0, 1.0, 1.0, 0.5]);
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
