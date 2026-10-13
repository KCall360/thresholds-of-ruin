//! Save-owned numeric advancement operands. No wire serializer defines these bytes.
use super::named_map;
use serde::{Deserialize, Serialize};
use tor_simulation::{self as s, player_advancement as a, player_respec as r};

fn required_option<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> Result<Option<T>, D::Error> {
    Option::deserialize(d)
}
fn label(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 128 && !value.chars().any(char::is_control)
}

#[derive(Serialize)]
struct Basis<'a> {
    actor: u64,
    branch: &'a str,
    stream: &'a str,
    reset: u64,
    build_epoch: u64,
}
pub(super) mod basis {
    use super::*;
    pub(crate) fn serialize<S: serde::Serializer>(
        value: &a::AdvancementBasis,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        Basis {
            actor: value.actor.0,
            branch: &value.branch,
            stream: &value.stream,
            reset: value.reset,
            build_epoch: value.build_epoch.get(),
        }
        .serialize(serializer)
    }
    pub(crate) fn deserialize<'de, D: serde::Deserializer<'de>>(
        d: D,
    ) -> Result<Box<a::AdvancementBasis>, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            actor: u64,
            branch: String,
            stream: String,
            reset: u64,
            build_epoch: u64,
        }
        let f: Fields = named_map(d)?;
        if f.actor == 0 || f.actor == u64::MAX || !label(&f.branch) || !label(&f.stream) {
            return Err(serde::de::Error::custom("invalid stored advancement basis"));
        }
        Ok(Box::new(a::AdvancementBasis {
            actor: s::ActorId(f.actor),
            branch: f.branch,
            stream: f.stream,
            reset: f.reset,
            build_epoch: s::creatures::BuildEpoch::new(f.build_epoch)
                .ok_or_else(|| serde::de::Error::custom("invalid stored build epoch"))?,
        }))
    }
}
#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Choice {
    SkillRank { skill: String },
    Talent { talent: String },
}
impl<'de> Deserialize<'de> for Choice {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            SkillRank { skill: String },
            Talent { talent: String },
        }
        Ok(match named_map(d)? {
            Fields::SkillRank { skill } => Self::SkillRank { skill },
            Fields::Talent { talent } => Self::Talent { talent },
        })
    }
}
impl From<&a::PurchaseChoice> for Choice {
    fn from(value: &a::PurchaseChoice) -> Self {
        match value {
            a::PurchaseChoice::SkillRank { skill } => Self::SkillRank {
                skill: skill.as_str().into(),
            },
            a::PurchaseChoice::Talent { talent } => Self::Talent {
                talent: talent.to_string(),
            },
        }
    }
}
impl Choice {
    fn domain<E: serde::de::Error>(self) -> Result<a::PurchaseChoice, E> {
        Ok(match self {
            Self::SkillRank { skill } => a::PurchaseChoice::SkillRank {
                skill: s::attributes::Skill::ALL
                    .into_iter()
                    .find(|value| value.as_str() == skill)
                    .ok_or_else(|| E::custom("invalid stored skill"))?,
            },
            Self::Talent { talent } => a::PurchaseChoice::Talent {
                talent: s::content_ids::TalentId::new(talent)
                    .map_err(|_| E::custom("invalid stored talent"))?,
            },
        })
    }
}
#[derive(Serialize)]
struct Purchase {
    owner: u16,
    pool: String,
    selection: Choice,
}
pub(super) mod purchase {
    use super::*;
    pub(crate) fn serialize<S: serde::Serializer>(
        value: &a::PurchaseOperation,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        Purchase {
            owner: value.owner,
            pool: value.pool.to_string(),
            selection: Choice::from(&value.selection),
        }
        .serialize(serializer)
    }
    pub(crate) fn deserialize<'de, D: serde::Deserializer<'de>>(
        d: D,
    ) -> Result<a::PurchaseOperation, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            owner: u16,
            pool: String,
            selection: Choice,
        }
        let f: Fields = named_map(d)?;
        if usize::from(f.owner) >= s::limits::HIT_DICE {
            return Err(serde::de::Error::custom("invalid stored owner"));
        }
        Ok(a::PurchaseOperation {
            owner: f.owner,
            pool: s::content_ids::PointPoolId::new(f.pool)
                .map_err(|_| serde::de::Error::custom("invalid stored point pool"))?,
            selection: f.selection.domain()?,
        })
    }
}
#[derive(Serialize)]
struct Source {
    owner: u16,
    index: u16,
}
impl<'de> Deserialize<'de> for Source {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            owner: u16,
            index: u16,
        }
        let f: Fields = named_map(d)?;
        if usize::from(f.owner) >= s::limits::HIT_DICE
            || usize::from(f.index) >= s::limits::POINTS_PER_HIT_DIE
        {
            return Err(serde::de::Error::custom("invalid stored source"));
        }
        Ok(Self {
            owner: f.owner,
            index: f.index,
        })
    }
}
#[derive(Serialize)]
struct ReplacementPurchase {
    pool: String,
    selection: Choice,
}
impl<'de> Deserialize<'de> for ReplacementPurchase {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            pool: String,
            selection: Choice,
        }
        let f: Fields = named_map(d)?;
        Ok(Self {
            pool: f.pool,
            selection: f.selection,
        })
    }
}
#[derive(Serialize)]
struct Replacement {
    source: Source,
    replacement: Option<ReplacementPurchase>,
}
pub(super) mod replacement {
    use super::*;
    pub(crate) fn serialize<S: serde::Serializer>(
        value: &r::ReplacementOperation,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        Replacement {
            source: Source {
                owner: value.source.owner,
                index: value.source.index,
            },
            replacement: value.replacement.as_ref().map(|value| ReplacementPurchase {
                pool: value.pool.to_string(),
                selection: Choice::from(&value.selection),
            }),
        }
        .serialize(serializer)
    }
    pub(crate) fn deserialize<'de, D: serde::Deserializer<'de>>(
        d: D,
    ) -> Result<r::ReplacementOperation, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            source: Source,
            #[serde(deserialize_with = "required_option")]
            replacement: Option<ReplacementPurchase>,
        }
        let f: Fields = named_map(d)?;
        Ok(r::ReplacementOperation {
            source: r::ReplacementSource {
                owner: f.source.owner,
                index: f.source.index,
            },
            replacement: f
                .replacement
                .map(|value| {
                    Ok(r::ReplacementPurchase {
                        pool: s::content_ids::PointPoolId::new(value.pool)
                            .map_err(|_| serde::de::Error::custom("invalid stored point pool"))?,
                        selection: value.selection.domain()?,
                    })
                })
                .transpose()?,
        })
    }
}
// Narrow named payload readers cover only the new save variants.
pub(super) fn read_purchase<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<(Box<a::AdvancementBasis>, a::PurchaseOperation), D::Error> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Fields {
        #[serde(with = "basis")]
        basis: Box<a::AdvancementBasis>,
        #[serde(with = "purchase")]
        purchase: a::PurchaseOperation,
    }
    let f: Fields = named_map(d)?;
    Ok((f.basis, f.purchase))
}
pub(super) fn read_respec<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<
    (
        Box<a::AdvancementBasis>,
        r::ReplacementOperation,
        super::SupplySelection,
    ),
    D::Error,
> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Fields {
        #[serde(with = "basis")]
        basis: Box<a::AdvancementBasis>,
        #[serde(with = "replacement")]
        replacement: r::ReplacementOperation,
        #[serde(deserialize_with = "named_map")]
        supplies: super::SupplySelection,
    }
    let f: Fields = named_map(d)?;
    // Respec has exactly one policy-owned cost ordinal and one physical source.
    if matches!(&f.supplies, super::SupplySelection::Explicit { items } if items.len() != 1 || items[0].cost_ordinal != 0)
    {
        return Err(serde::de::Error::custom("invalid stored ration selection"));
    }
    Ok((f.basis, f.replacement, f.supplies))
}

#[cfg(test)]
mod tests {
    use super::super::Action;
    // Future raw save-codec hardening tests. Install after host3 positive controls.
    #[test]
    fn advancement_boundary_saved_actions_require_complete_named_operands_at_byte_decode() {
        use serde_json::json;
        let basis =
            json!({"actor":1,"branch":"branch","stream":"stream","reset":0,"build_epoch":1});
        let purchase = json!({"owner":0,"pool":"advancement","selection":{"kind":"skill_rank","skill":"perception"}});
        let replacement = json!({"source":{"owner":0,"index":0},"replacement":null});
        let cases = [
            json!({"type":"purchase","basis":basis,"purchase":purchase}),
            json!({"type":"respec","basis":basis,"replacement":replacement,"supplies":{"kind":"carried"}}),
        ];
        for control in cases {
            let bytes = serde_json::to_vec(&control).unwrap();
            let actual: Action = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(serde_json::to_value(actual).unwrap(), control);
            for field in control
                .as_object()
                .unwrap()
                .keys()
                .filter(|field| *field != "type")
            {
                let mut missing = control.clone();
                missing.as_object_mut().unwrap().remove(field);
                assert!(
                    serde_json::from_slice::<Action>(&serde_json::to_vec(&missing).unwrap())
                        .is_err(),
                    "missing {field}"
                );
            }
            let mut unknown = control.clone();
            unknown["extra"] = json!(false);
            assert!(
                serde_json::from_slice::<Action>(&serde_json::to_vec(&unknown).unwrap()).is_err()
            );
            let mut basis_array = control.clone();
            basis_array["basis"] = json!([1, "branch", "stream", 0, 1]);
            assert!(
                serde_json::from_slice::<Action>(&serde_json::to_vec(&basis_array).unwrap())
                    .is_err()
            );
            let mut legacy_epoch = control.clone();
            legacy_epoch["basis"]
                .as_object_mut()
                .unwrap()
                .remove("build_epoch");
            assert!(
                serde_json::from_slice::<Action>(&serde_json::to_vec(&legacy_epoch).unwrap())
                    .is_err()
            );
            if control["type"] == "respec" {
                let mut carried_array = control.clone();
                carried_array["supplies"] = json!(["carried"]);
                assert!(
                    serde_json::from_slice::<Action>(&serde_json::to_vec(&carried_array).unwrap())
                        .is_err(),
                    "named ration selection"
                );
            }
        }
    }
}
