//! Authoritative items and derived region-local location/owner indexes.
//! No mutable map reference escapes this store. Indexes are rebuilt from the
//! authoritative entries on restore, and never form part of a save schema.
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

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ItemStore {
    entries: Shared<BTreeMap<ItemId, Item>>,
    locations: Locations,
}

impl Deref for ItemStore {
    type Target = BTreeMap<ItemId, Item>;
    fn deref(&self) -> &Self::Target {
        &self.entries
    }
}

impl ItemStore {
    pub fn from_entries(entries: BTreeMap<ItemId, Item>) -> Self {
        let mut locations = Locations::default();
        for (&id, item) in &entries {
            locations.insert(item.location, id);
        }
        Self {
            entries: Shared::new(entries),
            locations,
        }
    }

    pub fn at(&self, at: ItemLocation) -> impl Iterator<Item = ItemId> + '_ {
        self.locations
            .ids(at)
            .into_iter()
            .flat_map(|ids| ids.iter().copied())
    }

    pub fn perceived<'a>(
        &'a self,
        actor: ActorId,
        visible: &BTreeSet<Location>,
    ) -> impl Iterator<Item = (ItemId, &'a Item)> + 'a {
        let mut buckets: Vec<_> = visible
            .iter()
            .filter_map(|&at| self.locations.ids(ItemLocation::Ground(at)))
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
        let previous = self.entries.insert(id, item);
        if let Some(previous) = &previous {
            self.locations.remove(previous.location, id);
        }
        self.locations.insert(at, id);
        previous
    }

    pub fn remove(&mut self, id: &ItemId) -> Option<Item> {
        let item = self.entries.remove(id)?;
        self.locations.remove(item.location, *id);
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
            id,
            before: item.location,
            item,
            locations: &mut self.locations,
        };
        Some(edit(guard.item))
    }

    #[cfg(test)]
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
    id: ItemId,
    before: ItemLocation,
    item: &'a mut Item,
    locations: &'a mut Locations,
}
impl Drop for ItemEdit<'_> {
    fn drop(&mut self) {
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
            location,
            quantity: 1,
            spec: tor_world::Shared::new(ItemSpec::ordinary("weight".into())),
            motion: MotionState::default(),
            orientation: 0,
        }
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
