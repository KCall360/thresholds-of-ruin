//! Bounded physical scope point projection, independent of disclosure policy.
//!
//! World owns all physical constraints. Scratch transforms are never persisted;
//! output component ordinals are local to this projection, not action handles.
//! Distinct occurrences remain distinct even when projected positions coincide.
//! Caller provides canonical, validated scope members and lawful sample points.
use tor_world::{compose_rotation, inverse_rotation, rotate_vector, Location, RegionId, World};

#[derive(Clone, Copy, Debug)]
pub struct Budget {
    /// Structural traversal/lookup and allocation-initialization work units.
    pub work: usize,
    /// Sum of requested Vec backing-store bytes, excluding allocator metadata.
    pub allocation_bytes: usize,
    pub regions: usize,
    /// Two directed equality constraints per retained physical passage.
    pub directed_constraints: usize,
    pub samples: usize,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Budget,
    Allocation,
    InvalidSource,
    Arithmetic,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Alignment {
    Aligned,
    Inconsistent,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Component {
    pub alignment: Alignment,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sample {
    /// Chart-local ordinal only; not a region, entity, or navigation handle.
    pub component: usize,
    /// No convenient placement survives an inconsistent connected component.
    pub position: Option<[i64; 3]>,
}
/// Logical primitive work/backing allocation consumed from the supplied budget.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Usage {
    pub work: usize,
    pub allocation_bytes: usize,
}
#[derive(Debug, PartialEq, Eq)]
pub struct Projection {
    pub usage: Usage,
    pub components: Vec<Component>,
    pub samples: Vec<Sample>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Frame {
    rotation: u8,
    translation: [i64; 3],
}
#[derive(Clone, Copy)]
struct Edge {
    from: usize,
    to: usize,
    frame: Frame,
}
fn charge(left: &mut usize, amount: usize) -> Result<(), Error> {
    *left = left.checked_sub(amount).ok_or(Error::Budget)?;
    Ok(())
}
fn reserve<T>(n: usize, b: &mut Budget) -> Result<Vec<T>, Error> {
    charge(&mut b.work, n)?; // initialization/copy/push allowance for these slots
    charge(
        &mut b.allocation_bytes,
        n.checked_mul(std::mem::size_of::<T>())
            .ok_or(Error::Budget)?,
    )?;
    let mut v = Vec::new();
    v.try_reserve_exact(n).map_err(|_| Error::Allocation)?;
    Ok(v)
}
fn add(a: [i64; 3], b: [i64; 3]) -> Result<[i64; 3], Error> {
    Ok([
        a[0].checked_add(b[0]).ok_or(Error::Arithmetic)?,
        a[1].checked_add(b[1]).ok_or(Error::Arithmetic)?,
        a[2].checked_add(b[2]).ok_or(Error::Arithmetic)?,
    ])
}
fn neg(a: [i64; 3]) -> Result<[i64; 3], Error> {
    Ok([
        a[0].checked_neg().ok_or(Error::Arithmetic)?,
        a[1].checked_neg().ok_or(Error::Arithmetic)?,
        a[2].checked_neg().ok_or(Error::Arithmetic)?,
    ])
}
fn rotate(r: u8, a: [i64; 3]) -> Result<[i64; 3], Error> {
    if r >= 24 {
        return Err(Error::InvalidSource);
    }
    // Signed permutation without ever negating MIN inside rotate_vector.
    let mut out = [0; 3];
    for axis in 0..3 {
        let mut unit = [0; 3];
        unit[axis] = 1;
        let basis = rotate_vector(r, unit);
        for j in 0..3 {
            if basis[j] != 0 {
                out[j] = if basis[j] == 1 {
                    a[axis]
                } else {
                    a[axis].checked_neg().ok_or(Error::Arithmetic)?
                };
            }
        }
    }
    Ok(out)
}
impl Frame {
    fn apply(self, p: [i64; 3]) -> Result<[i64; 3], Error> {
        add(rotate(self.rotation, p)?, self.translation)
    }
    fn after(self, inner: Self) -> Result<Self, Error> {
        Ok(Self {
            rotation: compose_rotation(inner.rotation, self.rotation),
            translation: self.apply(inner.translation)?,
        })
    }
    fn inverse(self) -> Result<Self, Error> {
        let rotation = inverse_rotation(self.rotation);
        Ok(Self {
            rotation,
            translation: rotate(rotation, neg(self.translation)?)?,
        })
    }
}
fn vector(at: Location) -> [i64; 3] {
    [
        i64::from(at.position.x),
        i64::from(at.position.y),
        i64::from(at.position.z),
    ]
}
fn lookup(members: &[RegionId], r: RegionId, b: &mut Budget) -> Result<Option<usize>, Error> {
    charge(
        &mut b.work,
        usize::BITS as usize - members.len().leading_zeros() as usize + 1,
    )?;
    Ok(members.binary_search(&r).ok())
}

// Existing public World API suffices: explicit vertical passages are physical
// exactly when !is_stair; named transport stairs are absent from exits().
fn candidates(
    world: &World,
    region: RegionId,
) -> impl Iterator<Item = (&tor_world::Passage, u8, bool)> {
    world.exits(region).map(move |p| {
        (
            p,
            world.crossing_rotation(p.from, p.direction),
            !world.is_stair(p.from, p.direction),
        )
    })
}

/// Output has no source identities or edges. Do not serialize scratch frames.
/// Inputs remain borrowed/immutable across counting, construction and traversal.
/// All errors are atomic: no partial Projection escapes.
///
/// Members must be sorted, unique and resident. Point occurrence order and
/// multiplicity are preserved. Physical region alignment is not a disclosure
/// grant or an injective embedding: coincident rows must not be deduplicated.
/// This validates region containment, not membership in a particular logical
/// room/passage; the caller must select lawful samples from its canonical scope.
/// Budget bytes cover requested Vec storage, not allocator metadata/rounding.
/// Work measures structural operations, with World lookups retaining their
/// existing logarithmic cost; callers own the admitted-source resource proof.
pub fn project(
    world: &World,
    members: &[RegionId],
    points: &[Location],
    mut b: Budget,
) -> Result<Projection, Error> {
    let initial_budget = b;
    if members.len() > b.regions || points.len() > b.samples {
        return Err(Error::Budget);
    }
    for (i, r) in members.iter().enumerate() {
        charge(&mut b.work, 1)?;
        if (i > 0 && members[i - 1] >= *r) || world.region(*r).is_none() {
            return Err(Error::InvalidSource);
        }
    }
    let mut count = 0usize;
    // Count every candidate, not just accepted joins. External connections are
    // not alignment bridges through an unauthorized scope.
    for r in members {
        charge(&mut b.work, 1)?;
        for (p, rotation, physical) in candidates(world, *r) {
            charge(&mut b.work, 1)?;
            if rotation >= 24 || p.from.region != *r || !world.contains(p.from) {
                return Err(Error::InvalidSource);
            }
            if physical && lookup(members, p.to.region, &mut b)?.is_some() {
                if !world.contains(p.to) {
                    return Err(Error::InvalidSource);
                }
                count = count.checked_add(2).ok_or(Error::Budget)?;
                if count > b.directed_constraints {
                    return Err(Error::Budget);
                }
            }
        }
    }
    let mut edges = reserve::<Edge>(count, &mut b)?;
    for (from, r) in members.iter().enumerate() {
        charge(&mut b.work, 1)?;
        for (p, crossing, physical) in candidates(world, *r) {
            charge(&mut b.work, 1)?;
            if !physical {
                continue;
            }
            let Some(to) = lookup(members, p.to.region, &mut b)? else {
                continue;
            };
            // Crossing maps source axes -> destination axes. Thus destination
            // coordinates map back through inverse(crossing), with adjacent
            // source anchor at p.from + direction.delta, NOT at p.from itself.
            let rotation = inverse_rotation(crossing);
            let (x, y, z) = p.direction.delta();
            let adjacent = add(vector(p.from), [i64::from(x), i64::from(y), i64::from(z)])?;
            let frame = Frame {
                rotation,
                translation: add(adjacent, neg(rotate(rotation, vector(p.to))?)?)?,
            };
            edges.push(Edge { from, to, frame });
            // Reverse constraint is geometric equality, NOT a fabricated
            // reverse movement passage or action permission.
            edges.push(Edge {
                from: to,
                to: from,
                frame: frame.inverse()?,
            });
        }
    }
    // Counting-sort adjacency avoids unmetered comparison sorting.
    let n = members.len();
    let mut offsets = reserve::<usize>(n.checked_add(1).ok_or(Error::Budget)?, &mut b)?;
    offsets.resize(n + 1, 0);
    for e in &edges {
        charge(&mut b.work, 1)?;
        offsets[e.from + 1] += 1;
    }
    for i in 0..n {
        charge(&mut b.work, 1)?;
        offsets[i + 1] = offsets[i + 1]
            .checked_add(offsets[i])
            .ok_or(Error::Budget)?;
    }
    let mut cursor = reserve::<usize>(n, &mut b)?;
    cursor.extend_from_slice(&offsets[..n]);
    let mut order = reserve::<usize>(count, &mut b)?;
    order.resize(count, 0);
    for (i, e) in edges.iter().enumerate() {
        charge(&mut b.work, 1)?;
        order[cursor[e.from]] = i;
        cursor[e.from] += 1;
    }
    let mut frames = reserve::<Option<Frame>>(n, &mut b)?;
    frames.resize(n, None);
    let mut groups = reserve::<usize>(n, &mut b)?;
    groups.resize(n, 0);
    let mut queue = reserve::<usize>(n, &mut b)?;
    let mut components = reserve::<Component>(n, &mut b)?;
    for seed in 0..n {
        charge(&mut b.work, 1)?;
        if frames[seed].is_some() {
            continue;
        }
        let component = components.len();
        components.push(Component {
            alignment: Alignment::Aligned,
        });
        frames[seed] = Some(Frame {
            rotation: 0,
            translation: [0; 3],
        });
        groups[seed] = component;
        queue.clear();
        queue.push(seed);
        let mut head = 0;
        while head < queue.len() {
            charge(&mut b.work, 1)?;
            let from = queue[head];
            head += 1;
            let parent = frames[from].ok_or(Error::InvalidSource)?;
            for slot in offsets[from]..offsets[from + 1] {
                charge(&mut b.work, 1)?;
                let e = edges[order[slot]];
                let expected = parent.after(e.frame)?;
                match frames[e.to] {
                    Some(actual) if actual != expected => {
                        components[component].alignment = Alignment::Inconsistent
                    }
                    Some(_) => {}
                    None => {
                        frames[e.to] = Some(expected);
                        groups[e.to] = component;
                        queue.push(e.to);
                    }
                }
            }
        }
    }
    let mut samples = reserve::<Sample>(points.len(), &mut b)?;
    for p in points {
        charge(&mut b.work, 1)?;
        if !world.contains(*p) {
            return Err(Error::InvalidSource);
        }
        let i = lookup(members, p.region, &mut b)?.ok_or(Error::InvalidSource)?;
        let component = groups[i];
        let position = if components[component].alignment == Alignment::Aligned {
            Some(frames[i].ok_or(Error::InvalidSource)?.apply(vector(*p))?)
        } else {
            None
        };
        samples.push(Sample {
            component,
            position,
        });
    }
    Ok(Projection {
        usage: Usage {
            work: initial_budget.work - b.work,
            allocation_bytes: initial_budget.allocation_bytes - b.allocation_bytes,
        },
        components,
        samples,
    })
}

#[cfg(test)]
mod tests;
