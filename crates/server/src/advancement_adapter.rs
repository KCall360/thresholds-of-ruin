//! Exact transport/domain operands; no candidate build or receipt authority.
use tor_protocol as p;
use tor_simulation::{self as s, player_advancement as a, player_respec as r};

fn invalid() -> crate::Failure {
    crate::Failure::new(p::ErrorCode::InvalidAction, "Invalid advancement operation")
}
pub(crate) fn basis(value: &p::AdvancementBasis) -> Result<a::AdvancementBasis, crate::Failure> {
    let value = a::AdvancementBasis {
        actor: s::ActorId(value.actor.0),
        branch: value.branch.0.clone(),
        stream: value.stream.0.clone(),
        reset: value.reset,
        build_epoch: s::creatures::BuildEpoch::new(value.build_epoch).ok_or_else(invalid)?,
    };
    if !value.valid() {
        return Err(invalid());
    }
    Ok(value)
}
pub(crate) fn wire_basis(value: &a::AdvancementBasis) -> p::AdvancementBasis {
    p::AdvancementBasis {
        actor: p::ActorId(value.actor.0),
        branch: p::BranchId(value.branch.clone()),
        stream: p::StreamId(value.stream.clone()),
        reset: value.reset,
        build_epoch: value.build_epoch.get(),
    }
}
pub(crate) fn choice(value: &p::PurchaseChoice) -> Result<a::PurchaseChoice, crate::Failure> {
    Ok(match value {
        p::PurchaseChoice::SkillRank { skill } => a::PurchaseChoice::SkillRank {
            skill: s::attributes::Skill::ALL
                .into_iter()
                .find(|candidate| candidate.as_str() == skill)
                .ok_or_else(invalid)?,
        },
        p::PurchaseChoice::Expertise { expertise } => a::PurchaseChoice::Expertise {
            expertise: s::content_ids::ExpertiseId::new(expertise).map_err(|_| invalid())?,
        },
        p::PurchaseChoice::Talent { talent } => a::PurchaseChoice::Talent {
            talent: s::content_ids::TalentId::new(talent).map_err(|_| invalid())?,
        },
        p::PurchaseChoice::TalentForm { talent, form } => a::PurchaseChoice::TalentForm {
            talent: s::content_ids::TalentId::new(talent).map_err(|_| invalid())?,
            form: s::content_ids::FormId::new(form).map_err(|_| invalid())?,
        },
    })
}
pub(crate) fn wire_choice(value: &a::PurchaseChoice) -> p::PurchaseChoice {
    match value {
        a::PurchaseChoice::SkillRank { skill } => p::PurchaseChoice::SkillRank {
            skill: skill.as_str().into(),
        },
        a::PurchaseChoice::Expertise { expertise } => p::PurchaseChoice::Expertise {
            expertise: expertise.to_string(),
        },
        a::PurchaseChoice::Talent { talent } => p::PurchaseChoice::Talent {
            talent: talent.to_string(),
        },
        a::PurchaseChoice::TalentForm { talent, form } => p::PurchaseChoice::TalentForm {
            talent: talent.to_string(),
            form: form.to_string(),
        },
    }
}
pub(crate) fn purchase(
    value: &p::PurchaseOperation,
) -> Result<a::PurchaseOperation, crate::Failure> {
    if value.owner == u64::MAX {
        return Err(invalid());
    }
    Ok(a::PurchaseOperation {
        owner: s::HdIndex(value.owner),
        pool: s::content_ids::PointPoolId::new(&value.pool).map_err(|_| invalid())?,
        selection: choice(&value.selection)?,
    })
}
pub(crate) fn wire_purchase(value: &a::PurchaseOperation) -> p::PurchaseOperation {
    p::PurchaseOperation {
        owner: value.owner.get(),
        pool: value.pool.to_string(),
        selection: wire_choice(&value.selection),
    }
}
pub(crate) fn replacement(
    value: &p::ReplacementOperation,
) -> Result<r::ReplacementOperation, crate::Failure> {
    if value.source.owner == u64::MAX
        || usize::from(value.source.index) >= s::limits::POINTS_PER_HIT_DIE
    {
        return Err(invalid());
    }
    Ok(r::ReplacementOperation {
        source: r::ReplacementSource {
            owner: s::HdIndex(value.source.owner),
            index: value.source.index,
        },
        replacement: value
            .replacement
            .as_ref()
            .map(|row| {
                Ok::<_, crate::Failure>(r::ReplacementPurchase {
                    pool: s::content_ids::PointPoolId::new(&row.pool).map_err(|_| invalid())?,
                    selection: choice(&row.selection)?,
                })
            })
            .transpose()?,
    })
}
pub(crate) fn wire_replacement(value: &r::ReplacementOperation) -> p::ReplacementOperation {
    p::ReplacementOperation {
        source: p::ReplacementSource {
            owner: value.source.owner.get(),
            index: value.source.index,
        },
        replacement: value
            .replacement
            .as_ref()
            .map(|row| p::ReplacementPurchase {
                pool: row.pool.to_string(),
                selection: wire_choice(&row.selection),
            }),
    }
}

pub(crate) fn growth_source(
    value: &p::GrowthSource,
) -> Result<s::earned_growth::GrowthSource, crate::Failure> {
    Ok(match value {
        p::GrowthSource::Racial {} => s::earned_growth::GrowthSource::Racial {},
        p::GrowthSource::Class { class } => s::earned_growth::GrowthSource::Class {
            class: s::content_ids::ClassId::new(class).map_err(|_| invalid())?,
        },
    })
}
pub(crate) fn wire_growth_source(value: &s::earned_growth::GrowthSource) -> p::GrowthSource {
    match value {
        s::earned_growth::GrowthSource::Racial {} => p::GrowthSource::Racial {},
        s::earned_growth::GrowthSource::Class { class } => p::GrowthSource::Class {
            class: class.to_string(),
        },
    }
}
pub(crate) fn wire_future_growth(value: &s::earned_growth::FutureGrowth) -> p::FutureGrowth {
    match value {
        s::earned_growth::FutureGrowth::Disabled {} => p::FutureGrowth::Disabled {},
        s::earned_growth::FutureGrowth::Selected { source } => p::FutureGrowth::Selected {
            source: wire_growth_source(source),
        },
        s::earned_growth::FutureGrowth::Authored {} => p::FutureGrowth::Authored {},
    }
}

pub(crate) fn intellect_expertise(
    value: &p::IntellectExpertiseOperation,
) -> Result<a::IntellectExpertiseOperation, crate::Failure> {
    if value.threshold == 0 || value.threshold > s::limits::DERIVED_ATTRIBUTE {
        return Err(invalid());
    }
    Ok(a::IntellectExpertiseOperation {
        threshold: value.threshold,
        expertise: s::content_ids::ExpertiseId::new(&value.expertise).map_err(|_| invalid())?,
    })
}
pub(crate) fn wire_intellect_expertise(
    value: &a::IntellectExpertiseOperation,
) -> p::IntellectExpertiseOperation {
    p::IntellectExpertiseOperation {
        threshold: value.threshold,
        expertise: value.expertise.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn breath_and_every_named_skill_roundtrip_without_accepting_aliases() {
        for skill in s::attributes::Skill::ALL {
            let domain = a::PurchaseChoice::SkillRank { skill };
            let wire = wire_choice(&domain);
            assert_eq!(
                wire,
                p::PurchaseChoice::SkillRank {
                    skill: skill.as_str().into()
                }
            );
            assert_eq!(choice(&wire).unwrap(), domain);
        }
        assert_eq!(
            choice(&p::PurchaseChoice::SkillRank {
                skill: "breath".into()
            })
            .unwrap(),
            a::PurchaseChoice::SkillRank {
                skill: s::attributes::Skill::Breath
            }
        );
        for unknown in ["Breath", "breath_weapon", "breath ", "missing"] {
            assert!(choice(&p::PurchaseChoice::SkillRank {
                skill: unknown.into()
            })
            .is_err());
        }
    }
}
