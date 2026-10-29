//! Authoritative items and derived region-local location/owner indexes.
//! No mutable map reference escapes this store. Indexes are rebuilt from the
//! authoritative entries on restore, and never form part of a save schema.
use crate::mutation::{MutationChange, MutationCursor, MutationLedger};
use crate::navigation_map::RegionMap;
use crate::{ActorId, Item, ItemId, ItemLocation};
use std::cmp::Reverse;
use std::collections::{btree_map, btree_set, BTreeMap, BTreeSet, BinaryHeap};
use std::ops::Deref;
use tor_world::{Location, Shared};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Locations {
    ground: Shared<RegionMap<Location, Shared<BTreeSet<ItemId>>>>,
    carried: Shared<BTreeMap<ActorId, Shared<BTreeSet<ItemId>>>>,
}

impl Locations {
    fn ids(&self, at: ItemLocation) -> Option<&BTreeSet<ItemId>> {
        match at {
            ItemLocation::Ground(at) => self.ground.get(&at).map(|ids| &**ids),
            ItemLocation::Carried(owner) => self.carried.get(&owner).map(|ids| &**ids),
        }
    }

    fn insert(&mut self, at: ItemLocation, id: ItemId) {
        match at {
            ItemLocation::Ground(at) => {
                if !self.ground.contains_key(&at) {
                    self.ground.insert(at, Shared::default());
                }
                self.ground
                    .get_mut(&at)
                    .expect("location inserted")
                    .insert(id);
            }
            ItemLocation::Carried(owner) => {
                self.carried.entry(owner).or_default().insert(id);
            }
        }
    }

    fn remove(&mut self, at: ItemLocation, id: ItemId) {
        match at {
            ItemLocation::Ground(at) => {
                let ids = self.ground.get_mut(&at).expect("indexed ground item");
                assert!(ids.remove(&id), "indexed item identity");
                if ids.is_empty() {
                    self.ground.remove(&at);
                }
            }
            ItemLocation::Carried(owner) => {
                let ids = self.carried.get_mut(&owner).expect("indexed carried item");
                assert!(ids.remove(&id), "indexed item identity");
                if ids.is_empty() {
                    self.carried.remove(&owner);
                }
            }
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ItemMutation {
    pub(crate) before: Option<ItemLocation>,
    pub(crate) after: Option<ItemLocation>,
}
impl MutationChange for ItemMutation {
    fn append(&mut self, later: &Self) {
        self.after = later.after;
    }
}

#[derive(Clone, Default)]
pub(crate) struct ItemStore {
    entries: Shared<BTreeMap<ItemId, Item>>,
    locations: Locations,
    claimed: Shared<BTreeSet<ItemId>>,
    mutations: MutationLedger<ItemId, ItemMutation>,
}

// Mutation history is derived candidate-local evidence, not authoritative state.
impl std::fmt::Debug for ItemStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ItemStore")
            .field("entries", &self.entries)
            .field("locations", &self.locations)
            .field("claimed", &self.claimed)
            .finish()
    }
}
impl PartialEq for ItemStore {
    fn eq(&self, other: &Self) -> bool {
        self.entries == other.entries
            && self.locations == other.locations
            && self.claimed == other.claimed
    }
}
impl Eq for ItemStore {}

impl Deref for ItemStore {
    type Target = BTreeMap<ItemId, Item>;
    fn deref(&self) -> &Self::Target {
        &self.entries
    }
}

impl ItemStore {
    pub(crate) fn mutation_cursor(&self) -> MutationCursor<ItemId, ItemMutation> {
        self.mutations.cursor()
    }
    #[cfg(test)]
    pub(crate) fn mutation_stamp(&self, id: &ItemId) -> Option<crate::mutation::MutationStamp> {
        self.mutations.stamp(id)
    }
    pub(crate) fn changes_since(
        &self,
        cursor: &MutationCursor<ItemId, ItemMutation>,
    ) -> Option<BTreeMap<ItemId, ItemMutation>> {
        self.mutations.changes_since(cursor)
    }
    pub(crate) fn certifies_since(&self, cursor: &MutationCursor<ItemId, ItemMutation>) -> bool {
        self.mutations.certifies_since(cursor)
    }

    pub fn from_entries(entries: BTreeMap<ItemId, Item>) -> Self {
        let mut mutations = MutationLedger::default();
        let mut locations = Locations::default();
        let mut claimed = BTreeSet::new();
        for (&id, item) in &entries {
            mutations.publish(
                id,
                ItemMutation {
                    before: None,
                    after: Some(item.location),
                },
                true,
            );
            locations.insert(item.location, id);
            if !item.holds.is_empty() {
                claimed.insert(id);
            }
        }
        Self {
            entries: Shared::new(entries),
            locations,
            claimed: Shared::new(claimed),
            mutations,
        }
    }

    pub(crate) fn claimed(&self) -> impl Iterator<Item = (ItemId, &Item)> {
        self.claimed.iter().map(|id| (*id, &self.entries[id]))
    }

    pub fn at(&self, at: ItemLocation) -> impl Iterator<Item = ItemId> + '_ {
        self.locations
            .ids(at)
            .into_iter()
            .flat_map(|ids| ids.iter().copied())
    }

    /// Cursor for candidate-local ownership changes. It does not clone an
    /// inventory, and removing the previous entry cannot invalidate progress.
    pub(crate) fn next_at_after(
        &self,
        at: ItemLocation,
        previous: Option<ItemId>,
    ) -> Option<ItemId> {
        let ids = self.locations.ids(at)?;
        match previous {
            Some(id) => ids
                .range((std::ops::Bound::Excluded(id), std::ops::Bound::Unbounded))
                .next()
                .copied(),
            None => ids.first().copied(),
        }
    }

    pub fn perceived<'a>(
        &'a self,
        actor: ActorId,
        visible: &BTreeSet<Location>,
    ) -> impl Iterator<Item = (ItemId, &'a Item)> + 'a {
        let mut buckets: Vec<_> = visible
            .iter()
            .take(if self.locations.ground.iter().next().is_none() {
                0
            } else {
                visible.len()
            })
            .filter_map(|&at| {
                crate::diagnostics::item_ground_probe();
                self.locations.ids(ItemLocation::Ground(at))
            })
            .chain(self.locations.ids(ItemLocation::Carried(actor)))
            .map(BTreeSet::iter)
            .collect();
        // If the buckets cover the complete store, every item is disclosed.
        // Avoid one tree lookup per item in dense visible piles. Partial views
        // still visit only the selected identities, regardless of world size.
        if buckets.iter().map(ExactSizeIterator::len).sum::<usize>() == self.entries.len() {
            return PerceivedItems::All(self.entries.iter());
        }
        let heads = buckets
            .iter_mut()
            .enumerate()
            .filter_map(|(bucket, ids)| ids.next().map(|&id| Reverse((id, bucket))))
            .collect();
        PerceivedItems::Indexed {
            entries: &self.entries,
            buckets,
            heads,
        }
    }

    pub fn insert(&mut self, id: ItemId, item: Item) -> Option<Item> {
        let at = item.location;
        if item.holds.is_empty() {
            self.claimed.remove(&id);
        } else {
            self.claimed.insert(id);
        }
        let previous = self.entries.insert(id, item);
        if let Some(previous) = &previous {
            self.locations.remove(previous.location, id);
        }
        self.locations.insert(at, id);
        self.mutations.publish(
            id,
            ItemMutation {
                before: previous.as_ref().map(|item| item.location),
                after: Some(at),
            },
            true,
        );
        previous
    }

    pub fn remove(&mut self, id: &ItemId) -> Option<Item> {
        let item = self.entries.remove(id)?;
        self.claimed.remove(id);
        self.locations.remove(item.location, *id);
        self.mutations.publish(
            *id,
            ItemMutation {
                before: Some(item.location),
                after: None,
            },
            false,
        );
        Some(item)
    }

    pub fn extend(&mut self, entries: impl IntoIterator<Item = (ItemId, Item)>) {
        for (id, item) in entries {
            self.insert(id, item);
        }
    }

    pub fn edit<R>(&mut self, id: ItemId, edit: impl FnOnce(&mut Item) -> R) -> Option<R> {
        let item = self.entries.get_mut(&id)?;
        let guard = ItemEdit {
            mutations: &mut self.mutations,
            id,
            before: item.location,
            item,
            locations: &mut self.locations,
            claimed: &mut self.claimed,
        };
        Some(edit(guard.item))
    }

    pub fn shares_storage(&self, other: &Self) -> bool {
        self.entries.shares_storage(&other.entries)
    }
}

// Buckets have disjoint identities. Merge their ordered iterators without an
// allocation proportional to the number of disclosed items.
enum PerceivedItems<'a> {
    All(btree_map::Iter<'a, ItemId, Item>),
    Indexed {
        entries: &'a BTreeMap<ItemId, Item>,
        buckets: Vec<btree_set::Iter<'a, ItemId>>,
        heads: BinaryHeap<Reverse<(ItemId, usize)>>,
    },
}

impl<'a> Iterator for PerceivedItems<'a> {
    type Item = (ItemId, &'a Item);
    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::All(items) => items.next().map(|(&id, item)| (id, item)),
            Self::Indexed {
                entries,
                buckets,
                heads,
            } => {
                let Reverse((id, bucket)) = heads.pop()?;
                if let Some(&next) = buckets[bucket].next() {
                    heads.push(Reverse((next, bucket)));
                }
                Some((id, &entries[&id]))
            }
        }
    }
}

// Maintain indexes on early return and unwinding as well as ordinary edits.
struct ItemEdit<'a> {
    mutations: &'a mut MutationLedger<ItemId, ItemMutation>,
    id: ItemId,
    before: ItemLocation,
    item: &'a mut Item,
    locations: &'a mut Locations,
    claimed: &'a mut BTreeSet<ItemId>,
}
impl Drop for ItemEdit<'_> {
    fn drop(&mut self) {
        self.mutations.publish(
            self.id,
            ItemMutation {
                before: Some(self.before),
                after: Some(self.item.location),
            },
            true,
        );
        if self.item.holds.is_empty() {
            self.claimed.remove(&self.id);
        } else {
            self.claimed.insert(self.id);
        }
        if self.item.location != self.before {
            self.locations.remove(self.before, self.id);
            self.locations.insert(self.item.location, self.id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ItemSpec, MotionState};
    use tor_world::{Position, RegionId};

    fn ground(region: u64, x: i32) -> ItemLocation {
        ItemLocation::Ground(Location {
            region: RegionId(region),
            position: Position { x, y: 0, z: 0 },
        })
    }

    fn item(location: ItemLocation) -> Item {
        Item {
            condition: crate::armor::ItemCondition::default(),
            charges: None,
            relocation_elapsed: None,
            fuel: None,
            corpse: None,
            holds: vec![],
            location,
            quantity: 1,
            spec: tor_world::Shared::new(ItemSpec::ordinary("weight".into())),
            motion: MotionState::default(),
            orientation: 0,
        }
    }

    #[test]
    fn empty_ground_index_skips_scene_probes_without_losing_carried_items() {
        let owner = ActorId(1);
        let mut store = ItemStore::default();
        store.insert(ItemId(10), item(ItemLocation::Carried(owner)));
        store.insert(ItemId(20), item(ItemLocation::Carried(ActorId(2))));
        let visible: BTreeSet<_> = (0..8)
            .map(|x| match ground(1, x) {
                ItemLocation::Ground(at) => at,
                _ => unreachable!(),
            })
            .collect();
        let before = crate::diagnostics::work_counts().item_ground_probes;
        let observed: Vec<_> = store.perceived(owner, &visible).map(|(id, _)| id).collect();
        assert_eq!(observed, vec![ItemId(10)]);
        assert_eq!(
            crate::diagnostics::work_counts().item_ground_probes - before,
            0,
            "an empty ground index needs no visible-location lookups"
        );
        let retained = store.clone();
        store.insert(ItemId(5), item(ground(1, 2)));
        assert_eq!(
            store
                .perceived(owner, &visible)
                .map(|(id, _)| id)
                .collect::<Vec<_>>(),
            vec![ItemId(5), ItemId(10)]
        );
        assert_eq!(
            retained
                .perceived(owner, &visible)
                .map(|(id, _)| id)
                .collect::<Vec<_>>(),
            vec![ItemId(10)]
        );
        store.edit(ItemId(5), |item| item.location = ground(2, 2));
        assert_eq!(
            store
                .perceived(owner, &visible)
                .map(|(id, _)| id)
                .collect::<Vec<_>>(),
            vec![ItemId(10)]
        );
        store.remove(&ItemId(5));
        let before = crate::diagnostics::work_counts().item_ground_probes;
        assert_eq!(
            store
                .perceived(owner, &visible)
                .map(|(id, _)| id)
                .collect::<Vec<_>>(),
            vec![ItemId(10)]
        );
        assert_eq!(crate::diagnostics::work_counts().item_ground_probes, before);
    }

    #[test]
    fn mutation_publication_tracks_payload_touches_aba_and_candidate_rollback() {
        let id = ItemId(10);
        let mut store = ItemStore::from_entries(BTreeMap::from([(id, item(ground(1, 0)))]));
        let original = store.clone();
        let cursor = store.mutation_cursor();
        let stamp = store.mutation_stamp(&id).expect("initial publication");
        store.edit(id, |item| item.quantity = 2);
        assert_ne!(store.mutation_stamp(&id), Some(stamp));
        assert_eq!(original.mutation_stamp(&id), Some(stamp));
        let changes = store.changes_since(&cursor).unwrap();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[&id].before, Some(ground(1, 0)));
        assert_eq!(changes[&id].after, Some(ground(1, 0)));
        let middle = store.mutation_cursor();
        let middle_stamp = store.mutation_stamp(&id);
        store.edit(id, |item| item.location = ground(2, 0));
        assert_ne!(store.mutation_stamp(&id), middle_stamp);
        assert_eq!(
            store.changes_since(&middle).unwrap()[&id].after,
            Some(ground(2, 0))
        );
        store.edit(id, |item| {
            item.location = ground(1, 0);
            item.quantity = 1;
        });
        assert_eq!(store, original, "mutation identities are not authoritative");
        assert_eq!(
            store.changes_since(&cursor).unwrap()[&id].after,
            Some(ground(1, 0))
        );
        assert!(store.certifies_since(&cursor));
        assert!(!original.certifies_since(&middle));
        let removed = store.remove(&id).unwrap();
        assert_eq!(store.mutation_stamp(&id), None);
        assert!(store.changes_since(&cursor).unwrap()[&id].after.is_none());
        store.insert(id, removed);
        assert_ne!(store.mutation_stamp(&id), Some(stamp));
        assert_eq!(store, original);
        let restored = ItemStore::from_entries((*store).clone());
        assert_eq!(restored, store);
        assert!(restored.mutation_stamp(&id).is_some());
        assert!(restored.changes_since(&cursor).is_none());
    }

    #[test]
    fn interrupted_edits_publish_before_the_caller_recovers() {
        let id = ItemId(10);
        let mut store = ItemStore::default();
        store.insert(id, item(ground(1, 0)));
        let cursor = store.mutation_cursor();
        let stamp = store.mutation_stamp(&id);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            store.edit(id, |item| {
                item.quantity += 1;
                panic!("interrupted edit");
            });
        }));
        assert!(result.is_err());
        assert_ne!(store.mutation_stamp(&id), stamp);
        assert_eq!(store.changes_since(&cursor).unwrap().len(), 1);
    }

    fn check(store: &ItemStore) {
        let locations = [
            ground(1, 0),
            ground(1, 1),
            ground(2, 0),
            ItemLocation::Carried(ActorId(1)),
            ItemLocation::Carried(ActorId(2)),
        ];
        for at in locations {
            let expected: Vec<_> = store
                .iter()
                .filter_map(|(&id, item)| (item.location == at).then_some(id))
                .collect();
            assert_eq!(store.at(at).collect::<Vec<_>>(), expected);
        }
        let visible = BTreeSet::from_iter([ground(1, 0), ground(2, 0)].map(|at| {
            let ItemLocation::Ground(at) = at else {
                unreachable!()
            };
            at
        }));
        for actor in [ActorId(1), ActorId(2)] {
            let expected: Vec<_> = store
                .iter()
                .filter_map(|(&id, item)| {
                    let seen = match item.location {
                        ItemLocation::Ground(at) => visible.contains(&at),
                        ItemLocation::Carried(owner) => owner == actor,
                    };
                    seen.then_some(id)
                })
                .collect();
            assert_eq!(
                store
                    .perceived(actor, &visible)
                    .map(|(id, _)| id)
                    .collect::<Vec<_>>(),
                expected
            );
        }
        assert_eq!(store, &ItemStore::from_entries((**store).clone()));
    }

    #[test]
    fn indexed_queries_match_reference_scans_after_every_mutation() {
        let mut store = ItemStore::default();
        // Insert out of order; disclosure must still follow identity order.
        store.extend([
            (ItemId(30), item(ground(2, 0))),
            (ItemId(20), item(ground(1, 0))),
            (ItemId(10), item(ground(1, 0))),
        ]);
        check(&store);
        let snapshot = store.clone();
        assert!(snapshot.shares_storage(&store));
        assert!(snapshot
            .locations
            .ground
            .shares_storage(&store.locations.ground));
        store.edit(ItemId(10), |item| {
            item.location = ItemLocation::Carried(ActorId(1))
        });
        check(&store);
        check(&snapshot);
        assert_eq!(
            snapshot.at(ground(1, 0)).collect::<Vec<_>>(),
            vec![ItemId(10), ItemId(20)]
        );
        let untouched = ground(2, 0);
        let ItemLocation::Ground(at) = untouched else {
            unreachable!()
        };
        assert!(store
            .locations
            .ground
            .get(&at)
            .unwrap()
            .shares_storage(snapshot.locations.ground.get(&at).unwrap()));
        store.edit(ItemId(10), |item| {
            item.location = ItemLocation::Carried(ActorId(2))
        });
        check(&store);
        store.edit(ItemId(10), |item| {
            item.location = ground(1, 1);
            item.quantity = 8;
        });
        check(&store);
        store.insert(ItemId(20), item(ground(2, 0)));
        check(&store);
        for id in [10, 20, 30] {
            store.remove(&ItemId(id));
            check(&store);
        }
        assert_eq!(store.locations.ground.iter().count(), 0);
        assert!(store.locations.carried.is_empty());
    }

    #[test]
    fn interrupted_edit_keeps_the_location_index_consistent() {
        let mut store = ItemStore::default();
        store.insert(ItemId(10), item(ground(1, 0)));
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            store.edit(ItemId(10), |item| {
                item.location = ground(2, 0);
                panic!("interrupted operation");
            });
        }));
        assert!(result.is_err());
        check(&store);
    }
}
