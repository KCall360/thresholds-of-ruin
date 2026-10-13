//! Structural bounds, distinct from ruleset tuning.
pub const ID_BYTES: usize = 60;
pub const CATALOG_ENTRIES: usize = 4096;
pub const MODIFIERS_PER_SOURCE: usize = 128;
pub const REQUIREMENT_DEPTH: usize = 16;
pub const REQUIREMENT_NODES: usize = 256;
pub const HIT_DICE: usize = 256;
pub const TEMPLATES: usize = 32;
pub const CONDITIONS: usize = 128;
pub const NUMERIC: i64 = 1_000_000;
pub const DICE: u16 = 64;
pub const DAMAGE_COMPONENTS: usize = 32;
pub const POINT_POOLS_PER_SOURCE: usize = 16;
pub const POINTS_PER_HIT_DIE: usize = 256;
/// Total purchased talents, whether active or dormant; independent of point grants.
pub const OWNED_TALENTS: usize = 256;
pub const COST_HOLDS: usize = 64;
pub const DISPLAY_NAME_BYTES: usize = 256;
pub const EFFECTS_PER_BRANCH: usize = 128;
pub const DESCRIPTORS_PER_EFFECT: usize = 128;
pub const STAT_MAGNITUDE: i64 = 1000;
pub const DERIVED_ATTRIBUTE: u16 = 1000;
pub const DICE_SIDES: u16 = 1000;
pub const SIGHT_RANGE: u16 = 16;
pub const EQUIPMENT_SOCKETS: usize = 64;
pub const CONSUMABLE_EFFECTS: usize = 16;

pub const TAGS_PER_ABILITY: usize = 128;

/// Bounded ordered attacks; independent from content tuning.
pub const STRIKES: usize = 16;
pub const RIDERS_PER_STRIKE: usize = 8;
pub const EFFECTS_PER_SEQUENCE: usize = 256;
pub const DESCRIPTORS_PER_SEQUENCE: usize = 256;
