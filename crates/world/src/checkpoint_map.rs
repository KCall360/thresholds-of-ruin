//! Backend checkpoint encoding for ordered maps with compound keys.
//! No I/O or protocol types; duplicate and out-of-order keys fail closed.
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::BTreeMap;

pub fn serialize<K: Serialize, V: Serialize, S: Serializer>(
    map: &BTreeMap<K, V>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    serializer.collect_seq(map.iter())
}

pub fn deserialize<'de, K: Deserialize<'de> + Ord, V: Deserialize<'de>, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<BTreeMap<K, V>, D::Error> {
    let entries = Vec::<(K, V)>::deserialize(deserializer)?;
    if entries.windows(2).any(|pair| pair[0].0 >= pair[1].0) {
        return Err(serde::de::Error::custom(
            "checkpoint map keys must be unique and ordered",
        ));
    }
    Ok(entries.into_iter().collect())
}

/// Preserve strict ordered-map decoding for copy-on-write backend maps.
pub mod shared {
    use super::*;
    use crate::Shared;
    pub fn serialize<K: Serialize, V: Serialize, S: Serializer>(
        map: &Shared<BTreeMap<K, V>>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        super::serialize(map, serializer)
    }
    pub fn deserialize<
        'de,
        K: Deserialize<'de> + Ord,
        V: Deserialize<'de>,
        D: Deserializer<'de>,
    >(
        deserializer: D,
    ) -> Result<Shared<BTreeMap<K, V>>, D::Error> {
        super::deserialize(deserializer).map(Shared::new)
    }
}
