//! Pure spatial classifications, independent of population and physical geometry.
//!
//! These types do not certify canonical room volumes or doorway affiliations.
//! A later source verifier must establish those facts; World alone owns physical
//! adjacency and portal transforms. No classification creates a residency pin.

pub mod codec;
pub mod registry;
mod retention;

use std::num::NonZeroU32;

/// Room and passage identifiers deliberately occupy separate namespaces.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct RoomId(NonZeroU32);
impl RoomId {
    pub fn new(value: u32) -> Option<Self> {
        NonZeroU32::new(value).map(Self)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct PassageId(NonZeroU32);
impl PassageId {
    pub fn new(value: u32) -> Option<Self> {
        NonZeroU32::new(value).map(Self)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrimarySpace {
    Room(RoomId),
    Passage(PassageId),
}

/// A doorway retains its primary room and names its adjoining passage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Membership {
    primary: PrimarySpace,
    doorway: Option<PassageId>,
}
impl Membership {
    pub fn new(primary: PrimarySpace, doorway: Option<PassageId>) -> Option<Self> {
        if doorway.is_some() && matches!(primary, PrimarySpace::Passage(_)) {
            return None;
        }
        Some(Self { primary, doorway })
    }

    /// Directional Drain membership, not a symmetric affiliation intersection.
    /// Caller separately excludes self, dead actors and actors without Health;
    /// undead creatures with Health remain eligible.
    pub fn includes_drain_target(self, target: Self) -> bool {
        self.primary == target.primary
            || self
                .doorway
                .is_some_and(|passage| target.primary == PrimarySpace::Passage(passage))
            || matches!(self.primary, PrimarySpace::Passage(passage)
                if target.doorway == Some(passage))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn places() -> [Membership; 7] {
        let r1 = RoomId::new(1).unwrap();
        let r2 = RoomId::new(2).unwrap();
        let p1 = PassageId::new(1).unwrap();
        let p2 = PassageId::new(2).unwrap();
        [
            Membership::new(PrimarySpace::Room(r1), None).unwrap(),
            Membership::new(PrimarySpace::Room(r2), None).unwrap(),
            Membership::new(PrimarySpace::Passage(p1), None).unwrap(),
            Membership::new(PrimarySpace::Passage(p2), None).unwrap(),
            Membership::new(PrimarySpace::Room(r1), Some(p1)).unwrap(),
            Membership::new(PrimarySpace::Room(r1), Some(p2)).unwrap(),
            Membership::new(PrimarySpace::Room(r2), Some(p1)).unwrap(),
        ]
    }

    #[test]
    fn classification_rejects_zero_ids_and_passage_owned_doorways() {
        assert!(RoomId::new(0).is_none());
        assert!(PassageId::new(0).is_none());
        let p = PassageId::new(u32::MAX).unwrap();
        assert!(Membership::new(PrimarySpace::Passage(p), Some(p)).is_none());
        assert!(
            Membership::new(PrimarySpace::Room(RoomId::new(u32::MAX).unwrap()), Some(p)).is_some()
        );
        assert_ne!(
            PrimarySpace::Room(RoomId::new(1).unwrap()),
            PrimarySpace::Passage(PassageId::new(1).unwrap())
        );
    }

    #[test]
    fn drain_matches_the_directional_room_passage_doorway_matrix() {
        // Pinned Rogue sticks.c c7c119f, WS_DRAIN: same primary, caster
        // doorway passage, or passage caster to matching target doorway.
        // Order: room1, room2, passage1, passage2, doorR1P1, doorR1P2, doorR2P1.
        let expected = [
            [true, false, false, false, true, true, false],
            [false, true, false, false, false, false, true],
            [false, false, true, false, true, false, true],
            [false, false, false, true, false, true, false],
            [true, false, true, false, true, true, false],
            [true, false, false, true, true, true, false],
            [false, true, true, false, false, false, true],
        ];
        for (source, from) in places().into_iter().enumerate() {
            for (target, to) in places().into_iter().enumerate() {
                assert_eq!(
                    from.includes_drain_target(to),
                    expected[source][target],
                    "source={source} target={target}"
                );
            }
        }
    }

    #[test]
    fn shared_doorway_affiliation_does_not_make_drain_symmetric() {
        let [_, _, passage, _, first_door, _, other_door] = places();
        assert!(passage.includes_drain_target(other_door));
        assert!(first_door.includes_drain_target(passage));
        assert!(!first_door.includes_drain_target(other_door));
    }
}
