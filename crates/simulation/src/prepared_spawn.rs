//! Immutable, catalog-normalized construction inputs shared by ordinary setup
//! and authoritative population. These are recipes, never live entity state.
use crate::creatures::{
    record::BuildRecord, BuildError, CreatureBuild, HdChoices, PlannedAdvancement,
    RestorationPolicy,
};
use crate::{ActorId, AnatomySpec, BodySpec, CreatureIdentity, Game, GameError, ItemId, ItemSpec};
use serde::{Deserialize, Serialize};
use tor_world::Location;

/// Birth-only construction policy. This never changes a live actor on revisit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum DepthGrowth {
    Fixed {},
    RacialAfter { depth: u32 },
}

/// An ordinary purchase row applied only when birth adds this exact racial HD.
/// This recipe is never consulted by live-sheet queries or earned growth.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct OrdinalAdvancement {
    pub ordinal: crate::HdCount,
    #[serde(serialize_with = "serialize_choices")]
    pub choices: HdChoices,
}
fn serialize_choices<S: serde::Serializer>(
    choices: &HdChoices,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    crate::creatures::record::ChoiceRecord::capture(choices).serialize(serializer)
}
impl<'de> Deserialize<'de> for OrdinalAdvancement {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            ordinal: crate::HdCount,
            #[serde(deserialize_with = "crate::bounded::named_map")]
            choices: crate::creatures::record::ChoiceRecord,
        }
        let fields: Fields = crate::bounded::named_map(deserializer)?;
        if fields.ordinal.get() == 0 || fields.ordinal.get() > crate::limits::HIT_DICE as u64 {
            return Err(serde::de::Error::custom(
                "birth advancement ordinal is out of bounds",
            ));
        }
        Ok(Self {
            ordinal: fields.ordinal,
            choices: fields.choices.restore(),
        })
    }
}

fn validate_ordinals(base: u64, schedule: &[OrdinalAdvancement]) -> Result<(), BuildError> {
    if schedule.len() > crate::limits::HIT_DICE {
        return Err(BuildError::TooManyHitDice);
    }
    let mut previous = base;
    for entry in schedule {
        if entry.ordinal.get() <= previous || entry.ordinal.get() > crate::limits::HIT_DICE as u64 {
            return Err(BuildError::InvalidOwnership);
        }
        if entry.choices.purchases.len() > crate::limits::POINTS_PER_HIT_DIE {
            return Err(BuildError::NoAdvancementPoint);
        }
        previous = entry.ordinal.get();
    }
    Ok(())
}

/// Operational limits, not gameplay level caps. Hosts disclose no private counts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PopulationCapacity {
    HitDice { requested: u64, maximum: u64 },
    Arithmetic,
    Allocation,
    DerivedSheet,
}

impl DepthGrowth {
    pub fn valid(self) -> bool {
        match self {
            Self::Fixed {} => true,
            Self::RacialAfter { depth } => depth > 0,
        }
    }
    fn additional(self, depth: u32) -> u64 {
        match self {
            Self::Fixed {} => 0,
            Self::RacialAfter { depth: after } => {
                if depth > after {
                    u64::from(depth) - u64::from(after)
                } else {
                    0
                }
            }
        }
    }
    /// A shortened authored recipe must not lose or overwrite its future ordinals.
    pub fn validate_build(self, build: Option<&CreatureBuild>) -> Result<(), BuildError> {
        if !self.valid() {
            return Err(BuildError::InvalidDefinition);
        }
        if let Self::RacialAfter { .. } = self {
            let build = build.ok_or(BuildError::InvalidDefinition)?;
            if let RestorationPolicy::Authored(plan) = build.restoration_policy() {
                if plan.len() != build.ledger().entries().len() {
                    return Err(BuildError::InvalidOwnership);
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PreparedActor {
    pub build: Option<BuildRecord>,
    pub nutrition: crate::nutrition::NutritionRequirement,
    pub identity: Option<CreatureIdentity>,
    pub anatomy: Option<AnatomySpec>,
    pub body: Option<BodySpec>,
    pub ai: Option<crate::ai::AiProfile>,
    pub known_identities: Vec<String>,
    pub velocity: Option<[i64; 3]>,
    pub asset: Option<String>,
}

impl<'de> Deserialize<'de> for PreparedActor {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            #[serde(deserialize_with = "crate::bounded::required_option")]
            build: Option<BuildRecord>,
            nutrition: crate::nutrition::NutritionRequirement,
            #[serde(deserialize_with = "crate::bounded::required_option")]
            identity: Option<CreatureIdentity>,
            #[serde(deserialize_with = "crate::bounded::required_option")]
            anatomy: Option<AnatomySpec>,
            #[serde(deserialize_with = "crate::bounded::required_option")]
            body: Option<BodySpec>,
            #[serde(deserialize_with = "crate::bounded::required_option")]
            ai: Option<crate::ai::AiProfile>,
            known_identities: crate::bounded::Bounded<String, 4096>,
            #[serde(deserialize_with = "crate::bounded::required_option")]
            velocity: Option<[i64; 3]>,
            #[serde(deserialize_with = "crate::bounded::required_option")]
            asset: Option<String>,
        }
        let fields: Fields = crate::bounded::named_map(deserializer)?;
        Ok(Self {
            build: fields.build,
            nutrition: fields.nutrition,
            identity: fields.identity,
            anatomy: fields.anatomy,
            body: fields.body,
            ai: fields.ai,
            known_identities: fields.known_identities.0,
            velocity: fields.velocity,
            asset: fields.asset,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PreparedSpawnError {
    Capacity(PopulationCapacity),
    Anatomy(GameError),
    Knowledge(GameError),
    Build(BuildError),
    Creature(GameError),
    Nutrition(GameError),
    Ai(GameError),
    Body(GameError),
    Velocity(GameError),
    Asset(GameError),
}

impl From<PreparedSpawnError> for GameError {
    fn from(error: PreparedSpawnError) -> Self {
        match error {
            PreparedSpawnError::Capacity(error) => Self::PopulationCapacity(error),
            PreparedSpawnError::Build(error) => Self::Build(error),
            PreparedSpawnError::Anatomy(error)
            | PreparedSpawnError::Knowledge(error)
            | PreparedSpawnError::Creature(error)
            | PreparedSpawnError::Nutrition(error)
            | PreparedSpawnError::Ai(error)
            | PreparedSpawnError::Body(error)
            | PreparedSpawnError::Velocity(error)
            | PreparedSpawnError::Asset(error) => error,
        }
    }
}

impl PreparedSpawnError {
    /// Keep operational failure distinct from an authored sheet obstruction.
    fn into_build_error(self) -> BuildError {
        match self {
            Self::Build(error) => error,
            Self::Capacity(PopulationCapacity::HitDice { .. }) => BuildError::TooManyHitDice,
            Self::Capacity(PopulationCapacity::Arithmetic) => BuildError::Arithmetic,
            Self::Capacity(PopulationCapacity::Allocation) => BuildError::Allocation,
            Self::Capacity(PopulationCapacity::DerivedSheet) => BuildError::DerivedLimit,
            Self::Anatomy(_)
            | Self::Knowledge(_)
            | Self::Creature(_)
            | Self::Nutrition(_)
            | Self::Ai(_)
            | Self::Body(_)
            | Self::Velocity(_)
            | Self::Asset(_) => BuildError::InvalidDefinition,
        }
    }
}

impl CreatureBuild {
    /// Reseed an immutable prototype using ordinary exact-HD reconstruction.
    pub fn instantiate_seeded(&self, seed: u64) -> Result<Self, BuildError> {
        self.instantiate_seeded_racial(seed, 0)
            .map_err(PreparedSpawnError::into_build_error)
    }

    /// Materialize the complete actual ledger without rebuilding each growing
    /// prefix. Prototype restoration, build validation and actor installation
    /// retain their separate derivation/compilation boundaries for each birth.
    /// Empty added choices retain their ordinary unspent grants; a count-only
    /// suffix never substitutes for the authoritative per-HD records.
    pub fn instantiate_seeded_racial(
        &self,
        seed: u64,
        additional: u64,
    ) -> Result<Self, PreparedSpawnError> {
        self.instantiate_seeded_racial_with_advancement(seed, additional, &[])
    }

    /// Apply bounded authored choices only to newly materialized exact-HD rows.
    /// Normal reconstruction validates pool budgets, prerequisites and rank caps.
    pub fn instantiate_seeded_racial_with_advancement(
        &self,
        seed: u64,
        additional: u64,
        schedule: &[OrdinalAdvancement],
    ) -> Result<Self, PreparedSpawnError> {
        use crate::progression::{HdLedger, HdSource};
        let base = u64::try_from(self.ledger().entries().len())
            .map_err(|_| PreparedSpawnError::Capacity(PopulationCapacity::Allocation))?;
        validate_ordinals(base, schedule).map_err(PreparedSpawnError::Build)?;
        let maximum = crate::limits::HIT_DICE as u64;
        let requested = base
            .checked_add(additional)
            .ok_or(PreparedSpawnError::Capacity(PopulationCapacity::Arithmetic))?;
        if requested > maximum {
            return Err(PreparedSpawnError::Capacity(PopulationCapacity::HitDice {
                requested,
                maximum,
            }));
        }
        // Saved ownership may legitimately contain dormant talents, but a new
        // scheduled purchase must be eligible at its actual acquisition ordinal.
        // Only scheduled rows incur this bounded preparation/birth validation;
        // empty growth retains the single bulk reconstruction below.
        if schedule
            .first()
            .is_some_and(|entry| entry.ordinal.get() <= requested)
        {
            let mut eligible = self.clone();
            for entry in schedule
                .iter()
                .take_while(|entry| entry.ordinal.get() <= requested)
            {
                let previous = eligible.ledger().total_hd().get();
                eligible =
                    eligible.instantiate_seeded_racial(seed, entry.ordinal.get() - previous)?;
                let owner = crate::HdIndex::from_ordinal(entry.ordinal)
                    .ok_or(PreparedSpawnError::Build(BuildError::InvalidOwnership))?;
                if let Some(attribute) = entry.choices.attribute {
                    eligible
                        .increase_attribute(owner, attribute)
                        .map_err(PreparedSpawnError::Build)?;
                }
                for purchase in &entry.choices.purchases {
                    eligible
                        .purchase(owner, purchase.pool.clone(), purchase.selection.clone())
                        .map_err(PreparedSpawnError::Build)?;
                }
            }
        }
        if additional > 0 {
            DepthGrowth::RacialAfter { depth: 1 }
                .validate_build(Some(self))
                .map_err(PreparedSpawnError::Build)?;
        }
        let count = usize::try_from(requested)
            .map_err(|_| PreparedSpawnError::Capacity(PopulationCapacity::Allocation))?;
        let mut sources = Vec::new();
        sources
            .try_reserve_exact(count)
            .map_err(|_| PreparedSpawnError::Capacity(PopulationCapacity::Allocation))?;
        sources.extend(self.ledger().entries().iter().map(|row| row.source()));
        sources.resize(count, HdSource::Racial);
        let ledger = HdLedger::seeded(sources, seed)
            .map_err(|error| PreparedSpawnError::Build(BuildError::from(error)))?;
        let mut advancement = self.advancement().clone();
        let extra = count
            .checked_sub(advancement.choices.len())
            .ok_or(PreparedSpawnError::Build(BuildError::InvalidOwnership))?;
        advancement
            .choices
            .try_reserve_exact(extra)
            .map_err(|_| PreparedSpawnError::Capacity(PopulationCapacity::Allocation))?;
        advancement.choices.resize(count, HdChoices::default());
        if additional > 0 {
            if let RestorationPolicy::Authored(plan) = &mut advancement.restoration {
                plan.try_reserve_exact(extra)
                    .map_err(|_| PreparedSpawnError::Capacity(PopulationCapacity::Allocation))?;
                plan.resize(
                    count,
                    PlannedAdvancement {
                        source: HdSource::Racial,
                        choices: HdChoices::default(),
                    },
                );
            }
        }
        for entry in schedule
            .iter()
            .take_while(|entry| entry.ordinal.get() <= requested)
        {
            let index = usize::try_from(entry.ordinal.get() - 1)
                .map_err(|_| PreparedSpawnError::Capacity(PopulationCapacity::Allocation))?;
            advancement.choices[index] = entry.choices.clone();
            if let RestorationPolicy::Authored(plan) = &mut advancement.restoration {
                plan[index].choices = entry.choices.clone();
            }
        }
        let mut result = Self::from_recorded(
            self.content().clone(),
            self.species_id().clone(),
            self.initial_attributes(),
            ledger,
            self.binding(),
            advancement,
            self.template_ids().to_vec(),
        )
        .map_err(|error| match error {
            BuildError::DerivedLimit => {
                PreparedSpawnError::Capacity(PopulationCapacity::DerivedSheet)
            }
            BuildError::Arithmetic => PreparedSpawnError::Capacity(PopulationCapacity::Arithmetic),
            BuildError::Allocation => PreparedSpawnError::Capacity(PopulationCapacity::Allocation),
            error => PreparedSpawnError::Build(error),
        })?;
        result
            .set_future_growth(self.future_growth().clone())
            .map_err(PreparedSpawnError::Build)?;
        Ok(result)
    }
}

impl PreparedActor {
    pub fn validate_depth_growth(
        &self,
        content: std::sync::Arc<crate::creatures::Content>,
        growth: DepthGrowth,
    ) -> Result<(), BuildError> {
        let build = self
            .build
            .as_ref()
            .map(|record| record.restore(content))
            .transpose()?;
        growth.validate_build(build.as_ref())
    }

    pub fn validate_birth_advancement(
        &self,
        content: std::sync::Arc<crate::creatures::Content>,
        growth: DepthGrowth,
        schedule: &[OrdinalAdvancement],
    ) -> Result<(), BuildError> {
        if schedule.is_empty() {
            return self.validate_depth_growth(content, growth);
        }
        if matches!(growth, DepthGrowth::Fixed {}) {
            return Err(BuildError::InvalidDefinition);
        }
        let build = self
            .build
            .as_ref()
            .ok_or(BuildError::InvalidDefinition)?
            .restore(content)?;
        growth.validate_build(Some(&build))?;
        let base = build.ledger().entries().len() as u64;
        validate_ordinals(base, schedule)?;
        let last = schedule
            .last()
            .ok_or(BuildError::InvalidDefinition)?
            .ordinal
            .get();
        build
            .instantiate_seeded_racial_with_advancement(0, last - base, schedule)
            .map(|_| ())
            .map_err(PreparedSpawnError::into_build_error)
    }

    /// Complete checked construction before any actor identity is allocated.
    pub(crate) fn instantiate_build(
        &self,
        content: std::sync::Arc<crate::creatures::Content>,
        seed: u64,
        growth: DepthGrowth,
        depth: u32,
        schedule: &[OrdinalAdvancement],
    ) -> Result<Option<CreatureBuild>, PreparedSpawnError> {
        let prototype = self
            .build
            .as_ref()
            .map(|record| record.restore(content))
            .transpose()
            .map_err(PreparedSpawnError::Build)?;
        growth
            .validate_build(prototype.as_ref())
            .map_err(PreparedSpawnError::Build)?;
        prototype
            .map(|build| {
                build.instantiate_seeded_racial_with_advancement(
                    seed,
                    growth.additional(depth),
                    schedule,
                )
            })
            .transpose()
    }

    pub(crate) fn valid(&self, game: &Game) -> bool {
        self.build.is_some() == self.identity.is_some()
            && (self.nutrition == crate::nutrition::NutritionRequirement::Exempt
                || (self.build.is_some()
                    && crate::nutrition::Nutrition::initial(
                        self.nutrition,
                        game.content.rules().document().nutrition,
                    )
                    .is_some()))
            && !(self.build.is_some() && self.anatomy.is_some())
            && self.identity.as_ref().is_none_or(CreatureIdentity::valid)
            && self
                .build
                .as_ref()
                .is_none_or(|build| build.restore(game.content.clone()).is_ok())
            && self
                .anatomy
                .as_ref()
                .is_none_or(|a| a.valid(game.content.rules()))
            && self.body.as_ref().is_none_or(BodySpec::valid)
            && self
                .ai
                .as_ref()
                .is_none_or(|ai| self.build.is_some() && ai.valid())
            && self.known_identities.len() <= 4096
            && self
                .known_identities
                .iter()
                .all(|s| !s.is_empty() && s.len() <= 80 && !s.chars().any(char::is_control))
            && self.velocity.is_none_or(|v| {
                v.iter()
                    .all(|n| n.unsigned_abs() <= crate::physics::TERMINAL as u64)
            })
            && self
                .asset
                .as_ref()
                .is_none_or(|s| !s.is_empty() && s.len() <= 128 && !s.chars().any(char::is_control))
    }
}

impl Game {
    /// Install into a transaction candidate through the same ordinary constructors
    /// as authored actors. The caller owns publication/rollback of the candidate.
    pub fn configure_prepared_actor(
        &mut self,
        id: ActorId,
        definition: &PreparedActor,
        seed: u64,
    ) -> Result<(), PreparedSpawnError> {
        let build = definition.instantiate_build(
            self.content.clone(),
            seed,
            DepthGrowth::Fixed {},
            1,
            &[],
        )?;
        self.configure_prepared_actor_build(id, definition, build)
    }

    pub(crate) fn configure_prepared_actor_build(
        &mut self,
        id: ActorId,
        definition: &PreparedActor,
        build: Option<CreatureBuild>,
    ) -> Result<(), PreparedSpawnError> {
        if let Some(anatomy) = &definition.anatomy {
            self.configure_anatomy(id, anatomy.clone())
                .map_err(PreparedSpawnError::Anatomy)?;
        }
        for identity in &definition.known_identities {
            self.learn_identity(id, identity)
                .map_err(PreparedSpawnError::Knowledge)?;
        }
        if let Some(build) = build {
            let identity = definition
                .identity
                .clone()
                .ok_or(PreparedSpawnError::Build(BuildError::InvalidDefinition))?;
            self.configure_creature_in_transaction(id, identity, build)
                .map_err(PreparedSpawnError::Creature)?;
            self.configure_nutrition_in_transaction(id, definition.nutrition)
                .map_err(PreparedSpawnError::Nutrition)?;
        }
        if let Some(ai) = &definition.ai {
            self.configure_ai(id, ai.clone())
                .map_err(PreparedSpawnError::Ai)?;
        }
        if let Some(body) = &definition.body {
            self.set_body_in_transaction(id, body.clone())
                .map_err(PreparedSpawnError::Body)?;
        }
        if let Some(velocity) = definition.velocity {
            self.set_actor_velocity(id, velocity)
                .map_err(PreparedSpawnError::Velocity)?;
        }
        if let Some(asset) = &definition.asset {
            self.set_actor_asset(id, Some(asset.clone()))
                .map_err(PreparedSpawnError::Asset)?;
        }
        Ok(())
    }

    /// The enclosing entry owns rollback. No public actor-only intermediate
    /// escapes this helper and no Game clone is needed for each birth.
    pub(crate) fn spawn_prepared_actor_in_candidate(
        &mut self,
        definition: &PreparedActor,
        at: Location,
        turn_ticks: std::num::NonZeroU64,
        build: Option<CreatureBuild>,
        carried: Option<(u64, &ItemSpec)>,
    ) -> Result<(ActorId, Option<ItemId>), GameError> {
        let actor = self.spawn_actor(at, turn_ticks)?;
        self.configure_prepared_actor_build(actor, definition, build)?;
        let item = carried
            .map(|(quantity, spec)| self.spawn_item_stack(at, Some(actor), quantity, spec.clone()))
            .transpose()?;
        Ok((actor, item))
    }

    /// Trusted host placement of a standalone catalog build. The caller provides
    /// the ordinary actor-seed derivation; there is no population AI or stock.
    pub fn spawn_standalone_creature(
        &mut self,
        definition: &PreparedActor,
        at: Location,
        turn_ticks: std::num::NonZeroU64,
        health_seed: impl FnOnce(u64, u64) -> u64,
    ) -> Result<ActorId, GameError> {
        if definition.build.is_none() || definition.ai.is_some() {
            return Err(GameError::InvalidLocation);
        }
        let seed = health_seed(self.seed, self.next_actor_id);
        let build = definition
            .instantiate_build(self.content.clone(), seed, DepthGrowth::Fixed {}, 1, &[])
            .map_err(GameError::from)?;
        let mut candidate = self.clone();
        candidate.begin_effect_publication()?;
        let (actor, _) =
            candidate.spawn_prepared_actor_in_candidate(definition, at, turn_ticks, build, None)?;
        candidate.finish_effect_publication()?;
        *self = candidate;
        Ok(actor)
    }

    pub fn validate_standalone_creature_birth(
        &self,
        definition: &PreparedActor,
        actor: ActorId,
        health_seed: impl FnOnce(u64, u64) -> u64,
    ) -> Result<(), GameError> {
        if actor.0 == 0
            || actor.0 >= self.next_actor_id
            || definition.build.is_none()
            || definition.ai.is_some()
        {
            return Err(GameError::InvalidLocation);
        }
        definition
            .instantiate_build(
                self.content.clone(),
                health_seed(self.seed, actor.0),
                DepthGrowth::Fixed {},
                1,
                &[],
            )
            .map_err(GameError::from)?;
        Ok(())
    }

    /// Allocate from the ordinary item namespace, retaining its checked ownership,
    /// quantity, definition and lifecycle validation. No secondary allocator.
    pub fn spawn_item_stack(
        &mut self,
        at: Location,
        owner: Option<ActorId>,
        quantity: u64,
        spec: ItemSpec,
    ) -> Result<ItemId, GameError> {
        let id = self.next_item_id;
        self.place_item_stack(id, at, owner, quantity, spec)
    }
}

#[cfg(test)]
mod capacity_tests {
    use super::*;
    #[test]
    fn seeded_build_adapter_preserves_capacity_origin() {
        for (capacity, expected) in [
            (
                PopulationCapacity::HitDice {
                    requested: 257,
                    maximum: 256,
                },
                BuildError::TooManyHitDice,
            ),
            (PopulationCapacity::Arithmetic, BuildError::Arithmetic),
            (PopulationCapacity::Allocation, BuildError::Allocation),
            (PopulationCapacity::DerivedSheet, BuildError::DerivedLimit),
        ] {
            assert_eq!(
                PreparedSpawnError::Capacity(capacity).into_build_error(),
                expected
            );
            assert_eq!(
                GameError::from(PreparedSpawnError::Capacity(capacity)),
                GameError::PopulationCapacity(capacity)
            );
        }
        assert_eq!(
            PreparedSpawnError::Build(BuildError::InvalidInteraction).into_build_error(),
            BuildError::InvalidInteraction
        );
    }
    #[test]
    fn racial_birth_arithmetic_refusal_keeps_the_prototype_exact() {
        let base = crate::test_creatures::build(crate::test_creatures::species());
        let before = base.clone();
        assert_eq!(
            base.instantiate_seeded_racial(42, u64::MAX),
            Err(PreparedSpawnError::Capacity(PopulationCapacity::Arithmetic))
        );
        assert_eq!(base, before);
    }
}
