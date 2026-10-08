//! Typed opaque entity references. Tokens identify disclosed entities, not scene
//! occurrences or authority. The server owns their derivation and resolution.
use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use std::{fmt, str::FromStr};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidTarget;

impl fmt::Display for InvalidTarget {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Invalid opaque interaction target")
    }
}

impl std::error::Error for InvalidTarget {}

fn nibble(byte: u8) -> Result<u8, InvalidTarget> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        _ => Err(InvalidTarget),
    }
}

macro_rules! target {
    ($name:ident, $kind:literal, $description:literal) => {
        #[doc = $description]
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name([u8; 32]);

        impl $name {
            /// Construct a token from the server's scoped derivation. A token
            /// alone grants no authority and must still be resolved by the server.
            pub const fn from_digest(digest: [u8; 32]) -> Self {
                Self(digest)
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                const HEX: &[u8; 16] = b"0123456789abcdef";
                let mut token = [0_u8; 66];
                token[0] = $kind;
                token[1] = b'_';
                for (index, byte) in self.0.iter().copied().enumerate() {
                    token[index * 2 + 2] = HEX[usize::from(byte >> 4)];
                    token[index * 2 + 3] = HEX[usize::from(byte & 15)];
                }
                formatter.write_str(std::str::from_utf8(&token).expect("ASCII target encoding"))
            }
        }

        impl FromStr for $name {
            type Err = InvalidTarget;

            fn from_str(token: &str) -> Result<Self, Self::Err> {
                let bytes = token.as_bytes();
                if bytes.len() != 66 || bytes[0] != $kind || bytes[1] != b'_' {
                    return Err(InvalidTarget);
                }
                let mut digest = [0_u8; 32];
                for (index, pair) in bytes[2..].chunks_exact(2).enumerate() {
                    digest[index] = nibble(pair[0])? * 16 + nibble(pair[1])?;
                }
                Ok(Self(digest))
            }
        }

        impl Serialize for $name {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.collect_str(self)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                struct Visitor;
                impl de::Visitor<'_> for Visitor {
                    type Value = $name;

                    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                        formatter.write_str("a canonical typed opaque interaction target")
                    }

                    fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
                        value.parse().map_err(E::custom)
                    }
                }
                deserializer.deserialize_str(Visitor)
            }
        }
    };
}

target!(
    ActorTarget,
    b'a',
    "Observer-scoped reference to a disclosed actor."
);
target!(
    ItemTarget,
    b'i',
    "Observer-scoped reference to a disclosed item."
);
target!(
    DoorTarget,
    b'd',
    "Observer-scoped reference to a disclosed door."
);
