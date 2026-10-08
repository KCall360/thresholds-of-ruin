//! Save framing and strict JSON validation, independent of SQLite admission.
//! Valid stored shapes, checksums and existing byte/depth limits are preserved.
use crate::{
    engine::{invalid_archive, storage_failure},
    Failure,
};
#[cfg(test)]
use serde::Deserialize;
use serde::{
    de::{DeserializeOwned, DeserializeSeed},
    Serialize,
};

pub const MAX_PAYLOAD: usize = 1024 * 1024;
/// A region record row may be larger than a journal record.
pub(super) const MAX_REGION: usize = 16 * MAX_PAYLOAD;

pub(super) fn crc32c(bytes: impl Iterator<Item = u8>) -> u32 {
    let mut crc = !0u32;
    for byte in bytes {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0x82f63b78 & 0u32.wrapping_sub(crc & 1));
        }
    }
    !crc
}
pub(crate) fn frame<T: Serialize>(kind: u16, sequence: u64, value: &T) -> Result<Vec<u8>, Failure> {
    let payload = serde_json::to_vec(value).map_err(|_| storage_failure())?;
    if payload.len() > MAX_PAYLOAD {
        return Err(storage_failure());
    }
    let mut bytes = Vec::with_capacity(24 + payload.len());
    bytes.extend_from_slice(if kind == 0 { b"TORB" } else { b"TORJ" });
    bytes.extend_from_slice(&6u16.to_le_bytes());
    bytes.extend_from_slice(&kind.to_le_bytes());
    bytes.extend_from_slice(&sequence.to_le_bytes());
    bytes.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    let crc = crc32c(bytes[4..].iter().copied().chain(payload.iter().copied()));
    bytes.extend_from_slice(&crc.to_le_bytes());
    bytes.extend_from_slice(&payload);
    Ok(bytes)
}
/// A region record row: the journal frame layout with magic `TORR`, kind 3
/// and the record identity in the sequence field.
pub(super) fn region_frame(
    id: u64,
    record: &tor_simulation::RegionRecord,
) -> Result<Vec<u8>, Failure> {
    let payload = serde_json::to_vec(record).map_err(|_| storage_failure())?;
    if payload.len() > MAX_REGION {
        return Err(storage_failure());
    }
    let mut bytes = Vec::with_capacity(24 + payload.len());
    bytes.extend_from_slice(b"TORR");
    bytes.extend_from_slice(&6u16.to_le_bytes());
    bytes.extend_from_slice(&3u16.to_le_bytes());
    bytes.extend_from_slice(&id.to_le_bytes());
    bytes.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    let crc = crc32c(bytes[4..].iter().copied().chain(payload.iter().copied()));
    bytes.extend_from_slice(&crc.to_le_bytes());
    bytes.extend_from_slice(&payload);
    Ok(bytes)
}
pub(super) fn decode_region(
    bytes: &[u8],
    id: u64,
) -> Result<tor_simulation::RegionRecord, Failure> {
    if bytes.len() < 24
        || bytes.len() > MAX_REGION + 24
        || &bytes[..4] != b"TORR"
        || bytes[4..6] != 6u16.to_le_bytes()
        || bytes[6..8] != 3u16.to_le_bytes()
        || u64::from_le_bytes(bytes[8..16].try_into().unwrap()) != id
        || u32::from_le_bytes(bytes[16..20].try_into().unwrap()) as usize != bytes.len() - 24
        || crc32c(
            bytes[4..20]
                .iter()
                .copied()
                .chain(bytes[24..].iter().copied()),
        ) != u32::from_le_bytes(bytes[20..24].try_into().unwrap())
    {
        return Err(invalid_archive());
    }
    strict(&bytes[24..])
}
pub(super) fn decode(bytes: &[u8], sequence: u64) -> Result<(u16, &[u8]), Failure> {
    if bytes.len() < 24 || bytes.len() > MAX_PAYLOAD + 24 {
        return Err(invalid_archive());
    }
    let kind = u16::from_le_bytes(bytes[6..8].try_into().unwrap());
    let magic = if sequence == 0 { b"TORB" } else { b"TORJ" };
    if &bytes[..4] != magic
        || bytes[4..6] != 6u16.to_le_bytes()
        || u64::from_le_bytes(bytes[8..16].try_into().unwrap()) != sequence
        || u32::from_le_bytes(bytes[16..20].try_into().unwrap()) as usize != bytes.len() - 24
        || crc32c(
            bytes[4..20]
                .iter()
                .copied()
                .chain(bytes[24..].iter().copied()),
        ) != u32::from_le_bytes(bytes[20..24].try_into().unwrap())
        || (sequence == 0 && kind != 0)
        || (sequence != 0 && !matches!(kind, 1 | 2))
    {
        return Err(invalid_archive());
    }
    Ok((kind, &bytes[24..]))
}
pub(super) fn strict<T: DeserializeOwned + Serialize>(bytes: &[u8]) -> Result<T, Failure> {
    let value: T = serde_json::from_slice(bytes).map_err(|_| invalid_archive())?;
    // Keep one canonical tree, then compare the input without retaining a
    // second tree. Canonical comparison also rejects serde aliases, skipped
    // fields, missing serialized defaults and noncanonical numeric map keys.
    let canonical = serde_json::to_value(&value).map_err(|_| invalid_archive())?;
    let mut input = serde_json::Deserializer::from_slice(bytes);
    SameJson(&canonical)
        .deserialize(&mut input)
        .map_err(|_| invalid_archive())?;
    input.end().map_err(|_| invalid_archive())?;
    Ok(value)
}

/// Validate decoded keys and values against the canonical shape. Each object
/// retains only its seen keys; nested input values are compared and discarded.
struct SameJson<'a>(&'a serde_json::Value);

impl SameJson<'_> {
    fn scalar<E: serde::de::Error>(self, value: serde_json::Value) -> Result<(), E> {
        if self.0 == &value {
            Ok(())
        } else {
            Err(E::custom("noncanonical JSON value"))
        }
    }
}

impl<'de> DeserializeSeed<'de> for SameJson<'_> {
    type Value = ();
    fn deserialize<D: serde::Deserializer<'de>>(self, input: D) -> Result<(), D::Error> {
        input.deserialize_any(self)
    }
}

impl<'de> serde::de::Visitor<'de> for SameJson<'_> {
    type Value = ();
    fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str("the canonical JSON shape without duplicate keys")
    }
    fn visit_bool<E: serde::de::Error>(self, value: bool) -> Result<(), E> {
        self.scalar(value.into())
    }
    fn visit_i64<E: serde::de::Error>(self, value: i64) -> Result<(), E> {
        self.scalar(value.into())
    }
    fn visit_u64<E: serde::de::Error>(self, value: u64) -> Result<(), E> {
        self.scalar(value.into())
    }
    fn visit_f64<E: serde::de::Error>(self, value: f64) -> Result<(), E> {
        self.scalar(value.into())
    }
    fn visit_unit<E: serde::de::Error>(self) -> Result<(), E> {
        self.scalar(serde_json::Value::Null)
    }
    fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<(), E> {
        if self.0.as_str() == Some(value) {
            Ok(())
        } else {
            Err(E::custom("noncanonical JSON string"))
        }
    }
    fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut input: A) -> Result<(), A::Error> {
        let values = self
            .0
            .as_array()
            .ok_or_else(|| serde::de::Error::custom("noncanonical JSON array"))?;
        for value in values {
            if input.next_element_seed(SameJson(value))?.is_none() {
                return Err(serde::de::Error::custom("missing JSON array element"));
            }
        }
        if input.next_element::<serde::de::IgnoredAny>()?.is_some() {
            return Err(serde::de::Error::custom("extra JSON array element"));
        }
        Ok(())
    }
    fn visit_map<A: serde::de::MapAccess<'de>>(self, mut input: A) -> Result<(), A::Error> {
        let values = self
            .0
            .as_object()
            .ok_or_else(|| serde::de::Error::custom("noncanonical JSON object"))?;
        let mut seen = std::collections::BTreeSet::new();
        while let Some(key) = input.next_key::<String>()? {
            let value = values
                .get(&key)
                .ok_or_else(|| serde::de::Error::custom("noncanonical JSON key"))?;
            if !seen.insert(key) {
                return Err(serde::de::Error::custom("duplicate JSON key"));
            }
            input.next_value_seed(SameJson(value))?;
        }
        if seen.len() != values.len() {
            return Err(serde::de::Error::custom("missing JSON key"));
        }
        Ok(())
    }
}

// Typed maps otherwise silently retain the last duplicate key.
#[cfg(test)]
struct UniqueJson(serde_json::Value);
#[cfg(test)]
impl<'de> Deserialize<'de> for UniqueJson {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = UniqueJson;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("JSON without duplicate keys")
            }
            fn visit_bool<E: serde::de::Error>(self, v: bool) -> Result<UniqueJson, E> {
                Ok(UniqueJson(v.into()))
            }
            fn visit_i64<E: serde::de::Error>(self, v: i64) -> Result<UniqueJson, E> {
                Ok(UniqueJson(v.into()))
            }
            fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<UniqueJson, E> {
                Ok(UniqueJson(v.into()))
            }
            fn visit_f64<E: serde::de::Error>(self, v: f64) -> Result<UniqueJson, E> {
                Ok(UniqueJson(v.into()))
            }
            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<UniqueJson, E> {
                Ok(UniqueJson(v.into()))
            }
            fn visit_unit<E: serde::de::Error>(self) -> Result<UniqueJson, E> {
                Ok(UniqueJson(serde_json::Value::Null))
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut a: A,
            ) -> Result<UniqueJson, A::Error> {
                let mut values = Vec::new();
                while let Some(v) = a.next_element::<UniqueJson>()? {
                    values.push(v.0);
                }
                Ok(UniqueJson(values.into()))
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut a: A,
            ) -> Result<UniqueJson, A::Error> {
                let mut values = serde_json::Map::new();
                while let Some(key) = a.next_key::<String>()? {
                    if values.contains_key(&key) {
                        return Err(serde::de::Error::custom("duplicate key"));
                    }
                    values.insert(key, a.next_value::<UniqueJson>()?.0);
                }
                Ok(UniqueJson(values.into()))
            }
        }
        deserializer.deserialize_any(Visitor)
    }
}
#[cfg(test)]
mod tests {
    use super::super::Marker;
    use super::*;

    #[test]
    fn strict_json_rejects_nested_duplicate_keys() {
        assert!(strict::<serde_json::Value>(br#"{"map":{"a":1,"a":1}}"#).is_err());
        assert!(strict::<Marker>(br#"{"save_id":"id","generation":0}"#).is_err());
    }

    #[test]
    fn strict_json_keeps_aliases_defaults_and_skipped_fields_canonical() {
        #[derive(Debug, PartialEq, Serialize, Deserialize)]
        struct Fixture {
            #[serde(alias = "old_name")]
            name: String,
            #[serde(default)]
            count: u64,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            note: Option<String>,
            #[serde(skip)]
            transient: bool,
        }
        for (input, accepted) in [
            (r#"{"name":"snow \u2603","count":0}"#, true),
            (
                r#"{"count":18446744073709551615,"name":"snow ☃","note":""}"#,
                true,
            ),
            (r#"{"old_name":"snow ☃","count":0}"#, false),
            (r#"{"name":"snow ☃"}"#, false),
            (r#"{"name":"snow ☃","count":0,"note":null}"#, false),
            (r#"{"name":"snow ☃","count":0,"transient":false}"#, false),
            (r#"{"name":"snow ☃","count":0,"extra":{}}"#, false),
            (r#"{"name":"snow ☃","count":0,"co\u0075nt":0}"#, false),
        ] {
            assert_eq!(
                strict::<Fixture>(input.as_bytes()).is_ok(),
                accepted,
                "{input}"
            );
        }
    }

    #[test]
    fn strict_json_matches_legacy_validation_for_nested_scalar_collections() {
        #[derive(Debug, PartialEq, Serialize, Deserialize)]
        struct Fixture {
            signed: i64,
            unsigned: u64,
            fraction: f64,
            strings: Vec<Option<String>>,
            flags: std::collections::BTreeMap<String, Vec<bool>>,
        }
        for size in [0, 1, 32, 256] {
            let fixture = Fixture {
                signed: i64::MIN,
                unsigned: u64::MAX,
                fraction: -0.125,
                strings: (0..size)
                    .map(|n| (n % 2 == 0).then(|| format!("{n}\n\"☃")))
                    .collect(),
                flags: (0..size)
                    .map(|n| (format!("key-{n}"), vec![true, false]))
                    .collect(),
            };
            let canonical = serde_json::to_value(&fixture).unwrap();
            let mut variants = vec![canonical.clone()];
            for name in ["signed", "unsigned", "fraction", "strings", "flags"] {
                let mut omitted = canonical.clone();
                omitted.as_object_mut().unwrap().remove(name);
                variants.push(omitted);
                let mut wrong = canonical.clone();
                wrong[name] = serde_json::json!({"wrong": []});
                variants.push(wrong);
            }
            let mut unknown = canonical.clone();
            unknown["unknown"] = serde_json::json!([null, 0, {}]);
            variants.push(unknown);
            let mut extra = canonical.clone();
            extra["strings"]
                .as_array_mut()
                .unwrap()
                .push(serde_json::json!("extra"));
            variants.push(extra);
            for input in variants {
                let bytes = serde_json::to_vec(&input).unwrap();
                let legacy = serde_json::from_slice::<Fixture>(&bytes)
                    .ok()
                    .and_then(|typed| {
                        let original = serde_json::from_slice::<UniqueJson>(&bytes).ok()?.0;
                        (serde_json::to_value(&typed).ok()? == original).then_some(typed)
                    });
                assert_eq!(strict::<Fixture>(&bytes).ok(), legacy);
            }
        }
    }

    #[test]
    fn strict_json_round_trips_canonical_maps_at_each_size() {
        for size in [16, 256, 4096] {
            let expected: std::collections::BTreeMap<u64, String> = (0..size)
                .map(|id| (id, format!("value-{id}-{}", "x".repeat(80))))
                .collect();
            let bytes = serde_json::to_vec(&expected).unwrap();
            let actual = strict::<std::collections::BTreeMap<u64, String>>(&bytes).unwrap();
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn strict_json_preserves_canonical_shapes_and_rejections() {
        #[derive(Debug, PartialEq, Serialize, Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fixture {
            entries: std::collections::BTreeMap<u64, Vec<String>>,
            #[serde(default)]
            enabled: bool,
        }

        fn reference<T: DeserializeOwned + Serialize>(bytes: &[u8]) -> Option<T> {
            let typed: T = serde_json::from_slice(bytes).ok()?;
            let original = serde_json::from_slice::<UniqueJson>(bytes).ok()?.0;
            (serde_json::to_value(&typed).ok()? == original).then_some(typed)
        }

        for (text, valid) in [
            (r#"{"entries":{},"enabled":false}"#, true),
            (
                r#" { "enabled": true, "entries": {"18446744073709551615": ["escaped\"", "\u2603"], "0": []} } "#,
                true,
            ),
            (r#"{"entries":{},"enabled":false,"entries":{}}"#, false),
            (r#"{"entries":{},"enabled":false,"entr\u0069es":{}}"#, false),
            (r#"{"entries":{"1":[],"1":[]},"enabled":false}"#, false),
            (r#"{"entries":{"01":[]},"enabled":false}"#, false),
            (r#"{"entries":{"1":[],"01":[]},"enabled":false}"#, false),
            (
                r#"{"entries":{"18446744073709551616":[]},"enabled":false}"#,
                false,
            ),
            (r#"{"entries":{"-1":[]},"enabled":false}"#, false),
            (r#"{"entries":{},"enabled":false,"unknown":0}"#, false),
            (r#"{"entries":{}}"#, false),
            (r#"{"entries":{},"enabled":"false"}"#, false),
            (r#"{"entries":{"0":null},"enabled":false}"#, false),
            (r#"{"entries":{},"enabled":false} null"#, false),
        ] {
            let expected = reference::<Fixture>(text.as_bytes());
            assert_eq!(expected.is_some(), valid, "reference: {text}");
            assert_eq!(strict::<Fixture>(text.as_bytes()).ok(), expected, "{text}");
        }

        for depth in [16, 96, 160] {
            let text = format!("{}0{}", "[".repeat(depth), "]".repeat(depth));
            let expected = reference::<serde_json::Value>(text.as_bytes());
            assert_eq!(expected.is_some(), depth < 128);
            assert_eq!(strict::<serde_json::Value>(text.as_bytes()).ok(), expected);
        }
    }
}
