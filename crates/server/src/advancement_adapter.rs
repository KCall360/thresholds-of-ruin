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
        p::PurchaseChoice::Talent { talent } => a::PurchaseChoice::Talent {
            talent: s::content_ids::TalentId::new(talent).map_err(|_| invalid())?,
        },
    })
}
pub(crate) fn wire_choice(value: &a::PurchaseChoice) -> p::PurchaseChoice {
    match value {
        a::PurchaseChoice::SkillRank { skill } => p::PurchaseChoice::SkillRank {
            skill: skill.as_str().into(),
        },
        a::PurchaseChoice::Talent { talent } => p::PurchaseChoice::Talent {
            talent: talent.to_string(),
        },
    }
}
pub(crate) fn purchase(
    value: &p::PurchaseOperation,
) -> Result<a::PurchaseOperation, crate::Failure> {
    if usize::from(value.owner) >= s::limits::HIT_DICE {
        return Err(invalid());
    }
    Ok(a::PurchaseOperation {
        owner: value.owner,
        pool: s::content_ids::PointPoolId::new(&value.pool).map_err(|_| invalid())?,
        selection: choice(&value.selection)?,
    })
}
pub(crate) fn wire_purchase(value: &a::PurchaseOperation) -> p::PurchaseOperation {
    p::PurchaseOperation {
        owner: value.owner,
        pool: value.pool.to_string(),
        selection: wire_choice(&value.selection),
    }
}
pub(crate) fn replacement(
    value: &p::ReplacementOperation,
) -> Result<r::ReplacementOperation, crate::Failure> {
    if usize::from(value.source.owner) >= s::limits::HIT_DICE
        || usize::from(value.source.index) >= s::limits::POINTS_PER_HIT_DIE
    {
        return Err(invalid());
    }
    Ok(r::ReplacementOperation {
        source: r::ReplacementSource {
            owner: value.source.owner,
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
            owner: value.source.owner,
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
