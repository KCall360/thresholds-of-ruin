/// Backend material identity. Future materials add their own physical properties;
/// appearance strings are never used to determine solidity or interaction rules.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Material {
    Stone,
}

impl Material {
    pub fn name(self) -> &'static str {
        match self {
            Self::Stone => "stone",
        }
    }
}

/// Empty allocated space is distinct from both solid material and missing space.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Terrain {
    Empty,
    Solid(Material),
}
