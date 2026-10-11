pub(super) use crate::bounded::{required_option, Bounded};
use serde::de::{Error, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use std::fmt;

#[derive(Clone, Debug, Serialize)]
#[serde(transparent)]
pub(super) struct Identifier(pub String);
impl<'de> Deserialize<'de> for Identifier {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Text;
        impl Visitor<'_> for Text {
            type Value = Identifier;
            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("1..60 ASCII identifier characters")
            }
            fn visit_str<E: Error>(self, value: &str) -> Result<Self::Value, E> {
                if value.is_empty()
                    || value.len() > 60
                    || !value
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
                {
                    return Err(E::custom("invalid identifier"));
                }
                Ok(Identifier(value.to_owned()))
            }
        }
        deserializer.deserialize_str(Text)
    }
}
