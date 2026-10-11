//! Shared allocation bounds for untrusted definition and checkpoint sequences.
use serde::de::{Error, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use std::{fmt, marker::PhantomData};

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub(crate) struct Bounded<T, const MAXIMUM: usize>(pub Vec<T>);
impl<'de, T: Deserialize<'de>, const MAXIMUM: usize> Deserialize<'de> for Bounded<T, MAXIMUM> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Sequence<T, const MAXIMUM: usize>(PhantomData<T>);
        impl<'de, T: Deserialize<'de>, const MAXIMUM: usize> Visitor<'de> for Sequence<T, MAXIMUM> {
            type Value = Bounded<T, MAXIMUM>;
            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                write!(formatter, "at most {MAXIMUM} entries")
            }
            fn visit_seq<A: SeqAccess<'de>>(
                self,
                mut sequence: A,
            ) -> Result<Self::Value, A::Error> {
                if sequence.size_hint().is_some_and(|size| size > MAXIMUM) {
                    return Err(A::Error::custom("collection exceeds limit"));
                }
                let mut values = Vec::with_capacity(sequence.size_hint().unwrap_or(0).min(MAXIMUM));
                while values.len() < MAXIMUM {
                    let Some(value) = sequence.next_element()? else {
                        return Ok(Bounded(values));
                    };
                    values.push(value);
                }
                if sequence.next_element::<serde::de::IgnoredAny>()?.is_some() {
                    return Err(A::Error::custom("collection exceeds limit"));
                }
                Ok(Bounded(values))
            }
        }
        deserializer.deserialize_seq(Sequence::<T, MAXIMUM>(PhantomData))
    }
}

pub(crate) fn required_option<'de, T: Deserialize<'de>, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<T>, D::Error> {
    Option::<T>::deserialize(deserializer)
}

/// Named persistence objects never accept serde's alternate positional sequence form.
pub(crate) fn named_map<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::Deserialize<'de>,
{
    struct Named<T>(std::marker::PhantomData<T>);
    impl<'de, T: serde::Deserialize<'de>> serde::de::Visitor<'de> for Named<T> {
        type Value = T;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("a named record object")
        }
        fn visit_map<M: serde::de::MapAccess<'de>>(self, map: M) -> Result<T, M::Error> {
            T::deserialize(serde::de::value::MapAccessDeserializer::new(map))
        }
    }
    deserializer.deserialize_map(Named(std::marker::PhantomData))
}
