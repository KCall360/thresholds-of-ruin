//! Reusable geometry facts for exact sight and perceived navigation links.
use crate::{Direction, Location, RegionId, World};

/// A region's storage bounds and exit planes. A step is plain (an ordinary
/// neighbour in the same region) unless it leaves storage or starts on an
/// exit plane in that exit's direction. Only those steps can be redirected by
/// a passage or by rim projection, so only they need topology lookups.
pub(crate) struct RegionInfo {
    pub(crate) id: RegionId,
    pub(crate) lo: [i64; 3],
    pub(crate) hi: [i64; 3],
    /// Axis, sign, and plane coordinate of each exit.
    pub(crate) exits: Vec<(usize, i64, i64)>,
    /// Where the exits lead. Rim projection reads terrain there even when the
    /// step stays in this region, so a scene depends on these regions too.
    pub(crate) neighbours: Vec<RegionId>,
    pub(crate) transparent: Option<crate::Extent>,
}

impl RegionInfo {
    pub(crate) fn new(world: &World, id: RegionId) -> Option<Self> {
        let bounds = world.region(id)?.bounds;
        let (w, d, h) = bounds.dimensions();
        let lo = [bounds.origin.x, bounds.origin.y, bounds.origin.z].map(i64::from);
        let mut neighbours: Vec<_> = world.exits(id).map(|p| p.to.region).collect();
        neighbours.sort();
        neighbours.dedup();
        let mut exits: Vec<_> = world
            .exits(id)
            .filter_map(|p| {
                let (axis, sign) = axis_sign(p.direction)?;
                let from = p.from.position;
                Some((axis, sign, i64::from([from.x, from.y, from.z][axis])))
            })
            .collect();
        // An area passage has one endpoint per cell, but plane proofs depend
        // only on axis, direction and coordinate, not endpoint multiplicity.
        exits.sort_unstable();
        exits.dedup();
        Some(Self {
            id,
            lo,
            hi: [
                lo[0] + i64::from(w) - 1,
                lo[1] + i64::from(d) - 1,
                lo[2] + i64::from(h) - 1,
            ],
            exits,
            neighbours,
            transparent: world.transparent_box(id),
        })
    }

    /// A centre or exposed enclosing face reached through convex empty space.
    pub(crate) fn unoccluded(&self, eye: crate::Position, target: crate::Position) -> bool {
        let Some(bounds) = self.transparent else {
            return false;
        };
        if !bounds.contains(eye) {
            return false;
        }
        let origin = bounds.origin;
        let (w, d, h) = bounds.dimensions();
        let lo = [origin.x, origin.y, origin.z].map(i64::from);
        let hi = [w, d, h].map(i64::from);
        let target = [target.x, target.y, target.z].map(i64::from);
        let mut outside = 0;
        for axis in 0..3 {
            let upper = lo[axis] + hi[axis] - 1;
            if target[axis] < lo[axis] || target[axis] > upper {
                if target[axis] != lo[axis] - 1 && target[axis] != upper + 1 {
                    return false;
                }
                outside += 1;
            }
        }
        // One face is exposed to the box. Edge/corner cells need exact rays.
        outside <= 1
    }

    pub(crate) fn plain(&self, at: [i64; 3], axis: usize, sign: i64) -> bool {
        let next = at[axis] + sign;
        next >= self.lo[axis]
            && next <= self.hi[axis]
            && !self
                .exits
                .iter()
                .any(|&(a, s, c)| a == axis && s == sign && c == at[axis])
    }
}

pub(crate) fn axis_sign(direction: Direction) -> Option<(usize, i64)> {
    let (dx, dy, dz) = direction.delta();
    let delta = [dx, dy, dz];
    let axis = delta.iter().position(|v| *v != 0)?;
    (delta.iter().filter(|v| **v != 0).count() == 1).then_some((axis, i64::from(delta[axis])))
}

/// A read-only adjacency resolver for one batch of already perceived cells.
/// It reuses region bounds and exit planes. Callers validate source and target
/// terrain separately; a resolved link alone is not movement permission.
pub struct Adjacency<'a> {
    world: &'a World,
    regions: Vec<RegionInfo>,
}
impl World {
    pub fn adjacency(&self) -> Adjacency<'_> {
        Adjacency {
            world: self,
            regions: Vec::new(),
        }
    }
}
impl Adjacency<'_> {
    /// Cardinal topology from a validated loaded source, including abstract
    /// stairs. Redirected boundary steps keep the ordinary world resolver.
    pub fn resolve(&mut self, from: Location, direction: Direction) -> Option<(Location, u8)> {
        let (axis, sign) = axis_sign(direction)?;
        if !self.world.is_stair(from, direction) {
            let index = match self
                .regions
                .iter()
                .position(|region| region.id == from.region)
            {
                Some(index) => index,
                None => {
                    self.regions.push(RegionInfo::new(self.world, from.region)?);
                    self.regions.len() - 1
                }
            };
            let p = from.position;
            let at = [p.x, p.y, p.z].map(i64::from);
            let region = &self.regions[index];
            if (0..3).all(|axis| at[axis] >= region.lo[axis] && at[axis] <= region.hi[axis])
                && region.plain(at, axis, sign)
            {
                return Some((
                    Location {
                        position: direction.offset(p)?,
                        ..from
                    },
                    0,
                ));
            }
        }
        self.world
            .adjacent(from, direction)
            .map(|to| (to, self.world.crossing_rotation(from, direction)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn area_passages_share_one_exit_plane() {
        let mut world = World::new(vec![], vec![]).unwrap();
        for id in [1, 2] {
            world
                .add_region(crate::Region {
                    id: RegionId(id),
                    name: String::new(),
                    bounds: crate::Extent::new(20, 20, 4).unwrap(),
                })
                .unwrap();
        }
        let at = |region, x| Location {
            region: RegionId(region),
            position: crate::Position { x, y: 0, z: 0 },
        };
        world
            .connect_area(
                crate::Passage {
                    from: at(1, 19),
                    direction: Direction::East,
                    to: at(2, 0),
                },
                0,
                20,
                4,
            )
            .unwrap();
        let info = RegionInfo::new(&world, RegionId(1)).unwrap();
        assert_eq!(info.exits, vec![(0, 1, 19)]);
        assert!(info.plain([18, 10, 2], 0, 1));
        assert!(!info.plain([19, 10, 2], 0, 1));
    }
}
