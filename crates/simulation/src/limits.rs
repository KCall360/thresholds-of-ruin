//! Structural bounds, distinct from ruleset tuning.
pub const ID_BYTES: usize = 60;
pub const CATALOG_ENTRIES: usize = 4096;
pub const MODIFIERS_PER_SOURCE: usize = 128;
pub const REQUIREMENT_DEPTH: usize = 16;
pub const REQUIREMENT_NODES: usize = 256;
/// Temporary actual-row admission guard until exact archive and disclosure paging.
/// Scalar counts, ownership identities and funded tiers are independent u64 domains.
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
pub const OWNED_EXPERTISES: usize = 256;
/// Acquisition operands per root; bounded separately from the global catalog.
pub const FORMS_PER_TALENT: usize = 16;
pub const COST_HOLDS: usize = 64;
/// Distinct finite-supply rows captured by a single action.
pub const SUPPLY_COSTS: usize = 8;
pub const DISPLAY_NAME_BYTES: usize = 256;
pub const EFFECTS_PER_BRANCH: usize = 128;
pub const DESCRIPTORS_PER_EFFECT: usize = 128;
pub const STAT_MAGNITUDE: i64 = 1000;
pub const DERIVED_ATTRIBUTE: u16 = 1000;
pub const DICE_SIDES: u16 = 1000;
pub const SIGHT_RANGE: u16 = 16;
/// Every body/eye offset lies in [-8, 8]^3. Each physical view adds an L1 ball
/// of radius SIGHT_RANGE; portal repeats never add a second result for one
/// offset. The cube's interior, faces, edges and corners therefore contribute
/// 17^3 + 6*17^2*r + 12*17*C(r,2) + 8*C(r,3) offsets. The preserved immediate
/// neighbourhood is contained in that envelope. Add the two abstract stairs.
/// This conservative scene bound also bounds one aim's historical occurrences
/// and distinct perceived threats; it is not a claimed attainable single aim.
pub const PUBLIC_AIM_OCCURRENCES: usize = {
    let r = SIGHT_RANGE as usize;
    17_usize.pow(3)
        + 6 * 17_usize.pow(2) * r
        + 12 * 17 * (r * (r - 1) / 2)
        + 8 * (r * (r - 1) * (r - 2) / 6)
        + 2
};
pub const EQUIPMENT_SOCKETS: usize = 64;
pub const CONSUMABLE_EFFECTS: usize = 16;

pub const TAGS_PER_ABILITY: usize = 128;

/// Bounded ordered attacks; independent from content tuning.
pub const STRIKES: usize = 16;
pub const RIDERS_PER_STRIKE: usize = 8;
pub const EFFECTS_PER_SEQUENCE: usize = 256;
pub const DESCRIPTORS_PER_SEQUENCE: usize = 256;

/// Authoritative current-batch events and compact observer bindings.
pub const EVENTS_PER_BATCH: usize = 65_536;
pub const EVENT_RECEIPTS: usize = 262_144;

/// Retained exact HD choice versions supporting first-earned Intellect thresholds.
pub const EXPERTISE_HISTORY_ROWS: usize = 4096;

#[cfg(test)]
mod tests {
    #[test]
    fn body_origin_occurrence_limit_matches_the_independent_integer_envelope() {
        let mut physical = 0;
        for x in -24_i32..=24 {
            for y in -24_i32..=24 {
                for z in -24_i32..=24 {
                    let distance = [x, y, z]
                        .into_iter()
                        .map(|coordinate| (coordinate.abs() - 8).max(0))
                        .sum::<i32>();
                    physical += usize::from(distance <= 16);
                }
            }
        }
        assert_eq!(physical, 61_617);
        assert_eq!(super::PUBLIC_AIM_OCCURRENCES, physical + 2);
    }
}
