//! Tiles: a base terrain, optionally raised into hills and covered by a
//! feature (forest or jungle). Together they set what a hex yields when
//! worked, whether units can enter it, what it costs to carry goods through,
//! and how it helps a unit defending on it. Yields are first-pass numbers,
//! meant to be tuned.

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
    /// Shallow water along a shore. Units can't enter water, but cities can
    /// work it.
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
    pub fn name(self) -> &'static str {
        match self {
            Self::Horses => "SEAHORSES",
            Self::Iron => "PEARLS",
        }
    }
}

impl Terrain {
    pub fn name(self) -> &'static str {
        match self {
            Terrain::Grassland => "SEAGRASS",
            Terrain::Plains => "SANDBANK",
            Terrain::Desert => "SILT",
            Terrain::Tundra => "COLD SHELF",
            Terrain::Snow => "ICE SHELF",
            Terrain::Marsh => "MUD FLATS",
            Terrain::Mountains => "SEAMOUNT",
            Terrain::Coast => "BEACH",
            Terrain::Ocean => "ISLAND",
            Terrain::Lake => "TIDE POOL",
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

    /// Food and production of the bare ground.
    fn yields(self) -> (i32, i32) {
        match self {
            Terrain::Grassland => (3, 0),
            Terrain::Plains => (2, 1),
            Terrain::Desert => (0, 1),
            Terrain::Tundra => (1, 1),
            Terrain::Snow => (0, 0),
            Terrain::Marsh => (1, 0),
            Terrain::Mountains => (0, 0),
            Terrain::Coast | Terrain::Lake => (2, 0),
            Terrain::Ocean => (1, 0),
        }
    }
}

/// Vegetation covering a tile.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Feature {
    /// Wood for production at some cost in food; cover for defenders.
    Forest,
    /// Dense growth on marsh: some food and wood, and cover.
    Jungle,
}

impl Feature {
    pub fn name(self) -> &'static str {
        match self {
            Feature::Forest => "KELP FOREST",
            Feature::Jungle => "CORAL THICKET",
        }
    }
}

/// Everything about a hex's ground.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Tile {
    pub terrain: Terrain,
    /// Raised ground: more production, better defense, harder hauling.
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

    /// Like "SILT REEF" or "SEAGRASS REEF + KELP FOREST": the ocean theme's display names.
    pub fn name(self) -> String {
        let mut name = self.terrain.name().to_string();
        if self.hills {
            name += " REEF";
        }
        if let Some(feature) = self.feature {
            name += " + ";
            name += feature.name();
        }
        name
    }

    /// Food and production when worked, before fresh water or improvements:
    /// the ground's, with hills adding production, forest trading a food for
    /// a production, and jungle adding one of each.
    pub fn yields(self) -> (i32, i32) {
        let (mut food, mut production) = self.terrain.yields();
        if self.hills {
            production += 1;
        }
        match self.feature {
            Some(Feature::Forest) => {
                food = (food - 1).max(0);
                production += 1;
            }
            Some(Feature::Jungle) => {
                food += 1;
                production += 1;
            }
            None => {}
        }
        (food, production)
    }

    /// Logistics cost of carrying goods into this hex off-road, in half-hex
    /// units (a road costs 1): 2 on open ground, 3 on snow or marsh, and one
    /// more each for hills and forest or jungle.
    pub fn route_cost(self) -> i32 {
        let ground = match self.terrain {
            Terrain::Snow | Terrain::Marsh => 3,
            _ => 2,
        };
        ground + i32::from(self.hills) + i32::from(self.feature.is_some())
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

    #[test]
    fn hills_and_features_modify_the_ground() {
        let desert_hills = Tile {
            terrain: Terrain::Desert,
            hills: true,
            feature: None,
        };
        assert_eq!(desert_hills.yields(), (0, 2));
        assert_eq!(desert_hills.name(), "SILT REEF");
        let forested_hills = Tile {
            terrain: Terrain::Plains,
            hills: true,
            feature: Some(Feature::Forest),
        };
        assert_eq!(forested_hills.yields(), (1, 3));
        assert_eq!(forested_hills.route_cost(), 4);
        assert!((forested_hills.defense_multiplier() - 1.4).abs() < 1e-6);
        assert_eq!(forested_hills.name(), "SANDBANK REEF + KELP FOREST");
        let jungle = Tile {
            terrain: Terrain::Marsh,
            hills: false,
            feature: Some(Feature::Jungle),
        };
        assert_eq!(jungle.yields(), (2, 1));
    }
}
