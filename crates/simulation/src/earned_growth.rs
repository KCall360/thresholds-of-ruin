//! Actor-owned earned XP and explicit future HD source policy.
//!
//! XP applies HD immediately when its declared source is available. A missing
//! source retains the XP balance; it never silently spends or discards it.
use crate::creatures::{BuildError, CreatureBuild, DerivedCreature, PurchaseSelection};
use crate::rules::{GrowthPolicy, ThreatSource};
use serde::{Deserialize, Serialize};

/// One bounded causal origin. Later condition/motion adapters must carry their
/// original owner and episode through the applied effect, not guess a last hit.
/// This transient type is not a wire capability or a saved death-ID ledger.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FatalOrigin {
    Direct {
        ability: Option<crate::content_ids::AbilityId>,
        intention: Option<crate::IntentionId>,
        strike: Option<u16>,
        effect: u16,
    },
    Condition {
        condition: crate::content_ids::ConditionId,
        application: u64,
    },
    Displacement {
        episode: std::num::NonZeroU64,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FatalCause {
    pub owner: crate::ActorId,
    pub origin: FatalOrigin,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GrowthError {
    InvalidRecord,
    Arithmetic,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GrowthSource {
    Racial {},
    Class { class: crate::content_ids::ClassId },
}
impl<'de> Deserialize<'de> for GrowthSource {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            Racial {},
            Class { class: crate::content_ids::ClassId },
        }
        Ok(match crate::bounded::named_map(d)? {
            Fields::Racial {} => Self::Racial {},
            Fields::Class { class } => Self::Class { class },
        })
    }
}
impl GrowthSource {
    pub fn hd_source(&self) -> crate::progression::HdSource {
        match self {
            Self::Racial {} => crate::progression::HdSource::Racial,
            Self::Class { class } => crate::progression::HdSource::Class(class.clone()),
        }
    }
    pub fn valid(&self, rules: &crate::rules::Ruleset) -> bool {
        match self {
            Self::Racial {} => true,
            Self::Class { class } => rules.class(class).is_some(),
        }
    }
}

/// Ordinary growth is explicit data. Control ownership never enables NPC growth.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FutureGrowth {
    Disabled {},
    Selected { source: GrowthSource },
    Authored {},
}
impl<'de> Deserialize<'de> for FutureGrowth {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            Disabled {},
            Selected { source: GrowthSource },
            Authored {},
        }
        Ok(match crate::bounded::named_map(d)? {
            Fields::Disabled {} => Self::Disabled {},
            Fields::Selected { source } => Self::Selected { source },
            Fields::Authored {} => Self::Authored {},
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct EarnedProgress {
    authored_baseline: u16,
    highest_applied_tier: u16,
    highest_earned_tier: u16,
    peak_threat: u16,
    earned_xp: u64,
    spent_xp: u64,
    held_xp: u64,
}
impl<'de> Deserialize<'de> for EarnedProgress {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            authored_baseline: u16,
            highest_applied_tier: u16,
            highest_earned_tier: u16,
            peak_threat: u16,
            earned_xp: u64,
            spent_xp: u64,
            held_xp: u64,
        }
        let f: Fields = crate::bounded::named_map(d)?;
        let value = Self {
            authored_baseline: f.authored_baseline,
            highest_applied_tier: f.highest_applied_tier,
            highest_earned_tier: f.highest_earned_tier,
            peak_threat: f.peak_threat,
            earned_xp: f.earned_xp,
            spent_xp: f.spent_xp,
            held_xp: f.held_xp,
        };
        if !value.shape_valid() {
            return Err(serde::de::Error::custom("invalid earned progression"));
        }
        Ok(value)
    }
}
impl EarnedProgress {
    pub(crate) fn new(hit_dice: u16, threat: u16) -> Self {
        Self {
            authored_baseline: hit_dice,
            highest_applied_tier: hit_dice,
            highest_earned_tier: hit_dice,
            peak_threat: threat,
            earned_xp: 0,
            spent_xp: 0,
            held_xp: 0,
        }
    }
    pub fn highest_earned_tier(&self) -> u16 {
        self.highest_earned_tier
    }
    pub fn peak_threat(&self) -> u16 {
        self.peak_threat
    }
    pub fn held_xp(&self) -> u64 {
        self.held_xp
    }
    pub fn earned_xp(&self) -> u64 {
        self.earned_xp
    }
    pub fn spent_xp(&self) -> u64 {
        self.spent_xp
    }
    pub fn next_threshold(&self, policy: &GrowthPolicy) -> u32 {
        policy.threshold(self.highest_applied_tier)
    }
    fn shape_valid(&self) -> bool {
        self.authored_baseline <= self.highest_applied_tier
            && self.highest_applied_tier <= self.highest_earned_tier
            && self.highest_earned_tier <= crate::limits::HIT_DICE as u16
            && (1..=crate::limits::HIT_DICE as u16).contains(&self.peak_threat)
            && self.spent_xp.checked_add(self.held_xp) == Some(self.earned_xp)
    }
    pub(crate) fn validate(&self, current: u16, policy: &GrowthPolicy) -> Result<(), GrowthError> {
        let minimum = |tier| {
            (self.authored_baseline..tier).try_fold(0_u64, |sum, prior| {
                sum.checked_add(u64::from(policy.threshold(prior)))
            })
        };
        if self.shape_valid()
            && current <= self.highest_applied_tier
            && minimum(self.highest_applied_tier).is_some_and(|minimum| self.spent_xp >= minimum)
            && minimum(self.highest_earned_tier).is_some_and(|minimum| self.earned_xp >= minimum)
            && self.projected_tier(current, policy) <= self.highest_earned_tier
        {
            Ok(())
        } else {
            Err(GrowthError::InvalidRecord)
        }
    }
    fn projected_tier(&self, current: u16, policy: &GrowthPolicy) -> u16 {
        let mut tier = self.highest_applied_tier;
        let mut ordinal = current;
        let mut xp = self.held_xp;
        while usize::from(ordinal) < crate::limits::HIT_DICE {
            let cost = u64::from(policy.threshold(tier));
            if xp < cost {
                break;
            }
            xp -= cost;
            ordinal += 1;
            tier = tier.max(ordinal);
        }
        tier
    }
    pub(crate) fn award(
        &mut self,
        amount: u32,
        current: u16,
        policy: &GrowthPolicy,
    ) -> Result<(), GrowthError> {
        let earned = self
            .earned_xp
            .checked_add(u64::from(amount))
            .ok_or(GrowthError::Arithmetic)?;
        let held = self
            .held_xp
            .checked_add(u64::from(amount))
            .ok_or(GrowthError::Arithmetic)?;
        self.earned_xp = earned;
        self.held_xp = held;
        self.highest_earned_tier = self
            .highest_earned_tier
            .max(self.projected_tier(current, policy));
        Ok(())
    }
    pub(crate) fn reconcile_build(
        &mut self,
        current: u16,
        threat: u16,
        policy: &GrowthPolicy,
    ) -> Result<(), GrowthError> {
        // General-purpose source/wizard additions establish an authored baseline;
        // ordinary growth records its paid tier before invoking this boundary.
        if current > self.highest_applied_tier {
            self.authored_baseline = self.authored_baseline.max(current);
            self.highest_applied_tier = current;
        }
        self.peak_threat = self.peak_threat.max(threat);
        self.highest_earned_tier = self
            .highest_earned_tier
            .max(self.projected_tier(current, policy));
        self.validate(current, policy)
    }
    pub(crate) fn spend(
        &mut self,
        next_ordinal: u16,
        policy: &GrowthPolicy,
    ) -> Result<(), GrowthError> {
        let price = u64::from(self.next_threshold(policy));
        let held = self
            .held_xp
            .checked_sub(price)
            .ok_or(GrowthError::InvalidRecord)?;
        let spent = self
            .spent_xp
            .checked_add(price)
            .ok_or(GrowthError::Arithmetic)?;
        self.held_xp = held;
        self.spent_xp = spent;
        self.highest_applied_tier = self.highest_applied_tier.max(next_ordinal);
        self.highest_earned_tier = self.highest_earned_tier.max(next_ordinal);
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThreatContribution {
    pub source: ThreatSource,
    pub points: u32,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThreatAssessment {
    pub sources: Vec<ThreatContribution>,
    pub hit_dice_points: u32,
    pub training_points: u32,
    pub total: u32,
    pub tier: u16,
}

/// Source-only assessment from the permanent fold. The caller must supply that
/// build's permanent derivation, never an equipment/condition-adjusted creature.
pub(crate) fn assess(
    build: &CreatureBuild,
    permanent: &DerivedCreature,
) -> Result<ThreatAssessment, BuildError> {
    let policy = &build.ruleset().document().growth.threat;
    let mut identities = std::collections::BTreeSet::new();
    identities.insert(ThreatSource::Species {
        id: build.species_id().clone(),
    });
    identities.insert(ThreatSource::Type {
        id: permanent.kind.clone(),
    });
    identities.extend(
        permanent
            .subtypes
            .iter()
            .cloned()
            .map(|id| ThreatSource::Subtype { id }),
    );
    identities.extend(build.templates().map(|template| ThreatSource::Template {
        id: template.id.clone(),
    }));
    for die in build.ledger().entries() {
        if let crate::progression::HdSource::Class(id) = die.source() {
            identities.insert(ThreatSource::Class { id });
        }
    }
    let mut trained = 2_u32;
    for choice in build.choices() {
        for purchase in &choice.purchases {
            match &purchase.selection {
                PurchaseSelection::SkillRank(_) => trained += 1,
                PurchaseSelection::Talent(id)
                | PurchaseSelection::TalentForm { talent: id, .. } => {
                    identities.insert(ThreatSource::Talent { id: id.clone() });
                }
            }
        }
    }
    let sources: Vec<_> = identities
        .into_iter()
        .map(|source| ThreatContribution {
            points: policy.contribution(&source),
            source,
        })
        .collect();
    let hit_dice_points = policy
        .hit_die
        .checked_mul(u32::from(build.ledger().total_hd()))
        .ok_or(BuildError::DerivedLimit)?;
    let training_points = policy
        .trained_rank
        .checked_mul(trained)
        .ok_or(BuildError::DerivedLimit)?;
    let total = sources.iter().try_fold(
        hit_dice_points
            .checked_add(training_points)
            .ok_or(BuildError::DerivedLimit)?,
        |sum, source| {
            sum.checked_add(source.points)
                .ok_or(BuildError::DerivedLimit)
        },
    )?;
    let tier = u64::from(total)
        .div_ceil(u64::from(policy.points_per_tier))
        .clamp(1, crate::limits::HIT_DICE as u64) as u16;
    Ok(ThreatAssessment {
        sources,
        hit_dice_points,
        training_points,
        total,
        tier,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::creatures::{CreatureState, CreatureStateError};
    use crate::progression::{HdLedger, HdSource};

    fn creature(hd: usize, enabled: bool) -> CreatureState {
        let original = crate::test_creatures::build(crate::test_creatures::species());
        let mut build = CreatureBuild::new(
            original.content().clone(),
            original.species_id().clone(),
            HdLedger::seeded(vec![HdSource::Racial; hd], 42).unwrap(),
            original.binding(),
            *original.starting_skills(),
        )
        .unwrap();
        if enabled {
            build
                .set_future_growth(FutureGrowth::Selected {
                    source: GrowthSource::Racial {},
                })
                .unwrap();
        }
        CreatureState::new(build).unwrap()
    }
    fn drain(state: &mut CreatureState) {
        let mut build = state.build().clone();
        build.remove_latest().unwrap();
        state.rebuild(build).unwrap();
    }
    fn restore(state: &mut CreatureState) {
        let mut build = state.build().clone();
        build.add_hit_die(HdSource::Racial, 42).unwrap();
        state.rebuild(build).unwrap();
    }
    #[test]
    fn sixty_percent_survives_drain_and_fresh_xp_regrows_six_instead_of_seven() {
        let mut state = creature(6, true);
        let retained = state.build().ledger().entries().to_vec();
        state.award_xp(360).unwrap();
        drain(&mut state);
        assert_eq!(state.earned_progress().held_xp(), 360);
        assert_eq!(state.earned_progress().highest_earned_tier(), 6);
        assert_eq!(state.build().ledger().total_hd(), 5);
        state.award_xp(240).unwrap();
        assert_eq!(state.build().ledger().total_hd(), 6);
        assert_eq!(state.build().ledger().entries(), retained);
        assert_eq!(state.earned_progress().held_xp(), 0);
        assert_eq!(state.earned_progress().spent_xp(), 600);
        assert_eq!(state.build().unspent_points(), 6);
    }
    #[test]
    fn explicit_restoration_keeps_sixty_percent_toward_the_next_new_hit_die() {
        let mut state = creature(6, true);
        state.award_xp(360).unwrap();
        drain(&mut state);
        restore(&mut state);
        assert_eq!(state.earned_progress().held_xp(), 360);
        state.award_xp(240).unwrap();
        assert_eq!(state.build().ledger().total_hd(), 7);
        assert_eq!(state.earned_progress().highest_earned_tier(), 7);
        assert_eq!(state.earned_progress().spent_xp(), 600);
    }
    #[test]
    fn another_drain_never_reuses_an_already_completed_threshold() {
        let mut state = creature(6, true);
        drain(&mut state);
        state.award_xp(600).unwrap();
        drain(&mut state);
        assert_eq!(state.build().ledger().total_hd(), 5);
        assert_eq!(state.earned_progress().held_xp(), 0);
        state.award_xp(599).unwrap();
        assert_eq!(state.build().ledger().total_hd(), 5);
        state.award_xp(1).unwrap();
        assert_eq!(state.build().ledger().total_hd(), 6);
        assert_eq!(state.earned_progress().spent_xp(), 1200);
    }
    #[test]
    fn disabled_future_source_retains_xp_without_holding_recipient_comparison_low() {
        let mut state = creature(1, false);
        state.award_xp(300).unwrap();
        assert_eq!(state.build().ledger().total_hd(), 1);
        assert_eq!(state.earned_progress().held_xp(), 300);
        assert_eq!(state.earned_progress().highest_earned_tier(), 3);
        let mut build = state.build().clone();
        build
            .set_future_growth(FutureGrowth::Selected {
                source: GrowthSource::Racial {},
            })
            .unwrap();
        state.rebuild(build).unwrap();
        assert_eq!(state.build().ledger().total_hd(), 3);
        assert_eq!(state.earned_progress().held_xp(), 0);
        assert_eq!(state.earned_progress().earned_xp(), 300);
        assert_eq!(state.build().unspent_points(), 3);
    }
    #[test]
    fn posthumous_owner_retains_xp_without_growth_or_resurrection() {
        let mut state = creature(1, true);
        drain(&mut state);
        assert!(state.health().dead());
        let health = state.health();
        state.award_xp(100).unwrap();
        assert_eq!(state.health(), health);
        assert_eq!(state.build().ledger().total_hd(), 0);
        assert_eq!(state.earned_progress().held_xp(), 100);
    }
    #[test]
    fn xp_at_the_structural_hit_die_cap_is_retained() {
        let mut state = creature(crate::limits::HIT_DICE, true);
        state.award_xp(1_000_000).unwrap();
        assert_eq!(state.build().ledger().total_hd(), 256);
        assert_eq!(state.earned_progress().held_xp(), 1_000_000);
        assert_eq!(state.earned_progress().spent_xp(), 0);
    }
    #[test]
    fn xp_overflow_rejects_without_partial_state_changes() {
        let mut state = creature(1, false);
        let mut progress = state.earned_progress().clone();
        progress.earned_xp = u64::MAX;
        progress.held_xp = u64::MAX;
        progress.highest_earned_tier = 256;
        state.replace_earned_progress(progress).unwrap();
        let before = state.clone();
        assert_eq!(
            state.award_xp(1),
            Err(CreatureStateError::Growth(GrowthError::Arithmetic))
        );
        assert_eq!(state, before);
    }
    #[test]
    fn restored_paid_tier_requires_its_minimum_spent_xp() {
        let state = creature(1, false);
        let mut progress = state.earned_progress().clone();
        progress.highest_applied_tier = 2;
        progress.highest_earned_tier = 2;
        assert_eq!(
            progress.validate(2, &state.build().ruleset().document().growth),
            Err(GrowthError::InvalidRecord)
        );
    }
    #[test]
    fn restored_comparison_tier_requires_its_minimum_earned_xp() {
        let state = creature(1, false);
        let mut progress = state.earned_progress().clone();
        progress.highest_earned_tier = 2;
        assert_eq!(
            progress.validate(1, &state.build().ruleset().document().growth),
            Err(GrowthError::InvalidRecord)
        );
    }
    #[test]
    fn recorded_only_owner_restores_consistent_retained_xp_without_inventing_a_root() {
        let state = CreatureState::new(crate::test_creatures::build(
            crate::test_creatures::species(),
        ))
        .unwrap();
        let mut value =
            serde_json::to_value(crate::creatures::record::CreatureRecord::capture(&state))
                .unwrap();
        value["earned_progress"]["earned_xp"] = serde_json::json!(100);
        value["earned_progress"]["held_xp"] = serde_json::json!(100);
        value["earned_progress"]["highest_earned_tier"] = serde_json::json!(2);
        let record: crate::creatures::record::CreatureRecord =
            serde_json::from_value(value).unwrap();
        let restored = record
            .restore(state.build().content().clone(), vec![])
            .unwrap();
        assert_eq!(restored.earned_progress().held_xp(), 100);
        assert_eq!(restored.build().ledger().health_root_seed(), None);
        assert_eq!(restored.build().ledger(), state.build().ledger());
    }
    #[test]
    fn unknown_health_stream_is_explicitly_ineligible_for_ordinary_growth() {
        let mut state = CreatureState::new(crate::test_creatures::build(
            crate::test_creatures::species(),
        ))
        .unwrap();
        let before = state.build().clone();
        state.award_xp(100).unwrap();
        assert_eq!(state.build(), &before);
        assert_eq!(state.earned_progress().held_xp(), 100);
        assert_eq!(state.build().ledger().health_root_seed(), None);
    }
    #[test]
    fn highest_earned_and_victim_peak_do_not_drop_with_temporary_drain() {
        let mut state = creature(6, true);
        let peak = state.earned_progress().peak_threat();
        let policy = state.build().ruleset().document().growth.clone();
        let reward = policy.reward(peak, state.earned_progress().highest_earned_tier());
        drain(&mut state);
        assert_eq!(state.earned_progress().peak_threat(), peak);
        assert_eq!(
            policy.reward(
                state.earned_progress().peak_threat(),
                state.earned_progress().highest_earned_tier()
            ),
            reward
        );
    }
    #[test]
    fn strict_progress_records_reject_missing_unknown_and_inconsistent_accounting() {
        let state = creature(1, false);
        let value = serde_json::to_value(state.earned_progress()).unwrap();
        for key in value.as_object().unwrap().keys() {
            let mut invalid = value.clone();
            invalid.as_object_mut().unwrap().remove(key);
            assert!(
                serde_json::from_value::<EarnedProgress>(invalid).is_err(),
                "{key}"
            );
        }
        let mut invalid = value.clone();
        invalid["extra"] = serde_json::json!(0);
        assert!(serde_json::from_value::<EarnedProgress>(invalid).is_err());
        let mut invalid = value;
        invalid["held_xp"] = serde_json::json!(1);
        assert!(serde_json::from_value::<EarnedProgress>(invalid).is_err());
    }
}

#[cfg(test)]
mod source_tests {
    use super::*;
    use crate::creatures::{
        BuildAdvancement, CreatureState, HdChoices, PlannedAdvancement, RestorationPolicy,
    };
    use crate::progression::{HdLedger, HdSource};

    #[test]
    fn authored_regrowth_restores_choices_while_choose_again_appends_an_empty_slot() {
        let original = crate::test_creatures::build(crate::test_creatures::species());
        for authored in [false, true] {
            let mut choice = HdChoices::default();
            choice
                .purchases
                .push(crate::creatures::AdvancementPurchase {
                    pool: crate::content_ids::PointPoolId::new("advancement").unwrap(),
                    selection: PurchaseSelection::SkillRank(crate::attributes::Skill::Athletics),
                });
            let intended = vec![
                PlannedAdvancement {
                    source: HdSource::Racial,
                    choices: HdChoices::default(),
                },
                PlannedAdvancement {
                    source: HdSource::Racial,
                    choices: choice.clone(),
                },
            ];
            let mut build = CreatureBuild::from_recorded(
                original.content().clone(),
                original.species_id().clone(),
                original.initial_attributes(),
                HdLedger::seeded(vec![HdSource::Racial; 2], 42).unwrap(),
                original.binding(),
                BuildAdvancement {
                    starting_skills: *original.starting_skills(),
                    choices: vec![HdChoices::default(), choice.clone()],
                    restoration: if authored {
                        RestorationPolicy::Authored(intended)
                    } else {
                        RestorationPolicy::ChooseAgain
                    },
                },
                vec![],
            )
            .unwrap();
            build
                .set_future_growth(FutureGrowth::Selected {
                    source: GrowthSource::Racial {},
                })
                .unwrap();
            let mut state = CreatureState::new(build).unwrap();
            let root = state.build().ledger().entries()[1].health_seed();
            let mut drained = state.build().clone();
            drained.remove_latest().unwrap();
            state.rebuild(drained).unwrap();
            state.award_xp(200).unwrap();
            assert_eq!(state.build().ledger().total_hd(), 2);
            assert_eq!(state.build().ledger().entries()[1].health_seed(), root);
            assert_eq!(
                state.build().choices()[1],
                if authored {
                    choice
                } else {
                    HdChoices::default()
                }
            );
        }
    }
    #[test]
    fn exhausted_authored_source_keeps_earned_value_until_an_explicit_future_source_is_selected() {
        let original = crate::test_creatures::build(crate::test_creatures::species());
        let mut build = CreatureBuild::from_recorded(
            original.content().clone(),
            original.species_id().clone(),
            original.initial_attributes(),
            HdLedger::seeded(vec![HdSource::Racial], 42).unwrap(),
            original.binding(),
            BuildAdvancement {
                starting_skills: *original.starting_skills(),
                choices: vec![HdChoices::default()],
                restoration: RestorationPolicy::Authored(vec![PlannedAdvancement {
                    source: HdSource::Racial,
                    choices: HdChoices::default(),
                }]),
            },
            vec![],
        )
        .unwrap();
        build.set_future_growth(FutureGrowth::Authored {}).unwrap();
        let mut state = CreatureState::new(build).unwrap();
        state.award_xp(100).unwrap();
        assert_eq!(state.build().ledger().total_hd(), 1);
        assert_eq!(state.earned_progress().held_xp(), 100);
        let mut next = state.build().clone();
        next.set_future_growth(FutureGrowth::Selected {
            source: GrowthSource::Class {
                class: crate::content_ids::ClassId::new("mage").unwrap(),
            },
        })
        .unwrap();
        state.rebuild(next).unwrap();
        assert_eq!(state.build().ledger().total_hd(), 2);
        assert_eq!(
            state.build().ledger().entries()[1].source(),
            HdSource::Class(crate::content_ids::ClassId::new("mage").unwrap())
        );
        assert_eq!(state.earned_progress().held_xp(), 0);
    }
    #[test]
    fn composed_threat_accounts_for_innate_layers_class_template_and_owned_training() {
        use crate::content_ids::*;
        let mut rules = crate::test_creatures::ruleset().document().clone();
        let policy = &mut rules.growth.threat;
        policy.points_per_tier = 10;
        policy.hit_die = 23;
        policy.trained_rank = 29;
        policy.species = 5;
        policy.creature_type = 7;
        policy.subtype = 11;
        policy.class = 17;
        policy.template = 13;
        policy.talent = 19;
        rules.talents.push(crate::test_creatures::talent(
            "fixture_threat_source",
            vec![],
        ));
        let mut species = crate::test_creatures::species();
        species.subtypes.insert(SubtypeId::new("fire").unwrap());
        let id = species.id.clone();
        let template =
            crate::creatures::Template::new(TemplateId::new("threat_template").unwrap(), 0);
        let content = crate::creatures::Content::new(
            crate::rules::Ruleset::validate(rules).unwrap(),
            crate::creatures::ScenarioCatalog {
                species: [(id.clone(), species)].into(),
                templates: [(template.id.clone(), template.clone())].into(),
            },
        )
        .unwrap();
        let mut build = CreatureBuild::new(
            content,
            id,
            HdLedger::seeded(
                vec![HdSource::Class(ClassId::new("warrior").unwrap()); 2],
                42,
            )
            .unwrap(),
            crate::attributes::ManaBinding::Intellect,
            [
                crate::attributes::Skill::Deception,
                crate::attributes::Skill::Persuasion,
            ],
        )
        .unwrap();
        build.set_templates(vec![template.id]).unwrap();
        build.train(0, crate::attributes::Skill::Athletics).unwrap();
        build
            .select_talent(1, TalentId::new("fixture_threat_source").unwrap())
            .unwrap();
        let assessment = assess(&build, &build.derive().unwrap()).unwrap();
        assert_eq!(assessment.hit_dice_points, 46);
        assert_eq!(assessment.training_points, 87);
        assert_eq!(assessment.sources.len(), 6);
        assert_eq!(assessment.total, 205);
        assert_eq!(assessment.tier, 21);
        let mut state = CreatureState::new(build).unwrap();
        let peak = state.earned_progress().peak_threat();
        state
            .set_equipment(vec![crate::creatures::EquipmentSource {
                item: crate::ItemId(1),
                socket: crate::EquipmentSlotId(0),
                attack: None,
                modifiers: vec![crate::rules::Modifier {
                    scope: crate::rules::Scope::Always,
                    op: crate::rules::ModifierOp::Add {
                        stat: crate::rules::Stat::Health,
                        value: 100,
                    },
                }],
            }])
            .unwrap();
        state
            .apply_condition(
                &ConditionId::new("readied").unwrap(),
                crate::ActorId(2),
                100,
            )
            .unwrap();
        assert_eq!(
            state.earned_progress().peak_threat(),
            peak,
            "temporary sources cannot inflate earned threat"
        );
    }
}
