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
    TalentForm { talent: String, form: String },
}
impl<'de> Deserialize<'de> for Choice {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            SkillRank { skill: String },
            Talent { talent: String },
            TalentForm { talent: String, form: String },
        }
        Ok(match named_map(d)? {
            Fields::SkillRank { skill } => Self::SkillRank { skill },
            Fields::Talent { talent } => Self::Talent { talent },
            Fields::TalentForm { talent, form } => Self::TalentForm { talent, form },
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
            a::PurchaseChoice::TalentForm { talent, form } => Self::TalentForm {
                talent: talent.to_string(),
                form: form.to_string(),
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
            Self::TalentForm { talent, form } => a::PurchaseChoice::TalentForm {
                talent: s::content_ids::TalentId::new(talent)
                    .map_err(|_| E::custom("invalid stored talent"))?,
                form: s::content_ids::FormId::new(form)
                    .map_err(|_| E::custom("invalid stored form"))?,
            },
        })
    }
}
#[derive(Serialize)]
struct Purchase {
    owner: s::HdIndex,
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
            owner: s::HdIndex,
            pool: String,
            selection: Choice,
        }
        let f: Fields = named_map(d)?;
        if f.owner.ordinal().is_err() {
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
    owner: s::HdIndex,
    index: u16,
}
impl<'de> Deserialize<'de> for Source {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            owner: s::HdIndex,
            index: u16,
        }
        let f: Fields = named_map(d)?;
        if f.owner.ordinal().is_err() || usize::from(f.index) >= s::limits::POINTS_PER_HIT_DIE {
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
    #[test]
    fn stored_breath_purchase_and_replacement_preserve_the_exact_skill() {
        use serde_json::json;
        let selection = json!({"kind":"skill_rank","skill":"breath"});
        let basis =
            json!({"actor":1,"branch":"branch","stream":"stream","reset":0,"build_epoch":1});
        for control in [
            json!({"type":"purchase","basis":basis,"purchase":{"owner":7,"pool":"advancement","selection":selection}}),
            json!({"type":"respec","basis":basis,"replacement":{"source":{"owner":7,"index":0},"replacement":{"pool":"advancement","selection":selection}},"supplies":{"kind":"carried"}}),
        ] {
            let decoded: Action =
                serde_json::from_slice(&serde_json::to_vec(&control).unwrap()).unwrap();
            assert_eq!(serde_json::to_value(decoded).unwrap(), control);
            let path = if control["type"] == "purchase" {
                "/purchase/selection/skill"
            } else {
                "/replacement/replacement/selection/skill"
            };
            for unknown in ["Breath", "breath_weapon", "breath ", "missing"] {
                let mut bad = control.clone();
                *bad.pointer_mut(path).unwrap() = json!(unknown);
                assert!(
                    serde_json::from_slice::<Action>(&serde_json::to_vec(&bad).unwrap()).is_err()
                );
            }
        }
    }

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

#[cfg(test)]
mod form_tests {
    use super::super::Action;
    use serde_json::json;
    #[test]
    fn stored_form_actions_keep_exact_operands_and_require_named_fields() {
        let selection = json!({"kind":"talent_form","talent":"choice_root","form":"choice.form_0"});
        for control in [
            json!({"type":"purchase","basis":{"actor":1,"branch":"branch","stream":"stream","reset":0,"build_epoch":1},"purchase":{"owner":0,"pool":"advancement","selection":selection}}),
            json!({"type":"respec","basis":{"actor":1,"branch":"branch","stream":"stream","reset":0,"build_epoch":1},"replacement":{"source":{"owner":0,"index":0},"replacement":{"pool":"advancement","selection":selection}},"supplies":{"kind":"carried"}}),
        ] {
            let decoded: Action =
                serde_json::from_slice(&serde_json::to_vec(&control).unwrap()).unwrap();
            assert_eq!(serde_json::to_value(decoded).unwrap(), control);
            let path = if control["type"] == "purchase" {
                "/purchase/selection"
            } else {
                "/replacement/replacement/selection"
            };
            for bad in [
                json!({"kind":"talent_form","talent":"choice_root"}),
                json!({"kind":"talent_form","talent":"choice_root","form":null}),
                json!(["talent_form", "choice_root", "choice.form_0"]),
                json!({"kind":"talent_form","talent":"choice_root","form":"choice.form_0","extra":true}),
            ] {
                let mut value = control.clone();
                *value.pointer_mut(path).unwrap() = bad;
                assert!(
                    serde_json::from_slice::<Action>(&serde_json::to_vec(&value).unwrap()).is_err()
                );
            }
            let bytes = control.to_string().replace(
                "\"form\":\"choice.form_0\"",
                "\"form\":\"choice.form_0\",\"form\":\"choice.form_1\"",
            );
            assert!(serde_json::from_str::<Action>(&bytes).is_err());
        }
    }
}

/// Saved future-source operands use their own tagged schema and numeric basis.
pub(super) mod growth_source {
    use super::*;
    #[derive(Serialize, Deserialize)]
    #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
    enum Source {
        Racial {},
        Class { class: String },
    }
    pub(crate) fn serialize<S: serde::Serializer>(
        value: &s::earned_growth::GrowthSource,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        match value {
            s::earned_growth::GrowthSource::Racial {} => Source::Racial {},
            s::earned_growth::GrowthSource::Class { class } => Source::Class {
                class: class.to_string(),
            },
        }
        .serialize(serializer)
    }
    pub(crate) fn deserialize<'de, D: serde::Deserializer<'de>>(
        d: D,
    ) -> Result<s::earned_growth::GrowthSource, D::Error> {
        Ok(match named_map(d)? {
            Source::Racial {} => s::earned_growth::GrowthSource::Racial {},
            Source::Class { class } => s::earned_growth::GrowthSource::Class {
                class: s::content_ids::ClassId::new(class)
                    .map_err(|_| serde::de::Error::custom("invalid growth class identifier"))?,
            },
        })
    }
}
pub(super) fn read_growth_source<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<(Box<a::AdvancementBasis>, s::earned_growth::GrowthSource), D::Error> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Fields {
        #[serde(with = "basis")]
        basis: Box<a::AdvancementBasis>,
        #[serde(with = "growth_source")]
        source: s::earned_growth::GrowthSource,
    }
    let f: Fields = named_map(d)?;
    Ok((f.basis, f.source))
}

#[cfg(test)]
mod growth_tests {
    use super::super::Action;
    use serde_json::json;
    #[test]
    fn stored_source_selection_rejects_missing_unknown_duplicate_and_client_xp_fields() {
        let good = json!({"type":"choose_growth_source","basis":{"actor":1,"branch":"main","stream":"owner","reset":0,"build_epoch":1},"source":{"kind":"class","class":"warrior"}});
        let _: Action = serde_json::from_value(good.clone()).unwrap();
        for source in [
            json!({"kind":"class"}),
            json!({"kind":"class","class":"INVALID"}),
            json!({"kind":"racial","seed":1}),
            json!(["class", "warrior"]),
        ] {
            let mut bad = good.clone();
            bad["source"] = source;
            assert!(serde_json::from_value::<Action>(bad).is_err());
        }
        for field in ["xp", "seed", "threat", "candidate"] {
            let mut bad = good.clone();
            bad[field] = json!(1);
            assert!(serde_json::from_value::<Action>(bad).is_err());
        }
        let duplicate = good.to_string().replace(
            "\"class\":\"warrior\"",
            "\"class\":\"warrior\",\"class\":\"mage\"",
        );
        assert!(serde_json::from_str::<Action>(&duplicate).is_err());
    }
}
