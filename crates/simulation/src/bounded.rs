//! Shared allocation bounds for untrusted definition and checkpoint sequences.
use serde::de::{Error, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use std::{fmt, marker::PhantomData};

#[derive(Clone, Debug, Serialize)]
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
