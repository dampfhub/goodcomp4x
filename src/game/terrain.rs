//! Tiles: a base terrain, optionally raised into hills and covered by a
//! feature (forest or jungle). Together they set what a hex yields when
//! worked, whether units can enter it, what it costs to carry goods through,
//! and how it helps a unit defending on it. An unimproved tile gives at most
//! `UNIMPROVED_YIELD_CAP` goods in all (`Tile::yields`): hills and features
//! change which goods, not how many.

/// The ground a tile is made of.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Terrain {
    /// Lush, food-rich lowland.
    Grassland,
    #[default]
    Plains,
    /// Dry and poor, unless fresh water is next to it.
    Desert,
    /// Cold, thin land toward the poles.
    Tundra,
    /// Frozen waste: yields nothing.
    Snow,
    /// Wet, warm lowland; where jungle grows.
    Marsh,
    /// Impassable: nothing can enter or path through.
    Mountains,
    /// Shallow water along a shore. Ships can enter; cities can work it.
    Coast,
    /// Deep water, away from any shore.
    Ocean,
    /// Water enclosed by land. Gives fresh water to the land around it.
    Lake,
}

/// Strategic resources unlock specialist units when a Barracks occupies the
/// resource tile.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Resource {
    Horses,
    Iron,
}

impl Resource {
    #[cfg(test)]
    pub const ALL: [Resource; 2] = [Resource::Horses, Resource::Iron];

    /// Its place in `ALL`.
    pub fn index(self) -> usize {
        self as usize
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Horses => "HORSES",
            Self::Iron => "IRON",
        }
    }
}

/// Rich land worth scouting for and fighting over: a tile that yields past
/// the cap (`SPECIAL_BONUS` more) when a city works it. Placed by the world's map
/// generator between the starts (`mapgen.rs`). The kinds and numbers are a
/// first pass.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Special {
    /// Fruit trees: more food.
    Orchard,
    /// Good stone near the surface: more metal.
    Quarry,
}

impl Special {
    pub const ALL: [Special; 2] = [Special::Orchard, Special::Quarry];

    pub fn name(self) -> &'static str {
        match self {
            Self::Orchard => "ORCHARD",
            Self::Quarry => "QUARRY",
        }
    }

    /// Food, wood and metal it adds to the tile's own when worked, past the
    /// cap and on top of any improvement.
    pub fn bonus(self) -> (i32, i32, i32) {
        match self {
            Self::Orchard => (SPECIAL_BONUS, 0, 0),
            Self::Quarry => (0, 0, SPECIAL_BONUS),
        }
    }
}

impl Terrain {
    pub fn name(self) -> &'static str {
        match self {
            Terrain::Grassland => "GRASSLAND",
            Terrain::Plains => "PLAINS",
            Terrain::Desert => "DESERT",
            Terrain::Tundra => "TUNDRA",
            Terrain::Snow => "SNOW",
            Terrain::Marsh => "MARSH",
            Terrain::Mountains => "MOUNTAINS",
            Terrain::Coast => "COAST",
            Terrain::Ocean => "OCEAN",
            Terrain::Lake => "LAKE",
        }
    }

    pub fn is_water(self) -> bool {
        matches!(self, Terrain::Coast | Terrain::Ocean | Terrain::Lake)
    }

    /// Whether units can enter it: land other than mountains.
    pub fn is_passable(self) -> bool {
        self != Terrain::Mountains && !self.is_water()
    }

    /// Whether a city can put a citizen on it: anything but mountains.
    pub fn is_workable(self) -> bool {
        self != Terrain::Mountains
    }

    /// Food and wood of the bare ground.
    fn goods(self) -> (i32, i32) {
        match self {
            Terrain::Grassland | Terrain::Plains => (2, 0),
            Terrain::Desert => (0, 1),
            Terrain::Tundra | Terrain::Marsh => (1, 1),
            Terrain::Snow | Terrain::Mountains => (0, 0),
            Terrain::Coast | Terrain::Lake => (2, 0),
            Terrain::Ocean => (1, 0),
        }
    }
}

/// Most goods, food, wood and metal together, an unimproved tile gives when
/// worked, fresh water and hills included (`Tile::yields`). Specials
/// (`SPECIAL_BONUS`) and improvements go past it.
pub const UNIMPROVED_YIELD_CAP: i32 = 2;
/// Food fresh water (a river or lake beside the tile) adds, within the cap.
pub const FRESH_WATER_FOOD: i32 = 1;
/// Metal hills dig out of any tile, which keeps 1 of its own main good.
pub const HILLS_METAL: i32 = 1;
/// Wood a forest gives: a forested tile gives wood only.
pub const FOREST_WOOD: i32 = 2;
/// What a special tile (`Special`) adds to its capped tile: one more of its
/// good, so it's the one kind of tile giving 3 goods unimproved.
pub const SPECIAL_BONUS: i32 = 1;

/// Vegetation covering a tile.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Feature {
    /// Wood only, instead of the ground's goods; cover for defenders.
    Forest,
    /// Dense growth on marsh: cover, and the marsh's own food and wood.
    Jungle,
}

impl Feature {
    pub fn name(self) -> &'static str {
        match self {
            Feature::Forest => "FOREST",
            Feature::Jungle => "JUNGLE",
        }
    }
}

/// Everything about a hex's ground.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Tile {
    pub terrain: Terrain,
    /// Raised ground: metal, better defense.
    pub hills: bool,
    pub feature: Option<Feature>,
}

impl From<Terrain> for Tile {
    fn from(terrain: Terrain) -> Self {
        Tile {
            terrain,
            hills: false,
            feature: None,
        }
    }
}

/// Trims `food`, `wood` and `metal` to `UNIMPROVED_YIELD_CAP` in all: food
/// first, then wood, then metal.
fn capped(food: i32, wood: i32, metal: i32) -> (i32, i32, i32) {
    let mut goods = [food, wood, metal];
    let mut excess = (food + wood + metal - UNIMPROVED_YIELD_CAP).max(0);
    for good in &mut goods {
        let cut = excess.min(*good);
        *good -= cut;
        excess -= cut;
    }
    (goods[0], goods[1], goods[2])
}

/// Defense bonus for standing on hills, and under forest or jungle.
const HILLS_DEFENSE: f32 = 0.25;
const COVER_DEFENSE: f32 = 0.15;

impl Tile {
    /// Plains hills, for the hand-built scenarios.
    pub const HILLS: Tile = Tile {
        terrain: Terrain::Plains,
        hills: true,
        feature: None,
    };
    pub const MOUNTAINS: Tile = Tile {
        terrain: Terrain::Mountains,
        hills: false,
        feature: None,
    };

    /// Like "DESERT HILLS" or "GRASSLAND HILLS + FOREST".
    pub fn name(self) -> String {
        let mut name = self.terrain.name().to_string();
        if self.hills {
            name += " HILLS";
        }
        if let Some(feature) = self.feature {
            name += " + ";
            name += feature.name();
        }
        name
    }

    /// Food, wood and metal when worked unimproved, with or without fresh
    /// water beside it: the ground's, or wood only under a forest (jungle
    /// keeps the marsh's); hills dig out `HILLS_METAL` and keep 1 of the
    /// tile's main good (food on a tie); fresh water adds food on open land. The cap
    /// (`UNIMPROVED_YIELD_CAP`) applies here and nowhere else: past it food
    /// goes first, so fresh water only fills room below the cap.
    pub fn yields(self, fresh_water: bool) -> (i32, i32, i32) {
        let (mut food, mut wood) = match self.feature {
            Some(Feature::Forest) => (0, FOREST_WOOD),
            Some(Feature::Jungle) | None => self.terrain.goods(),
        };
        let mut metal = 0;
        if self.hills {
            (food, wood) = match (food, wood) {
                (0, 0) => (0, 0),
                (f, w) if f >= w => (1, 0),
                _ => (0, 1),
            };
            metal = HILLS_METAL;
        }
        if fresh_water && self.terrain.is_passable() {
            food += FRESH_WATER_FOOD;
        }
        capped(food, wood, metal)
    }

    /// Multiplier on the defense of a unit standing here: +25% on hills and
    /// +15% under forest or jungle, added together.
    pub fn defense_multiplier(self) -> f32 {
        let mut multiplier = 1.0;
        if self.hills {
            multiplier += HILLS_DEFENSE;
        }
        if self.feature.is_some() {
            multiplier += COVER_DEFENSE;
        }
        multiplier
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tile(terrain: Terrain, hills: bool, feature: Option<Feature>) -> Tile {
        Tile {
            terrain,
            hills,
            feature,
        }
    }

    /// The yield table in `docs/game-rules.md`, as (food, wood, metal)
    /// without fresh water and with it.
    #[test]
    fn yields_follow_the_table() {
        use Feature::{Forest, Jungle};
        use Terrain::*;
        let table = [
            (tile(Grassland, false, None), (2, 0, 0), (2, 0, 0)),
            (tile(Plains, false, None), (2, 0, 0), (2, 0, 0)),
            (tile(Plains, false, Some(Forest)), (0, 2, 0), (0, 2, 0)),
            (tile(Grassland, false, Some(Forest)), (0, 2, 0), (0, 2, 0)),
            (tile(Tundra, false, Some(Forest)), (0, 2, 0), (0, 2, 0)),
            (tile(Plains, true, None), (1, 0, 1), (1, 0, 1)),
            (tile(Plains, true, Some(Forest)), (0, 1, 1), (0, 1, 1)),
            (tile(Desert, false, None), (0, 1, 0), (1, 1, 0)),
            (tile(Desert, true, None), (0, 1, 1), (0, 1, 1)),
            (tile(Tundra, false, None), (1, 1, 0), (1, 1, 0)),
            (tile(Tundra, true, None), (1, 0, 1), (1, 0, 1)),
            (tile(Marsh, false, None), (1, 1, 0), (1, 1, 0)),
            (tile(Marsh, false, Some(Jungle)), (1, 1, 0), (1, 1, 0)),
            (tile(Snow, false, None), (0, 0, 0), (1, 0, 0)),
            (tile(Snow, true, None), (0, 0, 1), (1, 0, 1)),
            (tile(Coast, false, None), (2, 0, 0), (2, 0, 0)),
            (tile(Lake, false, None), (2, 0, 0), (2, 0, 0)),
            (tile(Ocean, false, None), (1, 0, 0), (1, 0, 0)),
            (tile(Mountains, false, None), (0, 0, 0), (0, 0, 0)),
        ];
        for (tile, dry, fresh) in table {
            assert_eq!(tile.yields(false), dry, "{}", tile.name());
            assert_eq!(tile.yields(true), fresh, "{} with fresh water", tile.name());
        }
    }

    #[test]
    fn no_unimproved_tile_gives_more_than_the_cap() {
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
        for terrain in terrains {
            for hills in [false, true] {
                for feature in [None, Some(Feature::Forest), Some(Feature::Jungle)] {
                    for fresh_water in [false, true] {
                        let (food, wood, metal) = tile(terrain, hills, feature).yields(fresh_water);
                        assert!(food >= 0 && wood >= 0 && metal >= 0);
                        assert!(food + wood + metal <= UNIMPROVED_YIELD_CAP);
                    }
                }
            }
        }
    }

    #[test]
    fn hills_and_features_modify_the_ground() {
        let desert_hills = tile(Terrain::Desert, true, None);
        assert_eq!(desert_hills.name(), "DESERT HILLS");
        let forested_hills = tile(Terrain::Plains, true, Some(Feature::Forest));
        assert!((forested_hills.defense_multiplier() - 1.4).abs() < 1e-6);
        assert_eq!(forested_hills.name(), "PLAINS HILLS + FOREST");
    }
}
