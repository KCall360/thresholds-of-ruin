//! Independent strict history pool schema; existing World/spatial pools own geometry.
use super::*;
use serde::de::{DeserializeSeed, Error as _, MapAccess, SeqAccess, Visitor};
use serde::{Deserializer, Serializer};
use std::{fmt, marker::PhantomData};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Alignment {
    Aligned,
    Inconsistent,
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(deny_unknown_fields)]
struct Point {
    source: Location,
    component: usize,
    #[serde(deserialize_with = "crate::bounded::required_option")]
    position: Option<[i64; 3]>,
    #[serde(deserialize_with = "crate::bounded::required_option")]
    mapping: Option<MappingProof>,
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(deny_unknown_fields)]
struct Root {
    kind: Kind,
    acquired_tick: u64,
    scope: String,
    world: usize,
    spatial: usize,
    components: Vec<Alignment>,
    points: Vec<Point>,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(deny_unknown_fields)]
struct SlotRef {
    #[serde(deserialize_with = "crate::bounded::required_option")]
    latest: Option<usize>,
    #[serde(deserialize_with = "crate::bounded::required_option")]
    attempt: Option<Attempt>,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(deny_unknown_fields)]
struct Slots {
    food: SlotRef,
    magic: SlotRef,
    mapping: SlotRef,
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(deny_unknown_fields)]
struct Binding {
    actor: ActorId,
    slots: Slots,
}
#[derive(Default)]
pub(crate) struct Pool {
    roots: Vec<Root>,
    owners: Vec<Vec<Binding>>,
    captured: Vec<Shared<HistoricalChart>>,
}
/// Restore-pass cache. Indices belong only to this exact immutable Pool namespace.
/// Representative indices retain no second geometry graph or cloned scope text.
pub(crate) struct RestoreCache {
    roots: BTreeMap<usize, Shared<HistoricalChart>>,
    mapping: Vec<usize>,
    memo_work_left: usize,
    max_scope_bytes: usize,
    #[cfg(test)]
    mapping_replays: usize,
}
impl Default for RestoreCache {
    fn default() -> Self {
        Self {
            roots: BTreeMap::new(),
            mapping: Vec::new(),
            memo_work_left: crate::checkpoint::ARCHIVE_BYTE_LIMIT,
            max_scope_bytes: 0,
            #[cfg(test)]
            mapping_replays: 0,
        }
    }
}
impl RestoreCache {
    fn pay_lookup(&mut self, scope_bytes: usize, insertion: bool) -> bool {
        let comparisons = usize::BITS as usize - self.mapping.len().leading_zeros() as usize + 1;
        let cost = self
            .max_scope_bytes
            .max(scope_bytes)
            .checked_add(3)
            .and_then(|n| n.checked_mul(comparisons))
            .and_then(|n| {
                if insertion {
                    self.mapping
                        .len()
                        .checked_mul(2)
                        .and_then(|moves| n.checked_add(moves))
                        .and_then(|n| n.checked_add(1))
                } else {
                    Some(n)
                }
            });
        let Some(left) = cost.and_then(|n| self.memo_work_left.checked_sub(n)) else {
            self.memo_work_left = 0;
            return false;
        };
        self.memo_work_left = left;
        true
    }
    fn mapping_proof(&mut self, pool: &Pool, root: &Root) -> Option<usize> {
        if root.kind != Kind::Mapping || !self.pay_lookup(root.scope.len(), false) {
            return None;
        }
        self.mapping
            .binary_search_by(|i| {
                let old = &pool.roots[*i];
                (old.world, old.spatial, old.scope.as_str()).cmp(&(
                    root.world,
                    root.spatial,
                    root.scope.as_str(),
                ))
            })
            .ok()
            .map(|i| self.mapping[i])
    }
    fn remember_mapping(&mut self, pool: &Pool, index: usize) {
        let root = &pool.roots[index];
        // Precharge comparisons and worst-case index moves/reallocation copies.
        // Work exhaustion disables this optional cache, never root validation.
        if !self.pay_lookup(root.scope.len(), true) {
            return;
        }
        let position = match self.mapping.binary_search_by(|i| {
            let old = &pool.roots[*i];
            (old.world, old.spatial, old.scope.as_str()).cmp(&(
                root.world,
                root.spatial,
                root.scope.as_str(),
            ))
        }) {
            Ok(_) => return,
            Err(i) => i,
        };
        // Pool structure already charges >=256 bytes per root; one usize per
        // representative is inside that bound. Allocation failure merely skips
        // this optimization; it never admits an unverified sample or adds a cap.
        if self.mapping.try_reserve_exact(1).is_ok() {
            self.mapping.insert(position, index);
            self.max_scope_bytes = self.max_scope_bytes.max(root.scope.len());
        }
    }
}
struct Meter {
    left: usize,
}
impl Meter {
    fn charge<E: serde::de::Error>(&mut self, n: usize) -> Result<(), E> {
        self.left = self
            .left
            .checked_sub(n)
            .ok_or_else(|| E::custom("history archive capacity"))?;
        Ok(())
    }
}
trait Entry: Sized {
    fn read<'de, D: Deserializer<'de>>(d: D, m: &mut Meter) -> Result<Self, D::Error>;
}
struct Seed<'a, T>(&'a mut Meter, PhantomData<T>);
impl<'de, T: Entry> DeserializeSeed<'de> for Seed<'_, T> {
    type Value = T;
    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<T, D::Error> {
        T::read(d, self.0)
    }
}
struct List<'a, T>(&'a mut Meter, PhantomData<T>);
impl<'de, T: Entry> DeserializeSeed<'de> for List<'_, T> {
    type Value = Vec<T>;
    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<Vec<T>, D::Error> {
        struct V<'a, T>(&'a mut Meter, PhantomData<T>);
        impl<'de, T: Entry> Visitor<'de> for V<'_, T> {
            type Value = Vec<T>;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("bounded history sequence")
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut a: A) -> Result<Vec<T>, A::Error> {
                let mut result = Vec::new();
                while let Some(value) = a.next_element_seed(Seed(self.0, PhantomData::<T>))? {
                    result
                        .try_reserve(1)
                        .map_err(|_| A::Error::custom("history allocation"))?;
                    result.push(value);
                }
                Ok(result)
            }
        }
        d.deserialize_seq(V(self.0, PhantomData))
    }
}
macro_rules! fixed_entry {
    ($ty:ty,$cost:expr) => {
        impl Entry for $ty {
            fn read<'de, D: Deserializer<'de>>(d: D, m: &mut Meter) -> Result<Self, D::Error> {
                m.charge($cost)?;
                Self::deserialize(d)
            }
        }
    };
}
fixed_entry!(Alignment, 32);
impl Entry for Point {
    fn read<'de, D: Deserializer<'de>>(d: D, m: &mut Meter) -> Result<Self, D::Error> {
        m.charge(256)?;
        let point = Self::deserialize(d)?;
        if point.mapping.is_some() {
            m.charge(256)?;
        }
        Ok(point)
    }
}
// Three independently timed slots, including the longest unavailable reason.
fixed_entry!(Binding, 512);
impl Entry for Vec<Binding> {
    fn read<'de, D: Deserializer<'de>>(d: D, m: &mut Meter) -> Result<Self, D::Error> {
        m.charge(16)?;
        List(m, PhantomData).deserialize(d)
    }
}
struct Text<'a>(&'a mut Meter);
impl<'de> DeserializeSeed<'de> for Text<'_> {
    type Value = String;
    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<String, D::Error> {
        struct V<'a>(&'a mut Meter);
        impl Visitor<'_> for V<'_> {
            type Value = String;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("bounded scope label")
            }
            fn visit_str<E: serde::de::Error>(self, s: &str) -> Result<String, E> {
                if s.is_empty() || s.chars().any(char::is_control) {
                    return Err(E::custom("invalid history scope"));
                }
                self.0.charge(
                    s.len()
                        .checked_mul(6)
                        .ok_or_else(|| E::custom("history text overflow"))?,
                )?;
                Ok(s.to_owned())
            }
        }
        d.deserialize_str(V(self.0))
    }
}
impl Entry for Root {
    fn read<'de, D: Deserializer<'de>>(d: D, m: &mut Meter) -> Result<Self, D::Error> {
        m.charge(256)?;
        #[derive(Deserialize)]
        #[serde(field_identifier, rename_all = "snake_case")]
        enum Field {
            Kind,
            AcquiredTick,
            Scope,
            World,
            Spatial,
            Components,
            Points,
        }
        struct V<'a>(&'a mut Meter);
        impl<'de> Visitor<'de> for V<'_> {
            type Value = Root;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("historical chart root")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut a: A) -> Result<Root, A::Error> {
                let (
                    mut kind,
                    mut tick,
                    mut scope,
                    mut world,
                    mut spatial,
                    mut components,
                    mut points,
                ) = (None, None, None, None, None, None, None);
                while let Some(field) = a.next_key::<Field>()? {
                    match field {
                        Field::Kind if kind.is_none() => kind = Some(a.next_value()?),
                        Field::AcquiredTick if tick.is_none() => tick = Some(a.next_value()?),
                        Field::Scope if scope.is_none() => {
                            scope = Some(a.next_value_seed(Text(self.0))?)
                        }
                        Field::World if world.is_none() => world = Some(a.next_value()?),
                        Field::Spatial if spatial.is_none() => spatial = Some(a.next_value()?),
                        Field::Components if components.is_none() => {
                            components = Some(a.next_value_seed(List(self.0, PhantomData))?)
                        }
                        Field::Points if points.is_none() => {
                            points = Some(a.next_value_seed(List(self.0, PhantomData))?)
                        }
                        _ => return Err(A::Error::custom("duplicate history field")),
                    }
                }
                Ok(Root {
                    kind: kind.ok_or_else(|| A::Error::missing_field("kind"))?,
                    acquired_tick: tick.ok_or_else(|| A::Error::missing_field("acquired_tick"))?,
                    scope: scope.ok_or_else(|| A::Error::missing_field("scope"))?,
                    world: world.ok_or_else(|| A::Error::missing_field("world"))?,
                    spatial: spatial.ok_or_else(|| A::Error::missing_field("spatial"))?,
                    components: components.ok_or_else(|| A::Error::missing_field("components"))?,
                    points: points.ok_or_else(|| A::Error::missing_field("points"))?,
                })
            }
        }
        d.deserialize_map(V(m))
    }
}
impl<'de> Deserialize<'de> for Pool {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(field_identifier, rename_all = "snake_case")]
        enum Field {
            Roots,
            Owners,
        }
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Pool;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("history payload pool")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut a: A) -> Result<Pool, A::Error> {
                let mut m = Meter {
                    left: crate::checkpoint::ARCHIVE_BYTE_LIMIT,
                };
                let (mut roots, mut owners) = (None, None);
                while let Some(field) = a.next_key::<Field>()? {
                    match field {
                        Field::Roots if roots.is_none() => {
                            roots = Some(a.next_value_seed(List(&mut m, PhantomData))?)
                        }
                        Field::Owners if owners.is_none() => {
                            owners = Some(a.next_value_seed(List(&mut m, PhantomData))?)
                        }
                        _ => return Err(A::Error::custom("duplicate history pool field")),
                    }
                }
                let value = Pool {
                    roots: roots.ok_or_else(|| A::Error::missing_field("roots"))?,
                    owners: owners.ok_or_else(|| A::Error::missing_field("owners"))?,
                    captured: vec![],
                };
                value
                    .structure()
                    .map_err(|_| A::Error::custom("invalid history pool"))?;
                Ok(value)
            }
        }
        d.deserialize_map(V)
    }
}
impl Pool {
    fn structure(&self) -> Result<(), Error> {
        let mut bytes = crate::checkpoint::ARCHIVE_BYTE_LIMIT;
        let mut unique_roots = BTreeSet::new();
        let mut unique_directories = BTreeSet::new();
        for root in &self.roots {
            let cost = root
                .points
                .iter()
                .try_fold(0usize, |n, p| {
                    n.checked_add(if p.mapping.is_some() { 512 } else { 256 })
                })
                .and_then(|n| {
                    root.components
                        .len()
                        .checked_mul(32)
                        .and_then(|c| n.checked_add(c))
                })
                .and_then(|n| {
                    root.scope
                        .len()
                        .checked_mul(6)
                        .and_then(|s| n.checked_add(s))
                })
                .and_then(|n| n.checked_add(256))
                .ok_or(Error::Capacity)?;
            bytes = bytes.checked_sub(cost).ok_or(Error::Capacity)?;
            if root.points.is_empty()
                || root.components.is_empty()
                || root.points.iter().any(|p| {
                    (root.kind == Kind::Mapping) != p.mapping.is_some()
                        || p.mapping.as_ref().is_some_and(|m| m.decode().is_err())
                })
                || root.scope.is_empty()
                || root.scope.chars().any(char::is_control)
            {
                return Err(Error::Structure);
            }
            if !unique_roots.insert(root) {
                return Err(Error::Structure);
            }
        }
        for dir in &self.owners {
            bytes = bytes
                .checked_sub(
                    dir.len()
                        .checked_mul(512)
                        .and_then(|n| n.checked_add(16))
                        .ok_or(Error::Capacity)?,
                )
                .ok_or(Error::Capacity)?;
            if dir.windows(2).any(|w| w[0].actor >= w[1].actor)
                || dir.iter().any(|b| b.actor.0 == 0)
            {
                return Err(Error::Structure);
            }
            if !unique_directories.insert(dir) {
                return Err(Error::Structure);
            }
            for binding in dir {
                if binding.slots.food.attempt.is_none()
                    && binding.slots.magic.attempt.is_none()
                    && binding.slots.mapping.attempt.is_none()
                {
                    return Err(Error::Structure);
                }
                for (kind, slot) in [
                    (Kind::Food, binding.slots.food),
                    (Kind::Magic, binding.slots.magic),
                    (Kind::Mapping, binding.slots.mapping),
                ] {
                    match (slot.attempt, slot.latest) {
                        (None, None) => {}
                        (Some(a), None) if !matches!(a.status, AttemptStatus::Positive) => {}
                        (Some(a), Some(i)) => {
                            let r = self.roots.get(i).ok_or(Error::Reference)?;
                            if r.kind != kind
                                || r.acquired_tick > a.attempted_at
                                || matches!(a.status, AttemptStatus::Positive)
                                    && r.acquired_tick != a.attempted_at
                            {
                                return Err(Error::Reference);
                            }
                        }
                        _ => return Err(Error::Reference),
                    }
                }
            }
        }
        Ok(())
    }
    pub(crate) fn capture(
        &mut self,
        owners: &BTreeMap<ActorId, Owner>,
        worlds: &mut Vec<World>,
        spatial: &mut crate::spatial_scope::retention::PoolState,
    ) -> usize {
        let mut directory = Vec::new();
        for (&actor, owner) in owners {
            let mut slot = |value: &Slot| -> SlotRef {
                let latest = value.latest.as_ref().map(|root| {
                    let world = worlds
                        .iter()
                        .position(|w| w == &*root.authority.world)
                        .unwrap_or_else(|| {
                            worlds.push((*root.authority.world).clone());
                            worlds.len() - 1
                        });
                    // One actual retained reference per distinct history root below,
                    // not per owner. Avoid adding unbound spatial references here.
                    if let Some(index) = self
                        .captured
                        .iter()
                        .position(|old| old.shares_storage(root) || old == root)
                    {
                        return index;
                    }
                    let spatial = spatial.capture(&root.authority.registry);
                    let stored = Root {
                        kind: root.kind,
                        acquired_tick: root.acquired_tick,
                        scope: root.scope.clone(),
                        world,
                        spatial,
                        components: root
                            .chart
                            .components()
                            .iter()
                            .map(|c| match c.alignment {
                                scope_projection::Alignment::Aligned => Alignment::Aligned,
                                scope_projection::Alignment::Inconsistent => {
                                    Alignment::Inconsistent
                                }
                            })
                            .collect(),
                        points: root
                            .chart
                            .rows()
                            .iter()
                            .enumerate()
                            .map(|(i, row)| {
                                let (source, mapping) = match root.source.as_ref() {
                                    SourceProof::Points(points) => (points[i], None),
                                    SourceProof::Mapping(points) => (
                                        points[i].location,
                                        Some(MappingProof::capture(points[i].mark, &row.mark)),
                                    ),
                                };
                                Point {
                                    source,
                                    component: row.component,
                                    position: row.position,
                                    mapping,
                                }
                            })
                            .collect(),
                    };
                    self.roots.push(stored);
                    self.captured.push(root.clone());
                    self.roots.len() - 1
                });
                SlotRef {
                    latest,
                    attempt: value.attempt,
                }
            };
            let slots = Slots {
                food: slot(&owner.food),
                magic: slot(&owner.magic),
                mapping: slot(&owner.mapping),
            };
            directory.push(Binding { actor, slots });
        }
        self.owners
            .iter()
            .position(|d| d == &directory)
            .unwrap_or_else(|| {
                self.owners.push(directory);
                self.owners.len() - 1
            })
    }
    pub(crate) fn owner_ids(&self, index: usize) -> Option<impl Iterator<Item = ActorId> + '_> {
        self.owners.get(index).map(|d| d.iter().map(|b| b.actor))
    }
    pub(crate) fn spatial_indices(&self) -> impl ExactSizeIterator<Item = usize> + '_ {
        self.roots.iter().map(|r| r.spatial)
    }
    pub(crate) fn world_indices(&self) -> impl Iterator<Item = usize> + '_ {
        self.roots.iter().map(|r| r.world)
    }
    pub(crate) fn references_valid(
        &self,
        boundaries: impl IntoIterator<Item = (usize, u64)>,
    ) -> bool {
        if self.structure().is_err() {
            return false;
        }
        let (mut owners, mut roots) = (BTreeSet::new(), BTreeSet::new());
        for (index, tick) in boundaries {
            let Some(directory) = self.owners.get(index) else {
                return false;
            };
            owners.insert(index);
            for b in directory {
                for slot in [b.slots.food, b.slots.magic, b.slots.mapping] {
                    if slot.attempt.is_some_and(|a| a.attempted_at > tick) {
                        return false;
                    }
                    if let Some(root) = slot.latest {
                        roots.insert(root);
                    }
                }
            }
        }
        owners.len() == self.owners.len() && roots.len() == self.roots.len()
    }
    pub(crate) fn restore(
        &self,
        index: usize,
        tick: u64,
        mut actor_exists: impl FnMut(ActorId) -> bool,
        cache: &mut RestoreCache,
        validated_worlds: &mut BTreeSet<usize>,
        mut authority: impl FnMut(usize, usize) -> Result<AcquisitionAuthority, Error>,
    ) -> Result<Shared<BTreeMap<ActorId, Owner>>, Error> {
        // Complete-pool structure/reachability was bound once by RestoreContext
        // or LocalHistory, not repeated at every retained snapshot.
        let directory = self.owners.get(index).ok_or(Error::Reference)?;
        let mut owners = BTreeMap::new();
        for binding in directory {
            if !actor_exists(binding.actor) {
                return Err(Error::Reference);
            }
            let mut slot = |stored: SlotRef| -> Result<Slot, Error> {
                let latest = if let Some(index) = stored.latest {
                    if let Some(existing) = cache.roots.get(&index) {
                        return Ok(Slot {
                            latest: Some(existing.clone()),
                            attempt: stored.attempt,
                        });
                    }
                    {
                        let root = self.roots.get(index).ok_or(Error::Reference)?;
                        if let Some(previous) = cache.mapping_proof(self, root) {
                            let old = &self.roots[previous];
                            // Even a memo hit checks this root's complete canonical
                            // public chart and private occurrence proof, not its ID.
                            if root.components != old.components || root.points != old.points {
                                return Err(Error::Projection);
                            }
                            let mut value =
                                (**cache.roots.get(&previous).ok_or(Error::Reference)?).clone();
                            value.acquired_tick = root.acquired_tick;
                            cache.roots.insert(index, Shared::new(value));
                            return Ok(Slot {
                                latest: Some(cache.roots[&index].clone()),
                                attempt: stored.attempt,
                            });
                        }
                        let proof = authority(root.world, root.spatial)?;
                        if !validated_worlds.contains(&root.world) {
                            if !proof.world.historical_checkpoint_valid() {
                                return Err(Error::Source);
                            }
                            validated_worlds.insert(root.world);
                        }
                        let mut components = Vec::new();
                        components
                            .try_reserve_exact(root.components.len())
                            .map_err(|_| Error::Capacity)?;
                        components.extend(root.components.iter().map(|a| chart::Component {
                            alignment: match a {
                                Alignment::Aligned => scope_projection::Alignment::Aligned,
                                Alignment::Inconsistent => {
                                    scope_projection::Alignment::Inconsistent
                                }
                            },
                        }));
                        let mut rows = Vec::new();
                        rows.try_reserve_exact(root.points.len())
                            .map_err(|_| Error::Capacity)?;
                        for p in &root.points {
                            let mark = match root.kind {
                                Kind::Food => chart::Mark::Food,
                                Kind::Magic => chart::Mark::Magic,
                                Kind::Mapping => {
                                    p.mapping.as_ref().ok_or(Error::Structure)?.decode()?.1
                                }
                            };
                            rows.push(chart::Row {
                                component: p.component,
                                position: p.position,
                                mark,
                            });
                        }
                        let candidate = chart::Candidate::from_complete(
                            root.kind.chart_kind(),
                            components,
                            rows,
                            archive_budget().chart,
                        )
                        .map_err(|_| Error::Structure)?;
                        let chart = candidate.shared_chart().cloned().ok_or(Error::Capacity)?;
                        let value = if root.kind == Kind::Mapping {
                            let mut points = Vec::new();
                            points
                                .try_reserve_exact(root.points.len())
                                .map_err(|_| Error::Capacity)?;
                            for p in &root.points {
                                points.push(crate::mapping_chart::SourceRow {
                                    location: p.source,
                                    mark: p.mapping.as_ref().ok_or(Error::Structure)?.decode()?.0,
                                });
                            }
                            #[cfg(test)]
                            {
                                cache.mapping_replays += 1;
                            }
                            HistoricalChart::mapping_after_world_validation(
                                root.acquired_tick,
                                root.scope.clone(),
                                Shared::new(proof),
                                chart,
                                points,
                                archive_budget(),
                            )?
                        } else {
                            let mut points = Vec::new();
                            points
                                .try_reserve_exact(root.points.len())
                                .map_err(|_| Error::Capacity)?;
                            points.extend(root.points.iter().map(|p| p.source));
                            HistoricalChart::checked_after_world_validation(
                                root.kind,
                                root.acquired_tick,
                                root.scope.clone(),
                                Shared::new(proof),
                                chart,
                                points,
                                archive_budget(),
                            )?
                        };
                        cache.roots.insert(index, Shared::new(value));
                        if root.kind == Kind::Mapping {
                            cache.remember_mapping(self, index);
                        }
                    }
                    Some(cache.roots[&index].clone())
                } else {
                    None
                };
                Ok(Slot {
                    latest,
                    attempt: stored.attempt,
                })
            };
            owners.insert(
                binding.actor,
                Owner {
                    food: slot(binding.slots.food)?,
                    magic: slot(binding.slots.magic)?,
                    mapping: slot(binding.slots.mapping)?,
                },
            );
        }
        if !owners_valid(&owners, tick, actor_exists) {
            return Err(Error::Reference);
        }
        Ok(Shared::new(owners))
    }
}
#[derive(Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LocalHistory {
    #[serde(with = "tor_world::checkpoint_worlds")]
    worlds: Vec<World>,
    spatial: crate::spatial_scope::retention::PoolState,
    history: Pool,
    owners: usize,
}
impl LocalHistory {
    pub(crate) fn capture(owners: &BTreeMap<ActorId, Owner>) -> Self {
        let (mut worlds, mut spatial, mut history) = (
            Vec::new(),
            crate::spatial_scope::retention::PoolState::default(),
            Pool::default(),
        );
        let owners = history.capture(owners, &mut worlds, &mut spatial);
        Self {
            worlds,
            spatial,
            history,
            owners,
        }
    }
    pub(crate) fn restore(self) -> Result<Shared<BTreeMap<ActorId, Owner>>, Error> {
        self.restore_with_cache(&mut RestoreCache::default())
    }
    // Private invocation keeps one cache namespace per local blob. Tests may
    // choose the optional memo work allowance while exercising this exact path.
    fn restore_with_cache(
        &self,
        cache: &mut RestoreCache,
    ) -> Result<Shared<BTreeMap<ActorId, Owner>>, Error> {
        // Actual global boundary time is checked again by RegionRecord attach;
        // active actor stamps are not global time and cannot substitute here.
        if !self.history.references_valid([(self.owners, u64::MAX)]) {
            return Err(Error::Reference);
        }
        let world_refs: BTreeSet<_> = self.history.world_indices().collect();
        if world_refs.len() != self.worlds.len()
            || world_refs.iter().any(|i| *i >= self.worlds.len())
        {
            return Err(Error::Reference);
        }
        let references: Vec<_> = self.history.spatial_indices().collect();
        let spatial = self
            .spatial
            .bind(references.into_iter())
            .map_err(|_| Error::Reference)?;
        let mut worlds = BTreeMap::new();
        let mut registries: BTreeMap<usize, Shared<crate::spatial_scope::registry::Registry>> =
            BTreeMap::new();
        self.history.restore(
            self.owners,
            u64::MAX,
            |_| true,
            cache,
            &mut BTreeSet::new(),
            |w, s| {
                let world = worlds
                    .entry(w)
                    .or_insert_with(|| Shared::new(self.worlds[w].clone()))
                    .clone();
                let registry = if let Some(r) = registries.get(&s) {
                    r.clone()
                } else {
                    let r = spatial.registry(s).ok_or(Error::Reference)?;
                    registries.insert(s, r.clone());
                    r
                };
                Ok(AcquisitionAuthority { world, registry })
            },
        )
    }
}

impl Serialize for Pool {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.structure()
            .map_err(|_| serde::ser::Error::custom("invalid history pool"))?;
        #[derive(Serialize)]
        struct Fields<'a> {
            roots: &'a [Root],
            owners: &'a [Vec<Binding>],
        }
        Fields {
            roots: &self.roots,
            owners: &self.owners,
        }
        .serialize(s)
    }
}

pub(crate) mod local_owners {
    use super::*;
    pub fn serialize<S: Serializer>(
        owners: &Shared<BTreeMap<ActorId, Owner>>,
        s: S,
    ) -> Result<S::Ok, S::Error> {
        LocalHistory::capture(owners).serialize(s)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(
        d: D,
    ) -> Result<Shared<BTreeMap<ActorId, Owner>>, D::Error> {
        LocalHistory::deserialize(d)?
            .restore()
            .map_err(|_| D::Error::custom("invalid detached history"))
    }
}

impl<'de> Deserialize<'de> for Point {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            source: Location,
            component: usize,
            #[serde(deserialize_with = "crate::bounded::required_option")]
            position: Option<[i64; 3]>,
            #[serde(deserialize_with = "crate::bounded::required_option")]
            mapping: Option<MappingProof>,
        }
        let f: Fields = crate::bounded::named_map(d)?;
        Ok(Self {
            source: f.source,
            component: f.component,
            position: f.position,
            mapping: f.mapping,
        })
    }
}
impl<'de> Deserialize<'de> for SlotRef {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            #[serde(deserialize_with = "crate::bounded::required_option")]
            latest: Option<usize>,
            #[serde(deserialize_with = "crate::bounded::required_option")]
            attempt: Option<Attempt>,
        }
        let f: Fields = crate::bounded::named_map(d)?;
        Ok(Self {
            latest: f.latest,
            attempt: f.attempt,
        })
    }
}
impl<'de> Deserialize<'de> for Slots {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            food: SlotRef,
            magic: SlotRef,
            mapping: SlotRef,
        }
        let f: Fields = crate::bounded::named_map(d)?;
        Ok(Self {
            food: f.food,
            magic: f.magic,
            mapping: f.mapping,
        })
    }
}
impl<'de> Deserialize<'de> for Binding {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            actor: ActorId,
            slots: Slots,
        }
        let f: Fields = crate::bounded::named_map(d)?;
        Ok(Self {
            actor: f.actor,
            slots: f.slots,
        })
    }
}

impl<'de> Deserialize<'de> for LocalHistory {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            #[serde(with = "tor_world::checkpoint_worlds")]
            worlds: Vec<World>,
            spatial: crate::spatial_scope::retention::PoolState,
            history: Pool,
            owners: usize,
        }
        let f: Fields = crate::bounded::named_map(d)?;
        Ok(Self {
            worlds: f.worlds,
            spatial: f.spatial,
            history: f.history,
            owners: f.owners,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum MappingTag {
    WallFace,
    Passage,
    Doorway,
    Stairs,
    SecretDoor,
    TrapPresence,
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
struct MappingProof {
    tag: MappingTag,
    room: Option<u32>,
    toward: Option<tor_world::Direction>,
    normal: Option<[i8; 3]>,
}
impl MappingProof {
    fn capture(source: crate::mapping_chart::SourceMark, display: &chart::Mark) -> Self {
        use crate::mapping_chart::SourceMark as S;
        let (tag, room, toward) = match source {
            S::WallFace { room, toward } => (MappingTag::WallFace, Some(room.get()), Some(toward)),
            S::Passage => (MappingTag::Passage, None, None),
            S::Doorway => (MappingTag::Doorway, None, None),
            S::Stairs => (MappingTag::Stairs, None, None),
            S::SecretDoor => (MappingTag::SecretDoor, None, None),
            S::TrapPresence => (MappingTag::TrapPresence, None, None),
        };
        let normal = match display {
            chart::Mark::Mapping(chart::MappingMark::WallFace { normal }) => *normal,
            _ => None,
        };
        Self {
            tag,
            room,
            toward,
            normal,
        }
    }
    fn decode(&self) -> Result<(crate::mapping_chart::SourceMark, chart::Mark), Error> {
        use crate::mapping_chart::SourceMark as S;
        use chart::MappingMark as M;
        let pair = match self.tag {
            MappingTag::WallFace => {
                let room = crate::spatial_scope::RoomId::new(self.room.ok_or(Error::Structure)?)
                    .ok_or(Error::Structure)?;
                let toward = self.toward.ok_or(Error::Structure)?;
                (
                    S::WallFace { room, toward },
                    M::WallFace {
                        normal: self.normal,
                    },
                )
            }
            tag => {
                if self.room.is_some() || self.toward.is_some() || self.normal.is_some() {
                    return Err(Error::Structure);
                }
                match tag {
                    MappingTag::Passage => (S::Passage, M::Passage),
                    MappingTag::Doorway => (S::Doorway, M::Doorway),
                    MappingTag::Stairs => (S::Stairs, M::Stairs),
                    MappingTag::SecretDoor => (S::SecretDoor, M::SecretDoor),
                    MappingTag::TrapPresence => (S::TrapPresence, M::TrapPresence),
                    MappingTag::WallFace => unreachable!(),
                }
            }
        };
        Ok((pair.0, chart::Mark::Mapping(pair.1)))
    }
}
impl<'de> Deserialize<'de> for MappingProof {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            tag: MappingTag,
            #[serde(deserialize_with = "crate::bounded::required_option")]
            room: Option<u32>,
            #[serde(deserialize_with = "crate::bounded::required_option")]
            toward: Option<tor_world::Direction>,
            #[serde(deserialize_with = "crate::bounded::required_option")]
            normal: Option<[i8; 3]>,
        }
        let f: Fields = crate::bounded::named_map(d)?;
        let value = Self {
            tag: f.tag,
            room: f.room,
            toward: f.toward,
            normal: f.normal,
        };
        value
            .decode()
            .map_err(|_| D::Error::custom("invalid mapping proof"))?;
        Ok(value)
    }
}

#[cfg(test)]
#[path = "codec_memo_tests.rs"]
mod memo_tests;
