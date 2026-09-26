#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Terrain {
    #[default]
    Plains,
    /// High ground: units standing here defend better.
    Hills,
    /// Impassable: nothing can enter or path through.
    Mountains,
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
            Self::Horses => "HORSES",
            Self::Iron => "IRON",
        }
    }
    pub fn glyph(self) -> char {
        match self {
            Self::Horses => 'H',
            Self::Iron => 'I',
        }
    }
}

impl Terrain {
    pub fn is_passable(self) -> bool {
        self != Terrain::Mountains
    }

    /// Multiplier on the defense of a unit standing on this terrain.
    pub fn defense_multiplier(self) -> f32 {
        match self {
            Terrain::Hills => 1.25,
            Terrain::Plains | Terrain::Mountains => 1.0,
        }
    }
}
