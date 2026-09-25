//! Ordered navigation knowledge shared by source region. Mutating a visible
//! region must not copy all previously discovered regions into a rewind snapshot.
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::BTreeMap;
use tor_world::{Direction, Location, RegionId, Shared};

/// Implementations must sort first by region; snapshot decoding checks global order.
pub(crate) trait RegionKey: Ord + Clone {
    fn region(&self) -> RegionId;
}
impl RegionKey for Location {
    fn region(&self) -> RegionId {
        self.region
    }
}
impl RegionKey for (Location, Direction) {
    fn region(&self) -> RegionId {
        self.0.region
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RegionMap<K, V> {
    regions: BTreeMap<RegionId, Shared<BTreeMap<K, V>>>,
}
impl<K, V> Default for RegionMap<K, V> {
    fn default() -> Self {
        Self {
            regions: BTreeMap::new(),
        }
    }
}
impl<K: RegionKey, V> RegionMap<K, V> {
    pub fn get(&self, key: &K) -> Option<&V> {
        self.regions.get(&key.region())?.get(key)
    }
    pub fn contains_key(&self, key: &K) -> bool {
        self.get(key).is_some()
    }
    pub fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
        self.regions.values().flat_map(|map| map.iter())
    }
    pub fn keys(&self) -> impl Iterator<Item = &K> {
        self.iter().map(|(key, _)| key)
    }
}
impl<K: RegionKey, V: Clone> RegionMap<K, V> {
    pub fn insert(&mut self, key: K, value: V) {
        self.regions
            .entry(key.region())
            .or_default()
            .insert(key, value);
    }
    pub fn remove(&mut self, key: &K) {
        if let Some(map) = self.regions.get_mut(&key.region()) {
            map.remove(key);
            if map.is_empty() {
                self.regions.remove(&key.region());
            }
        }
    }
    pub fn extend(&mut self, values: impl IntoIterator<Item = (K, V)>) {
        for (key, value) in values {
            self.insert(key, value);
        }
    }
}
// The wire snapshot remains the same strictly ordered flat key/value sequence.
impl<K: RegionKey + Serialize, V: Serialize> Serialize for RegionMap<K, V> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_seq(self.iter())
    }
}
impl<'de, K: RegionKey + Deserialize<'de>, V: Clone + Deserialize<'de>> Deserialize<'de>
    for RegionMap<K, V>
{
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let entries: BTreeMap<K, V> = tor_world::checkpoint_map::deserialize(deserializer)?;
        let mut result = Self::default();
        result.extend(entries);
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tor_world::Position;
    #[test]
    fn editing_one_region_preserves_all_other_snapshot_storage_and_order() {
        let key = |region| Location {
            region: RegionId(region),
            position: Position { x: 1, y: 1, z: 0 },
        };
        let mut map = RegionMap::default();
        for region in 1..=256 {
            map.insert(key(region), false);
        }
        let original = map.clone();
        map.insert(key(128), true);
        assert_eq!(original.get(&key(128)), Some(&false));
        assert_eq!(map.get(&key(128)), Some(&true));
        assert_eq!(
            map.keys().map(|k| k.region.0).collect::<Vec<_>>(),
            (1..=256).collect::<Vec<_>>()
        );
        for region in 1..=256 {
            assert_eq!(
                map.regions[&RegionId(region)].shares_storage(&original.regions[&RegionId(region)]),
                region != 128
            );
        }
        map.remove(&key(128));
        assert!(!map.contains_key(&key(128)));
        assert!(original.contains_key(&key(128)));
    }
}
