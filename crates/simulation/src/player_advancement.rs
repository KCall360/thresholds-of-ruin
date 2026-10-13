//! Ordinary advancement operands. The host authenticates branch/stream provenance;
//! simulation independently rechecks actor, permanent-build identity and rules.
use crate::attributes::Skill;
use crate::content_ids::{PointPoolId, TalentId};
use crate::creatures::{AdvancementPreview, BuildEpoch, BuildError, PurchaseSelection};
use crate::{ActorId, Game, GameError};
use serde::{Deserialize, Serialize};

/// Frozen preview identity, not an authorization capability. Accepted work keeps
/// its original host provenance through reconnects and retained-ancestor rewind.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct AdvancementBasis {
    pub actor: ActorId,
    pub branch: String,
    pub stream: String,
    pub reset: u64,
    pub build_epoch: BuildEpoch,
}
impl AdvancementBasis {
    pub fn valid(&self) -> bool {
        let label = |value: &str| {
            !value.trim().is_empty() && value.len() <= 128 && !value.chars().any(char::is_control)
        };
        self.actor.0 > 0 && self.actor.0 < u64::MAX && label(&self.branch) && label(&self.stream)
    }
}
impl<'de> Deserialize<'de> for AdvancementBasis {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            actor: ActorId,
            branch: String,
            stream: String,
            reset: u64,
            build_epoch: BuildEpoch,
        }
        let fields: Fields = crate::bounded::named_map(deserializer)?;
        let value = Self {
            actor: fields.actor,
            branch: fields.branch,
            stream: fields.stream,
            reset: fields.reset,
            build_epoch: fields.build_epoch,
        };
        if !value.valid() {
            return Err(serde::de::Error::custom("invalid advancement basis"));
        }
        Ok(value)
    }
}

/// One exact player choice. No candidate build, computed price or refund is an operand.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PurchaseChoice {
    SkillRank { skill: Skill },
    Talent { talent: TalentId },
}
impl<'de> Deserialize<'de> for PurchaseChoice {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            SkillRank { skill: Skill },
            Talent { talent: TalentId },
        }
        Ok(match crate::bounded::named_map(deserializer)? {
            Fields::SkillRank { skill } => Self::SkillRank { skill },
            Fields::Talent { talent } => Self::Talent { talent },
        })
    }
}
impl PurchaseChoice {
    pub fn selection(&self) -> PurchaseSelection {
        match self {
            Self::SkillRank { skill } => PurchaseSelection::SkillRank(*skill),
            Self::Talent { talent } => PurchaseSelection::Talent(talent.clone()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PurchaseOperation {
    pub owner: u16,
    pub pool: PointPoolId,
    pub selection: PurchaseChoice,
}
impl PurchaseOperation {
    pub(crate) fn structural_valid(&self, rules: &crate::rules::Ruleset) -> bool {
        usize::from(self.owner) < crate::limits::HIT_DICE
            && rules
                .point_pool(&self.pool)
                .is_some_and(|pool| pool.allowed.contains(&self.selection.selection().kind()))
            && match &self.selection {
                PurchaseChoice::SkillRank { .. } => true,
                PurchaseChoice::Talent { talent } => rules.talent(talent).is_some(),
            }
    }
}
impl<'de> Deserialize<'de> for PurchaseOperation {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            owner: u16,
            pool: PointPoolId,
            selection: PurchaseChoice,
        }
        let fields: Fields = crate::bounded::named_map(deserializer)?;
        if usize::from(fields.owner) >= crate::limits::HIT_DICE {
            return Err(serde::de::Error::custom("invalid purchase owner"));
        }
        Ok(Self {
            owner: fields.owner,
            pool: fields.pool,
            selection: fields.selection,
        })
    }
}

/// Read-only consequences of rebuilding the current projected runtime state.
/// This is descriptive output, never a saved candidate or commit operand.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdvancementConsequences {
    pub health: crate::health::Health,
    pub health_recovery: crate::health_recovery::HealthRecovery,
    pub resources: crate::resources::ByResource<AdvancementResource>,
    pub build_epoch: BuildEpoch,
    /// Aggregate removals by public condition identity, without disclosing causers.
    pub removed_conditions: Vec<RemovedCondition>,
    pub canceled_resource_holds: usize,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AdvancementResource {
    pub balance: u32,
    pub maximum: u32,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemovedCondition {
    pub condition: crate::content_ids::ConditionId,
    pub applications: u16,
    pub stacks: u32,
}
impl AdvancementConsequences {
    pub(crate) fn between(
        before: &crate::creatures::CreatureState,
        after: &crate::creatures::CreatureState,
    ) -> Self {
        let retained: std::collections::BTreeSet<_> = after
            .conditions()
            .instances()
            .map(|condition| (&condition.condition, condition.source, condition.order))
            .collect();
        let mut removed = std::collections::BTreeMap::<_, RemovedCondition>::new();
        for condition in before.conditions().instances() {
            if !retained.contains(&(&condition.condition, condition.source, condition.order)) {
                let row = removed
                    .entry(condition.condition.clone())
                    .or_insert_with(|| RemovedCondition {
                        condition: condition.condition.clone(),
                        applications: 0,
                        stacks: 0,
                    });
                row.applications += 1;
                row.stacks += u32::from(condition.stacks);
            }
        }
        let resources = after.costs().resources();
        Self {
            health: after.health(),
            health_recovery: after.health_recovery(),
            resources: crate::resources::ByResource::from_fn(|resource| AdvancementResource {
                balance: resources.balance(resource),
                maximum: resources.maximum(resource),
            }),
            build_epoch: after.build_epoch(),
            removed_conditions: removed.into_values().collect(),
            canceled_resource_holds: before
                .costs()
                .reservations()
                .keys()
                .filter(|owner| !after.costs().reservations().contains_key(owner))
                .count(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayerPurchasePreview {
    permanent: AdvancementPreview,
    runtime: AdvancementConsequences,
}
impl PlayerPurchasePreview {
    pub fn permanent(&self) -> &AdvancementPreview {
        &self.permanent
    }
    pub fn runtime(&self) -> &AdvancementConsequences {
        &self.runtime
    }
    pub fn build(&self) -> &crate::creatures::CreatureBuild {
        self.permanent.build()
    }
    /// Permanent derivation only. Effective Health/resources belong to `runtime`.
    pub fn derived(&self) -> &crate::creatures::DerivedCreature {
        self.permanent.derived()
    }
}

impl Game {
    /// Read-only ordinary readiness. The host additionally checks current control,
    /// run/queue permissions and exact admission provenance.
    pub fn player_advancement_ready(&self, actor: ActorId) -> bool {
        self.actors.get(&actor).is_some_and(|state| {
            state.alive()
                && state.pending.is_none()
                && state.ready_at <= self.tick
                && !self.actor_frozen(actor)
        })
    }
    /// Perceived danger only; never inspects another actor's private work.
    /// The caller must enforce the owner's disclosure authority.
    pub fn has_perceived_hostile(&self, actor: ActorId) -> bool {
        !self.visible_hostiles(actor).is_empty()
    }
    /// Pure backend preview, without ownership/transport authority. The host must
    /// disclose this only to the authorized owner and bind the original context.
    /// Temporary runtime state is projected solely to validate rebuild consequences.
    pub fn preview_player_purchase(
        &self,
        actor: ActorId,
        basis: &AdvancementBasis,
        purchase: &PurchaseOperation,
    ) -> Result<PlayerPurchasePreview, GameError> {
        self.prepare_player_purchase(actor, basis, purchase)
            .map(|(preview, _)| preview)
    }

    pub(crate) fn prepare_player_purchase(
        &self,
        actor: ActorId,
        basis: &AdvancementBasis,
        purchase: &PurchaseOperation,
    ) -> Result<
        (
            PlayerPurchasePreview,
            crate::lifecycle::PreparedActorOutcome,
        ),
        GameError,
    > {
        let creature = self.creature(actor).ok_or(GameError::UnknownActor)?;
        if !basis.valid() || basis.actor != actor || basis.build_epoch != creature.build_epoch() {
            return Err(GameError::Build(BuildError::StaleAdvancementPreview));
        }
        if creature.health().dead() {
            return Err(GameError::UnknownActor);
        }
        let preview = creature
            .build()
            .preview_purchase(
                usize::from(purchase.owner),
                purchase.pool.clone(),
                purchase.selection.selection(),
            )
            .map_err(GameError::Build)?;
        let (candidate, _) = self.prepare_creature_rebuild(actor, preview.build().clone())?;
        let candidate = self.prepare_actor_outcome(candidate)?;
        let after = candidate.creature().ok_or(GameError::UnknownActor)?;
        let runtime = AdvancementConsequences::between(&creature, after);
        Ok((
            PlayerPurchasePreview {
                permanent: preview,
                runtime,
            },
            candidate,
        ))
    }
}
