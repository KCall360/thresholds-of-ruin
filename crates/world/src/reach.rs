//! Regions within a few steps of some cells, by any movement, physics or
//! sight step. Region streaming pins them; see
//! `docs/region-streaming.md#performance`. Results are reused while every
//! region they read is unchanged, so reuse never changes them.
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::Arc;

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
    /// `cells` enters. Cells in one region that are all at least `steps` from
    /// its exits reach only that region, which the region's cached exit
    /// field answers without a search. Otherwise an earlier identical search
    /// is reused while the regions it read are unchanged.
    pub fn reach_regions(&self, cells: &BTreeSet<Location>, steps: usize) -> BTreeSet<RegionId> {
        if let Some(home) = self.stays_home(cells, steps) {
            return BTreeSet::from([home]);
        }
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

    /// The region `cells` are all in, if a search of `steps` from them can't
    /// leave it: every cell is at least `steps` from an exit.
    fn stays_home(&self, cells: &BTreeSet<Location>, steps: usize) -> Option<RegionId> {
        let home = cells.first()?.region;
        if cells.iter().any(|at| at.region != home) || self.region(home).is_none() {
            return None;
        }
        let field = self.exit_field(home, steps);
        cells
            .iter()
            .all(|at| field.get(at).is_none_or(|d| usize::from(*d) >= steps))
            .then_some(home)
    }

    /// For each cell of `region` fewer than `steps` steps from an exit (a
    /// cell with a step into another region), that distance. Built with the
    /// search's own steps, so it's exact however links, rims and walls lie.
    fn exit_field(&self, region: RegionId, steps: usize) -> crate::sight_cache::ExitField {
        if let Some(field) = self.sight.field(region, steps) {
            return field;
        }
        let bounds = self.region(region).expect("loaded region").bounds;
        let origin = bounds.origin();
        let (dx, dy, dz) = bounds.dimensions();
        let mut exits = VecDeque::new();
        let mut from: BTreeMap<Location, Vec<Location>> = BTreeMap::new();
        for x in 0..dx {
            for y in 0..dy {
                for z in 0..dz {
                    let at = Location {
                        region,
                        position: crate::Position {
                            x: origin.x + x,
                            y: origin.y + y,
                            z: origin.z + z,
                        },
                    };
                    let mut exit = false;
                    for to in self.steps_from(at) {
                        if to.region == region {
                            from.entry(to).or_default().push(at);
                        } else {
                            exit = true;
                        }
                    }
                    if exit {
                        exits.push_back((at, 0u8));
                    }
                }
            }
        }
        // Distances to the nearest exit, searching backwards along steps.
        let mut field = BTreeMap::new();
        while let Some((at, distance)) = exits.pop_front() {
            if usize::from(distance) >= steps || field.contains_key(&at) {
                continue;
            }
            field.insert(at, distance);
            for before in from.get(&at).into_iter().flatten() {
                if !field.contains_key(before) {
                    exits.push_back((*before, distance + 1));
                }
            }
        }
        let field = Arc::new(field);
        // The field read this region and, through rim projection, the
        // regions linked from it.
        let read = std::iter::once(region).chain(self.linked_regions(region));
        self.sight.insert_field(region, steps, read, field.clone());
        field
    }

    /// Every cell one step from `at`, by any movement, physics or sight step.
    fn steps_from(&self, at: Location) -> impl Iterator<Item = Location> + '_ {
        AXES.into_iter().flat_map(move |direction| {
            [
                self.physics_neighbor(at, direction).map(|(to, _)| to),
                self.movement_neighbor(at, direction).map(|(to, _)| to),
                self.adjacent(at, direction),
            ]
            .into_iter()
            .flatten()
        })
    }

    /// Compare [`World::reach_regions`] with an uncached search from every
    /// cell of every loaded region. Returns how many cells the exit field
    /// answered, or the first cell where the two disagree.
    pub fn check_reach(&self, steps: usize) -> Result<usize, Location> {
        let mut fielded = 0;
        let regions: Vec<_> = self.loaded_regions().collect();
        for region in regions {
            let bounds = self.region(region).expect("loaded region").bounds;
            let origin = bounds.origin();
            let (dx, dy, dz) = bounds.dimensions();
            for x in 0..dx {
                for y in 0..dy {
                    for z in 0..dz {
                        let cell = Location {
                            region,
                            position: crate::Position {
                                x: origin.x + x,
                                y: origin.y + y,
                                z: origin.z + z,
                            },
                        };
                        let cells = BTreeSet::from([cell]);
                        fielded += usize::from(self.stays_home(&cells, steps).is_some());
                        if self.reach_regions(&cells, steps)
                            != self.reach_regions_uncached(&cells, steps)
                        {
                            return Err(cell);
                        }
                    }
                }
            }
        }
        Ok(fielded)
    }

    /// [`World::reach_regions`] without the field or reuse, for tests.
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
                for to in self.steps_from(*at) {
                    if seen.insert(to) {
                        next.insert(to);
                    }
                }
            }
            frontier = next;
        }
        seen.into_iter().map(|at| at.region).collect()
    }
}
