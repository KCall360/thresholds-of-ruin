//! One dose has exactly one observable result. Permanent cap policy is applied
//! by the creature transaction, not encoded in this receipt.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StrengthGain {
    Unchanged,
    Restored,
    Acquired,
}
impl<'de> Deserialize<'de> for StrengthGain {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl serde::de::Visitor<'_> for Visitor {
            type Value = StrengthGain;
            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("unchanged, restored, or acquired")
            }
            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Self::Value, E> {
                match value {
                    "unchanged" => Ok(StrengthGain::Unchanged),
                    "restored" => Ok(StrengthGain::Restored),
                    "acquired" => Ok(StrengthGain::Acquired),
                    _ => Err(E::custom("invalid Strength gain result")),
                }
            }
        }
        deserializer.deserialize_str(Visitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn strength_gain_requires_one_closed_scalar_result() {
        for (result, spelling) in [
            (StrengthGain::Unchanged, "unchanged"),
            (StrengthGain::Restored, "restored"),
            (StrengthGain::Acquired, "acquired"),
        ] {
            let encoded = serde_json::to_string(&result).unwrap();
            assert_eq!(encoded, format!("\"{spelling}\""));
            assert_eq!(
                serde_json::from_str::<StrengthGain>(&encoded).unwrap(),
                result
            );
            assert!(
                serde_json::from_str::<StrengthGain>(&format!("{{\"{spelling}\":null}}")).is_err()
            );
        }
        for invalid in ["null", "0", "[]", "{}", "\"both\"", "\"\""] {
            assert!(serde_json::from_str::<StrengthGain>(invalid).is_err());
        }
    }
}
