//! Save framing and strict JSON validation, independent of SQLite admission.
//! Valid stored shapes, checksums and existing byte/depth limits are preserved.
use crate::{
    engine::{invalid_archive, storage_failure},
    Failure,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};

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
    let original = serde_json::from_slice::<UniqueJson>(bytes)
        .map_err(|_| invalid_archive())?
        .0;
    if serde_json::to_value(&value).map_err(|_| invalid_archive())? != original {
        return Err(invalid_archive());
    }
    Ok(value)
}

// Typed maps otherwise silently retain the last duplicate key.
struct UniqueJson(serde_json::Value);
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
