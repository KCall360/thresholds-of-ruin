//! Actor-owned earned XP and explicit future HD source policy.
//!
//! XP applies HD immediately when its declared source is available. A missing
//! source retains the XP balance; it never silently spends or discards it.
use crate::creatures::{BuildError, CreatureBuild, DerivedCreature, PurchaseSelection};
use crate::rules::{GrowthPolicy, ThreatSource};
use crate::HdCount;
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
    authored_baseline: HdCount,
    highest_applied_tier: HdCount,
    highest_earned_tier: HdCount,
    peak_threat: u64,
    earned_xp: u64,
    spent_xp: u64,
    held_xp: u64,
}
impl<'de> Deserialize<'de> for EarnedProgress {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            authored_baseline: HdCount,
            highest_applied_tier: HdCount,
            highest_earned_tier: HdCount,
            peak_threat: u64,
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
    pub(crate) fn new(hit_dice: HdCount, threat: u64) -> Self {
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
    pub fn highest_earned_tier(&self) -> HdCount {
        self.highest_earned_tier
    }
    pub fn highest_applied_tier(&self) -> HdCount {
        self.highest_applied_tier
    }
    pub fn peak_threat(&self) -> u64 {
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
    pub fn next_threshold(&self, policy: &GrowthPolicy) -> Result<u64, GrowthError> {
        policy
            .threshold(self.highest_applied_tier)
            .ok_or(GrowthError::Arithmetic)
    }
    fn shape_valid(&self) -> bool {
        self.authored_baseline <= self.highest_applied_tier
            && self.highest_applied_tier <= self.highest_earned_tier
            && self.peak_threat > 0
            && self.spent_xp.checked_add(self.held_xp) == Some(self.earned_xp)
    }
    pub(crate) fn validate(
        &self,
        current: HdCount,
        policy: &GrowthPolicy,
    ) -> Result<(), GrowthError> {
        if self.shape_valid()
            && current <= self.highest_applied_tier
            && self.next_threshold(policy).is_ok()
            && policy
                .threshold_sum(self.authored_baseline, self.highest_applied_tier)
                .is_some_and(|minimum| self.spent_xp >= minimum)
            && policy
                .threshold_sum(self.authored_baseline, self.highest_earned_tier)
                .is_some_and(|minimum| self.earned_xp >= minimum)
            && self
                .projected_tier(current, policy)
                .is_ok_and(|tier| tier <= self.highest_earned_tier)
        {
            Ok(())
        } else {
            Err(GrowthError::InvalidRecord)
        }
    }
    fn projected_tier(
        &self,
        current: HdCount,
        policy: &GrowthPolicy,
    ) -> Result<HdCount, GrowthError> {
        let tier = self.highest_applied_tier;
        let lost = tier
            .checked_sub(current)
            .ok_or(GrowthError::InvalidRecord)?
            .get();
        let price = self.next_threshold(policy)?;
        if price == 0 || policy.threshold_base == 0 {
            return Err(GrowthError::InvalidRecord);
        }
        let mut xp = self.held_xp;
        // Regrowth pays today's applied-tier price, not the historic price of
        // each lost ordinal, and cannot increase the applied-tier watermark.
        let restored = lost.min(xp / price);
        xp -= restored.checked_mul(price).ok_or(GrowthError::Arithmetic)?;
        if restored < lost {
            return Ok(tier);
        }

        let room = u64::MAX - tier.get();
        let mut low = 0;
        let mut high = room.min(xp / u64::from(policy.threshold_base));
        while low < high {
            let distance = high - low;
            let middle = low + distance / 2 + distance % 2;
            let end = tier.checked_add(middle).ok_or(GrowthError::Arithmetic)?;
            if policy
                .threshold_sum(tier, end)
                .is_some_and(|cost| cost <= xp)
            {
                low = middle;
            } else {
                high = middle - 1;
            }
        }
        let projected = tier.checked_add(low).ok_or(GrowthError::Arithmetic)?;
        if low == room {
            let spent = policy
                .threshold_sum(tier, projected)
                .ok_or(GrowthError::Arithmetic)?;
            // Exhausted representation is a hard error if another ordinal is
            // affordable. A binary-search endpoint must never become a plateau.
            if policy
                .threshold(projected)
                .is_some_and(|next| xp - spent >= next)
            {
                return Err(GrowthError::Arithmetic);
            }
        }
        Ok(projected)
    }
    pub(crate) fn award(
        &mut self,
        amount: u32,
        current: HdCount,
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
        let mut candidate = self.clone();
        candidate.earned_xp = earned;
        candidate.held_xp = held;
        candidate.highest_earned_tier = candidate
            .highest_earned_tier
            .max(candidate.projected_tier(current, policy)?);
        *self = candidate;
        Ok(())
    }
    pub(crate) fn reconcile_build(
        &mut self,
        current: HdCount,
        threat: u64,
        policy: &GrowthPolicy,
    ) -> Result<(), GrowthError> {
        // General-purpose source/wizard additions establish an authored baseline;
        // ordinary growth records its paid tier before invoking this boundary.
        let mut candidate = self.clone();
        if current > candidate.highest_applied_tier {
            candidate.authored_baseline = candidate.authored_baseline.max(current);
            candidate.highest_applied_tier = current;
        }
        candidate.peak_threat = candidate.peak_threat.max(threat);
        candidate.highest_earned_tier = candidate
            .highest_earned_tier
            .max(candidate.projected_tier(current, policy)?);
        candidate.validate(current, policy)?;
        *self = candidate;
        Ok(())
    }
    pub(crate) fn spend(
        &mut self,
        next_ordinal: HdCount,
        policy: &GrowthPolicy,
    ) -> Result<(), GrowthError> {
        let price = self.next_threshold(policy)?;
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
    pub points: u64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThreatAssessment {
    pub sources: Vec<ThreatContribution>,
    pub hit_dice_points: u64,
    pub training_points: u64,
    pub total: u64,
    pub tier: u64,
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
        crate::diagnostics::raw_hit_die_visit();
        if let crate::progression::HdSource::Class(id) = die.source() {
            identities.insert(ThreatSource::Class { id });
        }
    }
    let mut trained = 2_u64;
    for choice in build.choices() {
        crate::diagnostics::advancement_choice_visit();
        for purchase in &choice.purchases {
            match &purchase.selection {
                PurchaseSelection::SkillRank(_) => {
                    trained = trained.checked_add(1).ok_or(BuildError::Arithmetic)?
                }
                // Effective expertise benefits are counted once below, even
                // when the same ID has several independent owned grant sources.
                PurchaseSelection::Expertise(_) => {}
                PurchaseSelection::Talent(id)
                | PurchaseSelection::TalentForm { talent: id, .. } => {
                    identities.insert(ThreatSource::Talent { id: id.clone() });
                }
            }
        }
    }
    identities.extend(
        permanent
            .expertises
            .active_ids()
            .cloned()
            .map(|id| ThreatSource::Expertise { id }),
    );
    let sources: Vec<_> = identities
        .into_iter()
        .map(|source| ThreatContribution {
            points: u64::from(policy.contribution(&source)),
            source,
        })
        .collect();
    let hit_dice_points = u64::from(policy.hit_die)
        .checked_mul(build.ledger().total_hd().get())
        .ok_or(BuildError::Arithmetic)?;
    let training_points = u64::from(policy.trained_rank)
        .checked_mul(trained)
        .ok_or(BuildError::Arithmetic)?;
    let total = sources.iter().try_fold(
        hit_dice_points
            .checked_add(training_points)
            .ok_or(BuildError::Arithmetic)?,
        |sum, source| sum.checked_add(source.points).ok_or(BuildError::Arithmetic),
    )?;
    let tier = total.div_ceil(u64::from(policy.points_per_tier)).max(1);
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
    fn iterative_projection(current: u64, highest: u64, mut xp: u64, policy: &GrowthPolicy) -> u64 {
        let mut ordinal = current;
        let mut tier = highest;
        loop {
            let cost = u64::from(policy.threshold_base)
                + u64::from(policy.threshold_per_tier) * tier.saturating_sub(1);
            if xp < cost {
                return tier;
            }
            xp -= cost;
            ordinal += 1;
            tier = tier.max(ordinal);
        }
    }
    fn iterative_minimum(first: u64, end: u64, policy: &GrowthPolicy) -> u64 {
        (first..end)
            .map(|tier| {
                u64::from(policy.threshold_base)
                    + u64::from(policy.threshold_per_tier) * tier.saturating_sub(1)
            })
            .sum()
    }
    fn account(baseline: u64, applied: u64, earned: u64, spent: u64, held: u64) -> EarnedProgress {
        EarnedProgress {
            authored_baseline: HdCount(baseline),
            highest_applied_tier: HdCount(applied),
            highest_earned_tier: HdCount(earned),
            peak_threat: 1,
            earned_xp: spent.checked_add(held).unwrap(),
            spent_xp: spent,
            held_xp: held,
        }
    }

    #[test]
    fn wide_scalar_analytical_projection_and_sums_match_the_iterative_oracle_exhaustively() {
        let mut policy = crate::test_creatures::ruleset().document().growth.clone();
        for base in 1..=4 {
            for slope in 0..=3 {
                policy.threshold_base = base;
                policy.threshold_per_tier = slope;
                for highest in 0..=10 {
                    for current in 0..=highest {
                        for held in 0..=160 {
                            let progress = account(0, highest, highest, 0, held);
                            assert_eq!(progress.projected_tier(HdCount(current), &policy).unwrap().get(),
                                iterative_projection(current, highest, held, &policy),
                                "base={base} slope={slope} highest={highest} current={current} held={held}");
                        }
                    }
                    for baseline in 0..=highest {
                        assert_eq!(
                            policy.threshold_sum(HdCount(baseline), HdCount(highest)),
                            Some(iterative_minimum(baseline, highest, &policy))
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn wide_scalar_saved_account_validation_matches_small_iterative_states() {
        let mut policy = crate::test_creatures::ruleset().document().growth.clone();
        policy.threshold_base = 1;
        for slope in 0..=2 {
            policy.threshold_per_tier = slope;
            for baseline in 0..=3 {
                for applied in baseline..=5 {
                    for earned in applied..=7 {
                        for current in 0..=applied {
                            for spent in 0..=30 {
                                for held in 0..=30 {
                                    let progress = account(baseline, applied, earned, spent, held);
                                    let expected = spent
                                        >= iterative_minimum(baseline, applied, &policy)
                                        && spent + held
                                            >= iterative_minimum(baseline, earned, &policy)
                                        && iterative_projection(current, applied, held, &policy)
                                            <= earned;
                                    assert_eq!(progress.validate(HdCount(current), &policy).is_ok(), expected,
                                        "slope={slope} baseline={baseline} applied={applied} earned={earned} current={current} spent={spent} held={held}");
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn wide_scalar_xp_prices_and_extreme_accounts_are_checked_without_untrusted_loops() {
        let mut policy = crate::test_creatures::ruleset().document().growth.clone();
        assert_eq!(policy.threshold(HdCount(0)), Some(100));
        assert_eq!(policy.threshold(HdCount(1)), Some(100));
        assert_eq!(policy.threshold_sum(HdCount(0), HdCount(1)), Some(100));
        assert_eq!(policy.threshold(HdCount(65_537)), Some(6_553_700));
        assert_eq!(policy.threshold(HdCount(u64::MAX)), None);
        assert_eq!(policy.threshold_sum(HdCount(0), HdCount(u64::MAX)), None);
        assert_eq!(policy.threshold_sum(HdCount(2), HdCount(1)), None);
        let dishonest = account(1, u64::MAX, u64::MAX, 0, 0);
        assert_eq!(
            dishonest.validate(HdCount(1), &policy),
            Err(GrowthError::InvalidRecord)
        );
        policy.threshold_base = 1;
        policy.threshold_per_tier = 0;
        assert_eq!(
            policy.threshold_sum(HdCount(0), HdCount(u64::MAX)),
            Some(u64::MAX)
        );
        assert_eq!(
            account(0, 0, 0, 0, u64::MAX).projected_tier(HdCount(0), &policy),
            Ok(HdCount(u64::MAX))
        );
        assert_eq!(
            account(1, 1, 1, 0, u64::MAX).projected_tier(HdCount(1), &policy),
            Err(GrowthError::Arithmetic)
        );
        let mut exhausted = account(u64::MAX, u64::MAX, u64::MAX, 0, 0);
        assert_eq!(
            exhausted.projected_tier(HdCount(u64::MAX), &policy),
            Ok(HdCount(u64::MAX))
        );
        let before = exhausted.clone();
        assert_eq!(
            exhausted.award(1, HdCount(u64::MAX), &policy),
            Err(GrowthError::Arithmetic)
        );
        assert_eq!(
            exhausted, before,
            "hard count exhaustion cannot partially credit XP"
        );
        assert_eq!(
            account(u64::MAX, u64::MAX, u64::MAX, 0, 1)
                .projected_tier(HdCount(u64::MAX - 1), &policy),
            Ok(HdCount(u64::MAX))
        );
        assert_eq!(policy.reward(u64::MAX, HdCount(0)), policy.maximum_reward);
        assert_eq!(policy.reward(0, HdCount(u64::MAX)), policy.minimum_reward);
    }

    #[test]
    fn wide_scalar_funded_projection_is_not_the_actual_record_admission_limit() {
        let mut state = creature(1, false);
        state.award_xp(4_000_000).unwrap();
        assert_eq!(state.build().ledger().total_hd().get(), 1);
        assert_eq!(state.earned_progress().highest_earned_tier().get(), 283);
        assert_eq!(state.earned_progress().held_xp(), 4_000_000);
    }

    #[test]
    fn wide_scalar_threat_is_not_clamped_to_the_actual_record_limit() {
        let original = creature(1, false);
        let mut document = original.build().ruleset().document().clone();
        document.growth.threat.hit_die = 1_000_000;
        document.growth.threat.points_per_tier = 1;
        let content = crate::creatures::Content::new(
            crate::rules::Ruleset::validate(document).unwrap(),
            original.build().content().catalog().clone(),
        )
        .unwrap();
        let build = CreatureBuild::new(
            content,
            original.build().species_id().clone(),
            original.build().ledger().clone(),
            original.build().binding(),
            *original.build().starting_skills(),
        )
        .unwrap();
        let threat = assess(&build, &build.derive().unwrap()).unwrap();
        assert!(threat.tier >= 1_000_000);
    }

    #[test]
    fn wide_scalar_policy_validates_coefficients_without_a_million_price_ceiling() {
        let mut document = crate::test_creatures::ruleset().document().clone();
        document.growth.threshold_per_tier = 1_000_000;
        assert!(crate::rules::Ruleset::validate(document).is_ok());
    }

    #[test]
    fn attribute_restoration_retries_funded_growth_without_editing_the_build_epoch() {
        use crate::attributes::{Attribute, Attributes, ByAttribute};
        use crate::content_ids::{AbilityId, ClassId};
        use crate::creatures::Content;
        use crate::rules::{Modifier, ModifierOp, Scope, Timing, TimingSource};

        let mut species = crate::test_creatures::species();
        species.default_attributes = Attributes::new(ByAttribute {
            speed: 1,
            ..ByAttribute::default()
        })
        .unwrap();
        let original = crate::test_creatures::build(species);
        let mut document = original.ruleset().document().clone();
        document.constants.duration_scale_numerator = 2;
        document.constants.duration_scale_offset = 1;
        let mut ability = document.abilities[0].clone();
        ability.id = AbilityId::new("fixture_growth_timing").unwrap();
        ability.basic_attack = false;
        ability.timing = Timing {
            source: TimingSource::Fixed {
                prepare: 1_000_000,
                recover: 1,
            },
            scaled_by: Some(Attribute::Speed),
        };
        let ability_id = ability.id.clone();
        document.abilities.push(ability);
        let class = ClassId::new("warrior").unwrap();
        document
            .classes
            .iter_mut()
            .find(|entry| entry.id == class)
            .unwrap()
            .modifiers
            .push(Modifier {
                scope: Scope::Always,
                op: ModifierOp::GrantAbility {
                    ability: ability_id,
                },
            });
        let content = Content::new(
            crate::rules::Ruleset::validate(document).unwrap(),
            original.content().catalog().clone(),
        )
        .unwrap();
        let mut build = CreatureBuild::new(
            content,
            original.species_id().clone(),
            HdLedger::seeded(vec![HdSource::Racial], 42).unwrap(),
            original.binding(),
            *original.starting_skills(),
        )
        .unwrap();
        build
            .set_future_growth(FutureGrowth::Selected {
                source: GrowthSource::Class { class },
            })
            .unwrap();
        let mut state = CreatureState::new(build).unwrap();
        state.lose_attribute(Attribute::Speed, 1).unwrap();
        state.award_xp(100).unwrap();
        assert_eq!(state.build().ledger().total_hd().get(), 1);
        assert_eq!(state.earned_progress().held_xp(), 100);
        let before = state.clone();
        let epoch = state.build_epoch();
        state.restore_attribute(Attribute::Speed, 1).unwrap();
        assert_eq!(state.build_epoch(), epoch);
        state
            .retry_growth_after_fold(&before, |_| {
                Ok(crate::health_recovery::RecoveryContext::Known {
                    sensed_hostile: false,
                })
            })
            .unwrap();
        assert_eq!(state.build().ledger().total_hd().get(), 2);
        assert_eq!(state.earned_progress().held_xp(), 0);

        // Install the same valid funded/obstructed fixture in a real Game, then
        // let ordinary restoration publication own the retry. No test calls
        // the retry hook on this path.
        let mut game = crate::Game::two_room_in_stone(42, before.build().content().clone());
        let actor = game
            .spawn_actor(
                tor_world::Location {
                    region: tor_world::RegionId(1),
                    position: tor_world::Position { x: 1, y: 1, z: 0 },
                },
                std::num::NonZeroU64::new(100).unwrap(),
            )
            .unwrap();
        let identity = crate::CreatureIdentity {
            name: "restoration owner".into(),
            faction: "neutral".into(),
        };
        game.configure_creature(actor, identity.clone(), before.build().clone())
            .unwrap();
        game.actors.get_mut(&actor).unwrap().combat = Some(
            crate::actor_creatures::CombatState::from_creature(identity, before.clone()),
        );
        game.apply_effects(
            actor,
            &[crate::rules::Effect::RestoreAttribute {
                attribute: Attribute::Speed,
                amount: crate::rules::AmountExpr::Fixed { amount: 1 },
            }],
        )
        .unwrap();
        let grown = game.creature(actor).unwrap();
        assert_eq!(grown.build(), state.build());
        assert_eq!(grown.build_epoch().get(), epoch.get() + 1);
        assert_eq!(grown.earned_progress(), state.earned_progress());
        assert_eq!(grown.restorable_losses(), state.restorable_losses());
    }

    #[test]
    fn sixty_percent_survives_drain_and_fresh_xp_regrows_six_instead_of_seven() {
        let mut state = creature(6, true);
        let retained = state.build().ledger().entries().to_vec();
        state.award_xp(360).unwrap();
        drain(&mut state);
        assert_eq!(state.earned_progress().held_xp(), 360);
        assert_eq!(state.earned_progress().highest_earned_tier().get(), 6);
        assert_eq!(state.build().ledger().total_hd().get(), 5);
        state.award_xp(240).unwrap();
        assert_eq!(state.build().ledger().total_hd().get(), 6);
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
        assert_eq!(state.build().ledger().total_hd().get(), 7);
        assert_eq!(state.earned_progress().highest_earned_tier().get(), 7);
        assert_eq!(state.earned_progress().spent_xp(), 600);
    }
    #[test]
    fn another_drain_never_reuses_an_already_completed_threshold() {
        let mut state = creature(6, true);
        drain(&mut state);
        state.award_xp(600).unwrap();
        drain(&mut state);
        assert_eq!(state.build().ledger().total_hd().get(), 5);
        assert_eq!(state.earned_progress().held_xp(), 0);
        state.award_xp(599).unwrap();
        assert_eq!(state.build().ledger().total_hd().get(), 5);
        state.award_xp(1).unwrap();
        assert_eq!(state.build().ledger().total_hd().get(), 6);
        assert_eq!(state.earned_progress().spent_xp(), 1200);
    }
    #[test]
    fn disabled_future_source_retains_xp_without_holding_recipient_comparison_low() {
        let mut state = creature(1, false);
        state.award_xp(300).unwrap();
        assert_eq!(state.build().ledger().total_hd().get(), 1);
        assert_eq!(state.earned_progress().held_xp(), 300);
        assert_eq!(state.earned_progress().highest_earned_tier().get(), 3);
        let mut build = state.build().clone();
        build
            .set_future_growth(FutureGrowth::Selected {
                source: GrowthSource::Racial {},
            })
            .unwrap();
        state.rebuild(build).unwrap();
        assert_eq!(state.build().ledger().total_hd().get(), 3);
        assert_eq!(state.earned_progress().held_xp(), 0);
        assert_eq!(state.earned_progress().earned_xp(), 300);
        assert_eq!(state.build().unspent_points(), 3);
    }

    #[test]
    fn required_nutrition_same_policy_rebuild_grows_without_resetting_runtime() {
        use crate::creatures::{record::CreatureRecord, Content};
        use crate::nutrition::{Nutrition, NutritionPolicy, NutritionRequirement};

        let original = creature(1, false);
        let mut document = original.build().ruleset().document().clone();
        document.nutrition = NutritionPolicy::Enabled {
            initial: 8,
            maximum: 12,
            hungry_below: 7,
            weak_below: 3,
            starve_at: -3,
            unit_ticks: 100,
            hungry_recovery: crate::health_recovery::RecoveryNutrition::Slowed { multiplier: 2 },
            weak_recovery: crate::health_recovery::RecoveryNutrition::Stopped,
            empty_recovery: crate::health_recovery::RecoveryNutrition::Stopped,
            regeneration_cost: crate::nutrition::RegenerationCost {
                denominator: 1,
                health: 0,
                stamina: 0,
                focus: 0,
                mana: 0,
            },
        };
        let content = Content::new(
            crate::rules::Ruleset::validate(document).unwrap(),
            original.build().content().catalog().clone(),
        )
        .unwrap();
        let build = CreatureBuild::new(
            content,
            original.build().species_id().clone(),
            original.build().ledger().clone(),
            original.build().binding(),
            *original.build().starting_skills(),
        )
        .unwrap();
        let mut state = CreatureState::new(build).unwrap();
        state
            .configure_nutrition(NutritionRequirement::Required)
            .unwrap();
        state.award_xp(300).unwrap();
        state.damage(2);
        state
            .apply_condition(
                &crate::content_ids::ConditionId::new("fear").unwrap(),
                crate::ActorId(9),
                500,
            )
            .unwrap();
        state.advance_active(237);
        let before = state.clone();
        assert_eq!(state.earned_progress().held_xp(), 300);
        assert_eq!(
            state.nutrition(),
            Nutrition::Required {
                reserve: 6,
                elapsed: 37,
                digestion_remainder: 0,
                regeneration_remainder: 0
            }
        );
        assert!(state.health_recovery().elapsed() > 0);
        assert!(!state.conditions().is_empty());
        state.rebuild(state.build().clone()).unwrap();
        assert_eq!(
            state, before,
            "identical rebuild retains every clock and XP"
        );

        let mut selected = state.build().clone();
        selected
            .set_future_growth(FutureGrowth::Selected {
                source: GrowthSource::Racial {},
            })
            .unwrap();
        state.rebuild(selected).unwrap();
        assert_eq!(state.build().ledger().total_hd().get(), 3);
        assert_eq!(state.earned_progress().earned_xp(), 300);
        assert_eq!(state.earned_progress().spent_xp(), 300);
        assert_eq!(state.earned_progress().held_xp(), 0);
        assert_eq!(state.nutrition(), before.nutrition());
        // Growth changes the recovery denominator, not the nutrition policy.
        // Each earned HD preserves its fractional work with the ordinary floor:
        // floor(floor(437 * 2320 / 2400) * 2240 / 2320) = 407.
        assert_eq!(before.health_recovery().period(), 2400);
        assert_eq!(before.health_recovery().elapsed(), 437);
        assert_eq!(state.health_recovery().period(), 2240);
        assert_eq!(state.health_recovery().elapsed(), 407);
        assert_eq!(
            state.health_recovery().nutrition(),
            before.health_recovery().nutrition()
        );
        assert_eq!(state.health().injury(), before.health().injury());
        assert_eq!(state.conditions(), before.conditions());
        assert_eq!(
            state.received_health_loss_remaining(),
            before.received_health_loss_remaining()
        );
        assert_eq!(
            CreatureRecord::capture(&state)
                .restore(state.build().content().clone(), vec![])
                .unwrap(),
            state
        );
    }

    #[test]
    fn posthumous_owner_retains_xp_without_growth_or_resurrection() {
        let mut state = creature(1, true);
        drain(&mut state);
        assert!(state.health().dead());
        let health = state.health();
        state.award_xp(100).unwrap();
        assert_eq!(state.health(), health);
        assert_eq!(state.build().ledger().total_hd().get(), 0);
        assert_eq!(state.earned_progress().held_xp(), 100);
    }
    #[test]
    fn xp_at_the_structural_hit_die_cap_is_retained() {
        let mut state = creature(crate::limits::HIT_DICE, true);
        state.award_xp(1_000_000).unwrap();
        assert_eq!(state.build().ledger().total_hd().get(), 256);
        assert_eq!(state.earned_progress().held_xp(), 1_000_000);
        assert_eq!(state.earned_progress().spent_xp(), 0);
    }
    #[test]
    fn xp_overflow_rejects_without_partial_state_changes() {
        let mut state = creature(1, false);
        let mut progress = state.earned_progress().clone();
        progress.earned_xp = u64::MAX;
        progress.held_xp = u64::MAX;
        progress.highest_earned_tier = progress
            .projected_tier(
                state.build().ledger().total_hd(),
                &state.build().ruleset().document().growth,
            )
            .unwrap();
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
        progress.highest_applied_tier = HdCount(2);
        progress.highest_earned_tier = HdCount(2);
        assert_eq!(
            progress.validate(HdCount(2), &state.build().ruleset().document().growth),
            Err(GrowthError::InvalidRecord)
        );
    }
    #[test]
    fn restored_comparison_tier_requires_its_minimum_earned_xp() {
        let state = creature(1, false);
        let mut progress = state.earned_progress().clone();
        progress.highest_earned_tier = HdCount(2);
        assert_eq!(
            progress.validate(HdCount(1), &state.build().ruleset().document().growth),
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
            assert_eq!(state.build().ledger().total_hd().get(), 2);
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
        assert_eq!(state.build().ledger().total_hd().get(), 1);
        assert_eq!(state.earned_progress().held_xp(), 100);
        let mut next = state.build().clone();
        next.set_future_growth(FutureGrowth::Selected {
            source: GrowthSource::Class {
                class: crate::content_ids::ClassId::new("mage").unwrap(),
            },
        })
        .unwrap();
        state.rebuild(next).unwrap();
        assert_eq!(state.build().ledger().total_hd().get(), 2);
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
        build
            .train(crate::HdIndex(0), crate::attributes::Skill::Athletics)
            .unwrap();
        build
            .select_talent(
                crate::HdIndex(1),
                TalentId::new("fixture_threat_source").unwrap(),
            )
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
                weapon_profile: None,
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
