//! Stored scenario metadata is independent of authoring and runtime serializers.
//! Remote schemas borrow values during encoding and move decoded fields into the
//! runtime. Package sources, caches and diagnostic text never enter this schema.
use crate::engine::{invalid_archive, storage_failure};
use crate::scenario_package as source;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

// Only explicitly registered values can cross this boundary. There is no
// Serialize/Deserialize blanket implementation that could reintroduce a runtime
// serializer through a nested field. Containers preserve that rule recursively.
trait Stored: Sized {
    fn encode<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error>;
    fn decode<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error>;
}

struct Borrowed<'a, T>(&'a T);
impl<T: Stored> Serialize for Borrowed<'_, T> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.encode(serializer)
    }
}

struct Owned<T>(T);
impl<'de, T: Stored> Deserialize<'de> for Owned<T> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        T::decode(deserializer).map(Self)
    }
}
impl<T: Stored> Serialize for Owned<T> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.encode(serializer)
    }
}

mod mapped {
    use super::*;
    pub(super) fn serialize<T: Stored, S: serde::Serializer>(
        value: &T,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        value.encode(serializer)
    }
    pub(super) fn deserialize<'de, T: Stored, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> Result<T, D::Error> {
        T::decode(deserializer)
    }
}

impl<T: Stored> Stored for Option<T> {
    fn encode<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.as_ref().map(Borrowed).serialize(serializer)
    }
    fn decode<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Option::<Owned<T>>::deserialize(deserializer).map(|value| value.map(|value| value.0))
    }
}
impl<T: Stored> Stored for Arc<T> {
    fn encode<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.as_ref().encode(serializer)
    }
    fn decode<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        T::decode(deserializer).map(Arc::new)
    }
}
impl<T: Stored> Stored for Vec<T> {
    fn encode<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_seq(self.iter().map(Borrowed))
    }
    fn decode<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Values<T>(std::marker::PhantomData<T>);
        impl<'de, T: Stored> serde::de::Visitor<'de> for Values<T> {
            type Value = Vec<T>;
            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a sequence of stored values")
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut sequence: A,
            ) -> Result<Self::Value, A::Error> {
                let mut values = Vec::new();
                while let Some(Owned(value)) = sequence.next_element()? {
                    values.push(value);
                }
                Ok(values)
            }
        }
        deserializer.deserialize_seq(Values::<T>(std::marker::PhantomData))
    }
}
impl<T: Stored + Ord> Stored for BTreeSet<T> {
    fn encode<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_seq(self.iter().map(Borrowed))
    }
    fn decode<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Values<T>(std::marker::PhantomData<T>);
        impl<'de, T: Stored + Ord> serde::de::Visitor<'de> for Values<T> {
            type Value = BTreeSet<T>;
            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a sequence of unique stored values")
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut sequence: A,
            ) -> Result<Self::Value, A::Error> {
                let mut values = BTreeSet::new();
                while let Some(Owned(value)) = sequence.next_element()? {
                    if !values.insert(value) {
                        return Err(serde::de::Error::custom("duplicate stored set value"));
                    }
                }
                Ok(values)
            }
        }
        deserializer.deserialize_seq(Values::<T>(std::marker::PhantomData))
    }
}
impl<K: Stored + Ord, V: Stored> Stored for BTreeMap<K, V> {
    fn encode<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_map(
            self.iter()
                .map(|(key, value)| (Borrowed(key), Borrowed(value))),
        )
    }
    fn decode<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Values<K, V>(std::marker::PhantomData<(K, V)>);
        impl<'de, K: Stored + Ord, V: Stored> serde::de::Visitor<'de> for Values<K, V> {
            type Value = BTreeMap<K, V>;
            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a mapping of unique stored keys to stored values")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut mapping: A,
            ) -> Result<Self::Value, A::Error> {
                let mut values = BTreeMap::new();
                while let Some((Owned(key), Owned(value))) = mapping.next_entry()? {
                    if values.insert(key, value).is_some() {
                        return Err(serde::de::Error::custom("duplicate stored mapping key"));
                    }
                }
                Ok(values)
            }
        }
        deserializer.deserialize_map(Values::<K, V>(std::marker::PhantomData))
    }
}

// Scalar leaves have fixed JSON representations; domain values below use only
// their save-owned remote schema, never their own serde implementation.
macro_rules! scalar {
    ($($value:ty),+ $(,)?) => {$(
        impl Stored for $value {
            fn encode<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                self.serialize(serializer)
            }
            fn decode<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                Self::deserialize(deserializer)
            }
        }
    )+};
}
scalar!(String, u32);

#[derive(Serialize, Deserialize)]
#[serde(remote = "crate::Scenario", deny_unknown_fields)]
struct Scenario {
    seed: u64,
    #[serde(with = "mapped")]
    actors: Vec<crate::ActorSetup>,
    regions: u64,
    workload_version: Option<u32>,
    #[serde(with = "mapped")]
    package: Option<Arc<source::Package>>,
    #[serde(with = "mapped")]
    streaming: Option<crate::Streaming>,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "crate::ActorSetup", deny_unknown_fields)]
struct ActorSetup {
    #[serde(with = "mapped")]
    position: crate::journal::Position,
    turn_ticks: u64,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "crate::journal::Position", deny_unknown_fields)]
struct Position {
    region: u64,
    x: i32,
    y: i32,
    z: i32,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "crate::Streaming", deny_unknown_fields)]
struct Streaming {
    active_radius: u32,
    load_radius: u32,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "source::Manifest", deny_unknown_fields)]
struct Manifest {
    factions: BTreeMap<String, BTreeSet<String>>,
    #[serde(with = "mapped")]
    ai_profiles: BTreeMap<String, source::AiProfile>,
    format: u32,
    id: String,
    version: String,
    ruleset: String,
    default_character: u64,
    themes: Vec<String>,
    #[serde(with = "mapped")]
    zones: BTreeMap<String, source::Zone>,
    #[serde(with = "mapped")]
    archetypes: BTreeMap<String, source::Archetype>,
    #[serde(with = "mapped")]
    appearance_pools: BTreeMap<String, source::AppearancePool>,
    #[serde(with = "mapped")]
    characters: Vec<source::Character>,
    #[serde(with = "mapped")]
    objective: Option<source::Objective>,
    assets: BTreeMap<String, Vec<String>>,
    #[serde(with = "mapped")]
    terrain: Option<source::TerrainAssets>,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "source::Zone", deny_unknown_fields)]
struct Zone {
    themes: Option<Vec<String>>,
    #[serde(with = "mapped")]
    terrain: Option<source::TerrainAssets>,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "source::TerrainAssets", deny_unknown_fields)]
struct TerrainAssets {
    floor: Option<String>,
    wall: Option<String>,
    door: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "source::AppearancePool", deny_unknown_fields)]
struct AppearancePool {
    appearances: Vec<String>,
    confounding: bool,
    asset: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "source::Archetype", deny_unknown_fields)]
struct Archetype {
    #[serde(with = "mapped")]
    combat: Option<source::CombatSpec>,
    #[serde(with = "mapped")]
    body: Option<source::BodySpec>,
    identity: Option<String>,
    appearance_pool: Option<String>,
    stackable: bool,
    properties: BTreeMap<String, String>,
    name: Option<String>,
    turn_ticks: Option<u64>,
    asset: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "source::Character", deny_unknown_fields)]
struct Character {
    #[serde(with = "mapped")]
    combat: Option<source::CombatSpec>,
    #[serde(with = "mapped")]
    body: Option<source::BodySpec>,
    velocity: Option<[i64; 3]>,
    known_identities: Vec<String>,
    id: u64,
    anchor: String,
    turn_ticks: u64,
    unselected: String,
    ai: Option<String>,
    asset: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "source::Objective", deny_unknown_fields)]
struct Objective {
    anchor: String,
    item: Option<u64>,
    disclosed: bool,
    continue_play: bool,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "source::AiProfile", deny_unknown_fields)]
struct AiProfile {
    memory_ticks: u64,
    flee_percent: u32,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "source::BodySpec", deny_unknown_fields)]
struct BodySpec {
    cells: Vec<[i32; 3]>,
    eye: [i32; 3],
    mass: u32,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "source::DamageType", rename_all = "snake_case")]
enum DamageType {
    Energy,
    Impact,
    Keen,
    Spirit,
    Vital,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "source::AttackSpec", deny_unknown_fields)]
struct AttackSpec {
    bonus: i32,
    wind_up: u64,
    recovery: u64,
    #[serde(with = "mapped")]
    damage: BTreeMap<source::DamageType, u32>,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "source::CombatSpec", deny_unknown_fields)]
struct CombatSpec {
    name: String,
    max_hp: u32,
    defense: i32,
    #[serde(with = "mapped")]
    attack: source::AttackSpec,
    #[serde(with = "mapped")]
    immunities: BTreeSet<source::DamageType>,
    #[serde(with = "mapped")]
    reductions: BTreeMap<source::DamageType, u32>,
    faction: String,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "source::Certificate", deny_unknown_fields)]
struct Certificate {
    validator: String,
    ruleset: String,
    content_hash: String,
    files: BTreeMap<String, String>,
    regions: usize,
    model_hash: String,
    coverage: String,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "source::RegionIndex", deny_unknown_fields)]
struct RegionIndex {
    #[serde(with = "mapped")]
    regions: Vec<source::IndexedRegion>,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "source::IndexedRegion", deny_unknown_fields)]
struct IndexedRegion {
    id: u64,
    file: String,
    hash: String,
    name: String,
    size: [i32; 3],
    chamber: bool,
    zone: Option<String>,
    anchors: BTreeMap<String, [i32; 3]>,
    portals: Vec<String>,
    #[serde(with = "mapped")]
    actors: Vec<source::IndexedActor>,
    #[serde(with = "mapped")]
    items: Vec<source::IndexedItem>,
    doors: Vec<u64>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    generated: bool,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "source::IndexedActor", deny_unknown_fields)]
struct IndexedActor {
    id: u64,
    at: [i32; 3],
    turn_ticks: Option<u64>,
    archetype: Option<String>,
    external: bool,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "source::IndexedItem", deny_unknown_fields)]
struct IndexedItem {
    id: u64,
    carried_by: Option<u64>,
}

// The macro only connects an explicit schema to its runtime type. All fields,
// tags and nested mappings remain visible in the declarations above.
macro_rules! remote {
    ($($schema:ident => $value:ty),+ $(,)?) => {$(
        impl Stored for $value {
            fn encode<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                $schema::serialize(self, serializer)
            }
            fn decode<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                $schema::deserialize(deserializer)
            }
        }
    )+};
}
remote!(
    Scenario => crate::Scenario,
    ActorSetup => crate::ActorSetup,
    Position => crate::journal::Position,
    Streaming => crate::Streaming,
    Manifest => source::Manifest,
    Zone => source::Zone,
    TerrainAssets => source::TerrainAssets,
    AppearancePool => source::AppearancePool,
    Archetype => source::Archetype,
    Character => source::Character,
    Objective => source::Objective,
    AiProfile => source::AiProfile,
    BodySpec => source::BodySpec,
    DamageType => source::DamageType,
    AttackSpec => source::AttackSpec,
    CombatSpec => source::CombatSpec,
    Certificate => source::Certificate,
    RegionIndex => source::RegionIndex,
    IndexedRegion => source::IndexedRegion,
    IndexedActor => source::IndexedActor,
    IndexedItem => source::IndexedItem,
);

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Package {
    #[serde(with = "mapped")]
    manifest: source::Manifest,
    #[serde(with = "mapped")]
    certificate: source::Certificate,
    validated: bool,
    selected: u64,
    directory: Option<String>,
}

impl Stored for source::Package {
    fn encode<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut value = serializer.serialize_struct("Package", 5)?;
        value.serialize_field("manifest", &Borrowed(&self.manifest))?;
        value.serialize_field("certificate", &Borrowed(&self.certificate))?;
        value.serialize_field("validated", &self.validated)?;
        value.serialize_field("selected", &self.selected)?;
        value.serialize_field("directory", &self.directory)?;
        value.end()
    }
    fn decode<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = Package::deserialize(deserializer)?;
        Ok(Self::from_saved_metadata(
            value.manifest,
            value.certificate,
            value.validated,
            value.selected,
            value.directory,
        ))
    }
}

pub(crate) fn serialize<S: serde::Serializer>(
    value: &crate::Scenario,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    value.encode(serializer)
}
pub(crate) fn deserialize<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<crate::Scenario, D::Error> {
    crate::Scenario::decode(deserializer)
}

pub(crate) fn encode_index(index: &source::RegionIndex) -> Result<Vec<u8>, crate::Failure> {
    let mut bytes = serde_json::to_vec(&Borrowed(index)).map_err(|_| storage_failure())?;
    bytes.push(b'\n');
    Ok(bytes)
}
pub(crate) fn decode_index(bytes: &[u8]) -> Result<source::RegionIndex, crate::Failure> {
    if bytes.len() as u64 > source::MAX_INDEX_BYTES {
        return Err(invalid_archive());
    }
    let Owned(index) = crate::storage::codec::strict::<Owned<source::RegionIndex>>(bytes)?;
    if !index.regions.windows(2).all(|pair| pair[0].id < pair[1].id) {
        return Err(invalid_archive());
    }
    Ok(index)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::codec::strict;
    use serde_json::Value;

    fn legacy_scenarios() -> BTreeMap<String, Value> {
        // Captured from the immutable PR79 writer, before these schemas existed.
        // Only machine-specific package directories were normalized to null.
        serde_json::from_str(include_str!("../fixtures/saved-scenarios-v22.json")).unwrap()
    }

    #[test]
    fn pre_refactor_scenarios_round_trip_without_changing_the_saved_schema() {
        for (name, expected) in legacy_scenarios() {
            let bytes = serde_json::to_vec(&expected).unwrap();
            let Owned(actual) = strict::<Owned<crate::Scenario>>(&bytes).unwrap();
            assert_eq!(
                serde_json::to_value(Borrowed(&actual)).unwrap(),
                expected,
                "{name}"
            );
            if let Some(package) = actual.package {
                assert!(
                    package.index.regions.is_empty(),
                    "index attachment stays separate"
                );
                assert_eq!(
                    package.sources.files_read(),
                    0,
                    "decode must not acquire regions"
                );
            }
        }
    }

    #[test]
    fn stored_scenario_coordinates_require_numbers_and_reject_unknown_fields() {
        let original = &legacy_scenarios()["diagnostic"];
        for (field, bad) in [
            ("region", Value::String("1".into())),
            ("x", Value::from(i64::from(i32::MAX) + 1)),
            ("unexpected", Value::Bool(true)),
        ] {
            let mut value = original.clone();
            value["actors"][0]["position"][field] = bad;
            assert!(
                strict::<Owned<crate::Scenario>>(&serde_json::to_vec(&value).unwrap()).is_err(),
                "{field}"
            );
        }
    }

    #[test]
    fn stored_metadata_rejects_unknown_nested_fields_at_each_owned_boundary() {
        let original = &legacy_scenarios()["first-dungeon"];
        for path in [
            "/streaming",
            "/package",
            "/package/certificate",
            "/package/manifest",
            "/package/manifest/archetypes/guardian",
            "/package/manifest/archetypes/guardian/combat",
            "/package/manifest/archetypes/guardian/combat/attack",
            "/package/manifest/characters/0",
        ] {
            let mut value = original.clone();
            value
                .pointer_mut(path)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .insert("unexpected".into(), Value::Bool(true));
            assert!(
                strict::<Owned<crate::Scenario>>(&serde_json::to_vec(&value).unwrap()).is_err(),
                "{path}"
            );
        }
    }

    #[test]
    fn all_stored_damage_names_preserve_their_pre_refactor_spellings() {
        for (value, name) in [
            (source::DamageType::Energy, "energy"),
            (source::DamageType::Impact, "impact"),
            (source::DamageType::Keen, "keen"),
            (source::DamageType::Spirit, "spirit"),
            (source::DamageType::Vital, "vital"),
        ] {
            let bytes = serde_json::to_vec(&Borrowed(&value)).unwrap();
            assert_eq!(bytes, format!("\"{name}\"").as_bytes());
            assert_eq!(
                strict::<Owned<source::DamageType>>(&bytes).unwrap().0,
                value
            );
        }
    }

    #[test]
    fn stored_damage_collections_reject_duplicate_and_unknown_names() {
        for bytes in [
            br#"{"impact":1,"impact":2}"#.as_slice(),
            br#"{"unknown":1}"#.as_slice(),
        ] {
            assert!(strict::<Owned<BTreeMap<source::DamageType, u32>>>(bytes).is_err());
        }
        for bytes in [
            br#"["impact","impact"]"#.as_slice(),
            br#"["unknown"]"#.as_slice(),
        ] {
            assert!(strict::<Owned<BTreeSet<source::DamageType>>>(bytes).is_err());
        }
    }

    #[test]
    fn stored_indexes_preserve_existing_bytes_and_generated_flag_omission() {
        for bytes in [
            include_bytes!("../../../scenarios/two-room/index.json").as_slice(),
            include_bytes!("../../../scenarios/tests/generated-filler/index.json").as_slice(),
            include_bytes!("../../../scenarios/first-dungeon/index.json").as_slice(),
        ] {
            let index = decode_index(bytes).unwrap();
            assert_eq!(encode_index(&index).unwrap(), bytes);
            assert_eq!(index.to_bytes().unwrap(), bytes);
        }
    }

    #[test]
    fn stored_index_rejects_nested_duplicates_and_noncanonical_optional_fields() {
        let text = include_str!("../../../scenarios/two-room/index.json");
        for bad in [
            text.replacen(
                "\"start\":[1,1,0]",
                "\"start\":[0,0,0],\"start\":[1,1,0]",
                1,
            ),
            text.replacen("\"zone\":null,", "", 1),
            text.replacen(
                "\"chamber\":true",
                "\"chamber\":true,\"generated\":false",
                1,
            ),
            text.replacen("\"items\":", "\"unexpected\":true,\"items\":", 1),
        ] {
            assert_ne!(bad, text);
            assert!(decode_index(bad.as_bytes()).is_err());
        }
    }

    #[test]
    fn stored_index_requires_unique_increasing_region_identities() {
        let bytes = include_bytes!("../../../scenarios/two-room/index.json");
        let original: Value = serde_json::from_slice(bytes).unwrap();
        let mut duplicate = original.clone();
        duplicate["regions"][1]["id"] = Value::from(1);
        assert!(decode_index(&serde_json::to_vec(&duplicate).unwrap()).is_err());
        let mut descending = original;
        descending["regions"].as_array_mut().unwrap().reverse();
        assert!(decode_index(&serde_json::to_vec(&descending).unwrap()).is_err());
    }
}
