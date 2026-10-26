//! Closed captured directional policies; payment remains on the parent RayPlan.
use super::{ray::RayIncomingSource, TokenClaim};
use crate::attributes::Skill;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum RayTraversal {
    Bounded { clear_visits: u16 },
    FirstBody,
}
impl<'de> Deserialize<'de> for RayTraversal {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            Bounded { clear_visits: u16 },
            FirstBody {},
        }
        match crate::bounded::named_map(d)? {
            Fields::Bounded { clear_visits }
                if (1..=super::ray::MAX_RAY_VISITS).contains(&clear_visits) =>
            {
                Ok(Self::Bounded { clear_visits })
            }
            Fields::FirstBody {} => Ok(Self::FirstBody),
            _ => Err(serde::de::Error::custom("invalid ray traversal")),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum RayResolution {
    Checkless,
    Opposed {
        source_skill: Skill,
        target_skill: Skill,
        source_bonus: i64,
        source_edge: i64,
        effect_edge: i64,
        incoming: Vec<RayIncomingSource>,
        tokens: Vec<TokenClaim>,
    },
}
impl<'de> Deserialize<'de> for RayResolution {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            Checkless {},
            Opposed {
                source_skill: Skill,
                target_skill: Skill,
                source_bonus: i64,
                source_edge: i64,
                effect_edge: i64,
                #[serde(deserialize_with = "super::ray::read_incoming")]
                incoming: Vec<RayIncomingSource>,
                #[serde(deserialize_with = "super::strikes::read_claims")]
                tokens: Vec<TokenClaim>,
            },
        }
        Ok(match crate::bounded::named_map(d)? {
            Fields::Checkless {} => Self::Checkless,
            Fields::Opposed {
                source_skill,
                target_skill,
                source_bonus,
                source_edge,
                effect_edge,
                incoming,
                tokens,
            } => Self::Opposed {
                source_skill,
                target_skill,
                source_bonus,
                source_edge,
                effect_edge,
                incoming,
                tokens,
            },
        })
    }
}
impl RayResolution {
    pub fn tokens(&self) -> &[TokenClaim] {
        match self {
            Self::Checkless => &[],
            Self::Opposed { tokens, .. } => tokens,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn contact_policies_have_no_fabricated_range_or_check_operands() {
        let checkless: RayResolution = serde_json::from_str(r#"{"kind":"checkless"}"#).unwrap();
        assert!(checkless.tokens().is_empty());
        assert_eq!(
            serde_json::to_string(&checkless).unwrap(),
            r#"{"kind":"checkless"}"#
        );
        for invalid in [
            r#""checkless""#,
            r#"{"kind":"checkless","source_skill":"lore"}"#,
            r#"{"kind":"checkless","tokens":[]}"#,
            r#"{"kind":"opposed"}"#,
        ] {
            assert!(serde_json::from_str::<RayResolution>(invalid).is_err());
        }
        assert_eq!(
            serde_json::from_str::<RayTraversal>(r#"{"kind":"first_body"}"#).unwrap(),
            RayTraversal::FirstBody
        );
        for invalid in [
            r#"{"kind":"first_body","clear_visits":64}"#,
            r#"{"kind":"bounded","clear_visits":0}"#,
            r#"{"kind":"bounded"}"#,
        ] {
            assert!(serde_json::from_str::<RayTraversal>(invalid).is_err());
        }
    }
}
