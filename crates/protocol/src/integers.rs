//! Canonical decimal strings for wire integers wider than JavaScript's exact range.
//! Rust values remain integers; small coordinates, versions and bounded counts remain numbers.
use serde::{Deserialize, Serialize};

fn magnitude(text: &str) -> bool {
    !text.is_empty()
        && (text == "0" || !text.starts_with('0'))
        && text.bytes().all(|byte| byte.is_ascii_digit())
}

pub(crate) mod unsigned {
    use super::*;
    pub(crate) fn serialize<S: serde::Serializer>(
        value: &u64,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        serializer.collect_str(value)
    }
    pub(crate) fn deserialize<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> Result<u64, D::Error> {
        struct Visitor;
        impl serde::de::Visitor<'_> for Visitor {
            type Value = u64;
            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a canonical unsigned decimal string")
            }
            fn visit_str<E: serde::de::Error>(self, text: &str) -> Result<u64, E> {
                if text.len() > 20 || !magnitude(text) {
                    return Err(E::custom("invalid unsigned decimal string"));
                }
                text.parse()
                    .map_err(|_| E::custom("unsigned decimal string exceeds u64"))
            }
        }
        deserializer.deserialize_str(Visitor)
    }
}

pub(crate) mod signed {
    use super::*;
    pub(crate) fn serialize<S: serde::Serializer>(
        value: &i64,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        serializer.collect_str(value)
    }
    pub(crate) fn deserialize<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> Result<i64, D::Error> {
        struct Visitor;
        impl serde::de::Visitor<'_> for Visitor {
            type Value = i64;
            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a canonical signed decimal string")
            }
            fn visit_str<E: serde::de::Error>(self, text: &str) -> Result<i64, E> {
                if text.len() > 20
                    || text == "-0"
                    || !magnitude(text.strip_prefix('-').unwrap_or(text))
                {
                    return Err(E::custom("invalid signed decimal string"));
                }
                text.parse()
                    .map_err(|_| E::custom("signed decimal string exceeds i64"))
            }
        }
        deserializer.deserialize_str(Visitor)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(transparent)]
struct Unsigned(#[serde(with = "unsigned")] u64);
#[derive(Serialize, Deserialize)]
#[serde(transparent)]
struct Signed(#[serde(with = "signed")] i64);

pub(crate) mod optional_unsigned {
    use super::*;
    pub(crate) fn serialize<S: serde::Serializer>(
        value: &Option<u64>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        value.map(Unsigned).serialize(serializer)
    }
    pub(crate) fn deserialize<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<u64>, D::Error> {
        Option::<Unsigned>::deserialize(deserializer).map(|value| value.map(|value| value.0))
    }
}

pub(crate) mod signed_triple {
    use super::*;
    pub(crate) fn serialize<S: serde::Serializer>(
        value: &[i64; 3],
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        value.map(Signed).serialize(serializer)
    }
    pub(crate) fn deserialize<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> Result<[i64; 3], D::Error> {
        <[Signed; 3]>::deserialize(deserializer).map(|values| values.map(|value| value.0))
    }
}
