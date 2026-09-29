//! Regions within a few steps of some cells, by any movement, physics or
//! sight step. Region streaming pins them; see
//! `docs/region-streaming.md#pins`. Results are reused while every region a
//! search read is unchanged, so reuse never changes them.
use std::collections::BTreeSet;

use crate::{Direction, Location, RegionId, World};

/// Axis steps, in every direction a body can move, reach or fall.
const AXES: [Direction; 6] = [
    Direction::North,
    Direction::East,
    Direction::South,
    Direction::West,
    Direction::Up,
    Direction::Down,
];

impl World {
    /// Every region a breadth-first search of `steps` axis steps from
    /// `cells` enters, reusing an earlier identical search while the regions
    /// it read are unchanged.
    pub fn reach_regions(&self, cells: &BTreeSet<Location>, steps: usize) -> BTreeSet<RegionId> {
        let key = (cells.iter().copied().collect(), steps);
        if let Some(reached) = self.sight.reach(&key) {
            return reached.into_iter().collect();
        }
        let reached = self.reach_regions_uncached(cells, steps);
        // A search reads the cells it enters and, through links and rim
        // projection, the regions linked from theirs.
        let read: BTreeSet<RegionId> = reached
            .iter()
            .flat_map(|r| std::iter::once(*r).chain(self.linked_regions(*r)))
            .collect();
        self.sight
            .insert_reach(key, read, reached.iter().copied().collect());
        reached
    }

    /// [`World::reach_regions`] without reuse, for tests.
    pub fn reach_regions_uncached(
        &self,
        cells: &BTreeSet<Location>,
        steps: usize,
    ) -> BTreeSet<RegionId> {
        let mut frontier = cells.clone();
        let mut seen = frontier.clone();
        for _ in 0..steps {
            let mut next = BTreeSet::new();
            for at in &frontier {
                for direction in AXES {
                    let found = [
                        self.physics_neighbor(*at, direction).map(|(to, _)| to),
                        self.movement_neighbor(*at, direction).map(|(to, _)| to),
                        self.adjacent(*at, direction),
                    ];
                    for to in found.into_iter().flatten() {
                        if seen.insert(to) {
                            next.insert(to);
                        }
                    }
                }
            }
            frontier = next;
        }
        seen.into_iter().map(|at| at.region).collect()
    }
}
