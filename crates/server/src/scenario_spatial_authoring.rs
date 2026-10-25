//! Explicit bounded source geometry; no authored passage IDs or transform graph.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
// A source cannot contain more entries than bytes in its existing input envelope.
// This is an incremental allocation ceiling, not a new per-scope/member limit.
const SOURCE_ENTRIES: usize = super::MAX_BYTES as usize;
pub fn unique_map<'de, D, K, V>(deserializer: D) -> Result<BTreeMap<K, V>, D::Error>
where
    D: serde::Deserializer<'de>,
    K: Deserialize<'de> + Ord,
    V: Deserialize<'de>,
{
    struct Entries<K, V>(std::marker::PhantomData<(K, V)>);
    impl<'de, K: Deserialize<'de> + Ord, V: Deserialize<'de>> serde::de::Visitor<'de>
        for Entries<K, V>
    {
        type Value = BTreeMap<K, V>;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("bounded unique named geometry entries")
        }
        fn visit_map<A: serde::de::MapAccess<'de>>(
            self,
            mut map: A,
        ) -> Result<Self::Value, A::Error> {
            let mut result = BTreeMap::new();
            while let Some(key) = map.next_key()? {
                if result.len() >= SOURCE_ENTRIES || result.contains_key(&key) {
                    return Err(serde::de::Error::custom(
                        "duplicate or excessive spatial entries",
                    ));
                }
                result.insert(key, map.next_value()?);
            }
            Ok(result)
        }
    }
    deserializer.deserialize_map(Entries(std::marker::PhantomData))
}
fn bounded_vec<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct Values<T>(std::marker::PhantomData<T>);
    struct Reject;
    impl<'de> serde::de::DeserializeSeed<'de> for Reject {
        type Value = ();
        fn deserialize<D: serde::Deserializer<'de>>(self, _: D) -> Result<(), D::Error> {
            Err(serde::de::Error::custom("excessive spatial source entries"))
        }
    }
    impl<'de, T: Deserialize<'de>> serde::de::Visitor<'de> for Values<T> {
        type Value = Vec<T>;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("bounded geometry entries")
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut seq: A) -> Result<Vec<T>, A::Error> {
            let mut result = Vec::new();
            while result.len() < SOURCE_ENTRIES {
                match seq.next_element()? {
                    Some(v) => result.push(v),
                    None => return Ok(result),
                }
            }
            seq.next_element_seed(Reject)?;
            Ok(result)
        }
    }
    deserializer.deserialize_seq(Values(std::marker::PhantomData))
}
fn room_map<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<BTreeMap<u32, Vec<RoomVolume>>, D::Error> {
    struct Volumes(Vec<RoomVolume>);
    impl<'de> Deserialize<'de> for Volumes {
        fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
            bounded_vec(d).map(Self)
        }
    }
    let entries: BTreeMap<u32, Volumes> = unique_map(d)?;
    Ok(entries.into_iter().map(|(id, v)| (id, v.0)).collect())
}
#[derive(Clone, Debug, Serialize)]
pub struct AuthoredSpatialScope {
    pub members: Vec<u64>,
    pub rooms: BTreeMap<u32, Vec<RoomVolume>>,
    pub passages: Vec<SourceRun>,
    pub doorways: Vec<OwnedRun>,
}
#[derive(Clone, Debug, Serialize)]
pub struct RoomVolume {
    pub region: u64,
    pub min: [i32; 3],
    pub max: [i32; 3],
}
#[derive(Clone, Debug, Serialize)]
pub struct SourceRun {
    pub region: u64,
    pub z: i32,
    pub y: i32,
    pub first_x: i32,
    pub last_x: i32,
}
#[derive(Clone, Debug, Serialize)]
pub struct OwnedRun {
    pub room: u32,
    pub region: u64,
    pub z: i32,
    pub y: i32,
    pub first_x: i32,
    pub last_x: i32,
}
#[derive(Deserialize)]
#[serde(remote = "AuthoredSpatialScope", deny_unknown_fields)]
struct ScopeFields {
    #[serde(deserialize_with = "bounded_vec")]
    members: Vec<u64>,
    #[serde(deserialize_with = "room_map")]
    rooms: BTreeMap<u32, Vec<RoomVolume>>,
    #[serde(deserialize_with = "bounded_vec")]
    passages: Vec<SourceRun>,
    #[serde(deserialize_with = "bounded_vec")]
    doorways: Vec<OwnedRun>,
}
#[derive(Deserialize)]
#[serde(remote = "RoomVolume", deny_unknown_fields)]
struct VolumeFields {
    region: u64,
    min: [i32; 3],
    max: [i32; 3],
}
#[derive(Deserialize)]
#[serde(remote = "SourceRun", deny_unknown_fields)]
struct RunFields {
    region: u64,
    z: i32,
    y: i32,
    first_x: i32,
    last_x: i32,
}
#[derive(Deserialize)]
#[serde(remote = "OwnedRun", deny_unknown_fields)]
struct OwnedFields {
    room: u32,
    region: u64,
    z: i32,
    y: i32,
    first_x: i32,
    last_x: i32,
}
macro_rules! named {
    ($value:ty,$fields:ident) => {
        impl<'de> Deserialize<'de> for $value {
            fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                struct Named;
                impl<'de> serde::de::Visitor<'de> for Named {
                    type Value = $value;
                    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        f.write_str("named spatial source record")
                    }
                    fn visit_map<A: serde::de::MapAccess<'de>>(
                        self,
                        map: A,
                    ) -> Result<Self::Value, A::Error> {
                        $fields::deserialize(serde::de::value::MapAccessDeserializer::new(map))
                    }
                }
                d.deserialize_map(Named)
            }
        }
    };
}
named!(AuthoredSpatialScope, ScopeFields);
named!(RoomVolume, VolumeFields);
named!(SourceRun, RunFields);
named!(OwnedRun, OwnedFields);

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn spatial_source_records_are_named_closed_and_duplicate_safe() {
        let valid = serde_json::json!({"members":[1],"rooms":{"1":[{"region":1,"min":[0,0,0],"max":[2,2,1]}]},"passages":[],"doorways":[]});
        let parsed: AuthoredSpatialScope = serde_json::from_value(valid.clone()).unwrap();
        assert_eq!(parsed.members, vec![1]);
        let encoded = toml::to_string(&parsed).unwrap();
        let roundtrip: AuthoredSpatialScope = toml::from_str(&encoded).unwrap();
        assert_eq!(serde_json::to_value(roundtrip).unwrap(), valid);
        assert!(encoded.contains("rooms"));
        for field in ["members", "rooms", "passages", "doorways"] {
            let mut missing = valid.clone();
            missing.as_object_mut().unwrap().remove(field);
            assert!(serde_json::from_value::<AuthoredSpatialScope>(missing).is_err());
        }
        assert!(serde_json::from_str::<AuthoredSpatialScope>(
            r#"{"members":[1],"members":[1],"rooms":{},"passages":[],"doorways":[]}"#
        )
        .is_err());
        assert!(serde_json::from_str::<AuthoredSpatialScope>(
            r#"{"members":[1],"rooms":{"1":[],"1":[]},"passages":[],"doorways":[]}"#
        )
        .is_err());
        let mut unknown = valid.clone();
        unknown["graph"] = serde_json::json!([]);
        assert!(serde_json::from_value::<AuthoredSpatialScope>(unknown).is_err());
        assert!(
            serde_json::from_value::<AuthoredSpatialScope>(serde_json::json!([[1], {}, [], []]))
                .is_err()
        );
        assert!(
            serde_json::from_value::<RoomVolume>(serde_json::json!([1, [0, 0, 0], [2, 2, 1]]))
                .is_err()
        );
        assert!(serde_json::from_value::<SourceRun>(serde_json::json!([1, 0, 0, 0, 2])).is_err());
        assert!(serde_json::from_value::<OwnedRun>(serde_json::json!([1, 1, 0, 0, 0, 2])).is_err());
        for value in [serde_json::Value::Null, serde_json::json!([])] {
            assert!(serde_json::from_value::<AuthoredSpatialScope>(value).is_err());
        }
    }
}
