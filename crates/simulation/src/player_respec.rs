//! Exact timed replacements share permanent ownership and finite item authority.
use crate::content_ids::PointPoolId;
use crate::creatures::{
    AdvancementPurchase, AdvancementReplacementPreview, BuildError, PurchaseLocation,
};
use crate::player_advancement::{AdvancementBasis, AdvancementConsequences, PurchaseChoice};
use crate::{ActorId, Game, GameError};
use serde::{Deserialize, Serialize};

/// Location in the pinned source build; never an index into a refreshed proposal.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct ReplacementSource {
    pub owner: crate::HdIndex,
    pub index: u16,
}
impl ReplacementSource {
    pub fn location(self) -> PurchaseLocation {
        PurchaseLocation {
            owner: self.owner,
            index: usize::from(self.index),
        }
    }
    fn valid(self) -> bool {
        self.owner.ordinal().is_ok() && usize::from(self.index) < crate::limits::POINTS_PER_HIT_DIE
    }
}
impl<'de> Deserialize<'de> for ReplacementSource {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            owner: crate::HdIndex,
            index: u16,
        }
        let Fields { owner, index } = crate::bounded::named_map(d)?;
        let value = Self { owner, index };
        if !value.valid() {
            return Err(serde::de::Error::custom("invalid replacement source"));
        }
        Ok(value)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ReplacementPurchase {
    pub pool: PointPoolId,
    pub selection: PurchaseChoice,
}
impl<'de> Deserialize<'de> for ReplacementPurchase {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            pool: PointPoolId,
            selection: PurchaseChoice,
        }
        let Fields { pool, selection } = crate::bounded::named_map(d)?;
        Ok(Self { pool, selection })
    }
}
impl ReplacementPurchase {
    fn purchase(&self) -> AdvancementPurchase {
        AdvancementPurchase {
            id: None,
            pool: self.pool.clone(),
            selection: self.selection.selection(),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ReplacementOperation {
    pub source: ReplacementSource,
    pub replacement: Option<ReplacementPurchase>,
}
impl<'de> Deserialize<'de> for ReplacementOperation {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            source: ReplacementSource,
            #[serde(deserialize_with = "crate::bounded::required_option")]
            replacement: Option<ReplacementPurchase>,
        }
        let Fields {
            source,
            replacement,
        } = crate::bounded::named_map(d)?;
        Ok(Self {
            source,
            replacement,
        })
    }
}
impl ReplacementOperation {
    pub(crate) fn structural_valid(&self, rules: &crate::rules::Ruleset) -> bool {
        self.source.valid()
            && self.replacement.as_ref().is_none_or(|replacement| {
                crate::player_advancement::PurchaseOperation {
                    owner: self.source.owner,
                    pool: replacement.pool.clone(),
                    selection: replacement.selection.clone(),
                }
                .structural_valid(rules)
            })
    }
    fn preview(
        &self,
        build: &crate::creatures::CreatureBuild,
    ) -> Result<AdvancementReplacementPreview, BuildError> {
        build.preview_replacement(
            self.source.location(),
            self.replacement.as_ref().map(ReplacementPurchase::purchase),
        )
    }
}

/// Fixed safety semantics with the authored actor-active interval pinned to content.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RespecSafety {
    pub received_loss_interval: u64,
}
impl<'de> Deserialize<'de> for RespecSafety {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            received_loss_interval: u64,
        }
        let Fields {
            received_loss_interval,
        } = crate::bounded::named_map(d)?;
        Ok(Self {
            received_loss_interval,
        })
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RespecPlan {
    pub basis: AdvancementBasis,
    pub replacement: ReplacementOperation,
    /// Frozen timing witness; validation never consults the actor's live rate.
    pub action_duration: crate::action_duration::ActionDuration,
    pub preparation: u64,
    pub recovery: u64,
    pub safety: RespecSafety,
    pub ration: crate::rules::SupplyCost,
    pub supplies: crate::supplies::SupplySelection,
}
impl<'de> Deserialize<'de> for RespecPlan {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            basis: AdvancementBasis,
            replacement: ReplacementOperation,
            action_duration: crate::action_duration::ActionDuration,
            preparation: u64,
            recovery: u64,
            safety: RespecSafety,
            ration: crate::rules::SupplyCost,
            supplies: crate::supplies::SupplySelection,
        }
        let f: Fields = crate::bounded::named_map(d)?;
        Ok(Self {
            basis: f.basis,
            replacement: f.replacement,
            action_duration: f.action_duration,
            preparation: f.preparation,
            recovery: f.recovery,
            safety: f.safety,
            ration: f.ration,
            supplies: f.supplies,
        })
    }
}
impl RespecPlan {
    pub(crate) fn matches_build(&self, creature: &crate::creatures::CreatureState) -> bool {
        self.basis.build_epoch == creature.build_epoch()
            && self.replacement.preview(creature.build()).is_ok()
    }
    pub fn valid(&self, rules: &crate::rules::Ruleset) -> bool {
        let policy = &rules.document().advancement;
        self.basis.valid()
            && self.replacement.structural_valid(rules)
            && self.supplies.valid(1)
            && self.action_duration.apply_bounded(policy.respec_prepare) == Ok(self.preparation)
            && self.action_duration.apply_bounded(policy.respec_recovery) == Ok(self.recovery)
            && self.safety.received_loss_interval == policy.received_loss_interval
            && self.ration == policy.respec_ration
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayerReplacementPreview {
    permanent: AdvancementReplacementPreview,
    runtime: AdvancementConsequences,
}
impl PlayerReplacementPreview {
    pub fn permanent(&self) -> &AdvancementReplacementPreview {
        &self.permanent
    }
    pub fn runtime(&self) -> &AdvancementConsequences {
        &self.runtime
    }
}
impl Game {
    pub fn preview_player_replacement(
        &self,
        actor: ActorId,
        basis: &AdvancementBasis,
        replacement: &ReplacementOperation,
    ) -> Result<PlayerReplacementPreview, GameError> {
        let before = self.creature(actor).ok_or(GameError::UnknownActor)?;
        let (permanent, candidate) = self.prepare_player_replacement(actor, basis, replacement)?;
        let finalized = self.prepare_actor_outcome(candidate)?;
        let runtime = AdvancementConsequences::between(
            &before,
            finalized.creature().ok_or(GameError::UnknownActor)?,
        );
        Ok(PlayerReplacementPreview { permanent, runtime })
    }
    fn prepare_player_replacement(
        &self,
        actor: ActorId,
        basis: &AdvancementBasis,
        replacement: &ReplacementOperation,
    ) -> Result<
        (
            AdvancementReplacementPreview,
            crate::lifecycle::ActorOutcome,
        ),
        GameError,
    > {
        self.require_owner_advancement()?;
        let creature = self.creature(actor).ok_or(GameError::UnknownActor)?;
        if !basis.valid() || basis.actor != actor || basis.build_epoch != creature.build_epoch() {
            return Err(GameError::Build(BuildError::StaleAdvancementPreview));
        }
        if creature.health().dead() {
            return Err(GameError::UnknownActor);
        }
        let preview = replacement
            .preview(creature.build())
            .map_err(GameError::Build)?;
        let mut build = creature.build().clone();
        build
            .commit_replacement(preview.clone())
            .map_err(GameError::Build)?;
        let (outcome, _) = self.prepare_creature_rebuild(actor, build)?;
        Ok((preview, outcome))
    }
    pub(crate) fn respec_environment_safe(&self, actor: ActorId) -> bool {
        self.creature(actor).is_some_and(|creature| {
            !creature.health().dead() && creature.received_health_loss_remaining() == 0
        }) && self.visible_hostiles(actor).is_empty()
    }
    pub(crate) fn respec_plan(
        &self,
        actor: ActorId,
        basis: AdvancementBasis,
        replacement: ReplacementOperation,
        supplies: crate::supplies::SupplySelection,
    ) -> Result<crate::abilities::Plan, GameError> {
        if !self.respec_environment_safe(actor) {
            return Err(GameError::ActorBusy);
        }
        self.preview_player_replacement(actor, &basis, &replacement)?;
        let policy = &self.content.rules.document().advancement;
        let action_duration = self
            .creature(actor)
            .ok_or(GameError::UnknownActor)?
            .derived()
            .action_duration;
        let plan = crate::abilities::Plan::Respec(tor_world::Shared::new(RespecPlan {
            basis,
            replacement,
            action_duration,
            preparation: action_duration
                .apply_bounded(policy.respec_prepare)
                .map_err(GameError::Build)?,
            recovery: action_duration
                .apply_bounded(policy.respec_recovery)
                .map_err(GameError::Build)?,
            safety: RespecSafety {
                received_loss_interval: policy.received_loss_interval,
            },
            ration: policy.respec_ration.clone(),
            supplies,
        }));
        if !plan.valid(&self.content.rules) {
            return Err(GameError::Rule(crate::abilities::RuleError::InvalidPlan));
        }
        self.item_allocation(&plan)?;
        Ok(plan)
    }
    pub(crate) fn complete_respec(
        &mut self,
        mut pending: crate::combat::Preparation,
        due: u64,
    ) -> Result<(), GameError> {
        let crate::abilities::Plan::Respec(plan) = &pending.plan else {
            return Err(GameError::Rule(crate::abilities::RuleError::InvalidPlan));
        };
        let actor = plan.basis.actor;
        // Continuous eligibility has already discarded rule-defined failures.
        // Everything below is fallible within the caller's rollback transaction.
        let (_, mut outcome) =
            self.prepare_player_replacement(actor, &plan.basis, &plan.replacement)?;
        let ready_at = due
            .checked_add(plan.recovery)
            .ok_or(GameError::Schedule(crate::ScheduleError::TimeExhausted))?;
        let completion = crate::combat::CombatEvent::RespecCompleted {
            actor,
            basis: plan.basis.clone(),
            replacement: plan.replacement.clone(),
            intention: pending.intention,
        };
        outcome
            .state
            .complete_preparation()?
            .ok_or(GameError::Rule(crate::abilities::RuleError::InvalidPlan))?;
        outcome.state.ready_at = ready_at;
        outcome.spent_items = pending.item_lineage();
        self.spend_item_payment(&mut pending)?;
        // Must finalize after Spend: death must not retain an already deleted ration.
        let finalized = self.prepare_actor_outcome(outcome)?;
        self.publish_actor_outcome(finalized)?;
        self.push_event(completion, None)?;
        Ok(())
    }
}
