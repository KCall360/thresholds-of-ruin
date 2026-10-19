//! Strict mutable creature records. Definitions live once in the save catalog.
mod intellect;
use super::{
    AdvancementPurchase, BuildAdvancement, BuildEpoch, BuildError, Content, CreatureBuild,
    HdChoices, PlannedAdvancement, PurchaseSelection, RestorationPolicy,
};
use crate::attributes::{Attribute, Attributes, ByAttribute, ManaBinding, Skill};
use crate::bounded::{named_map, required_option, Bounded};
use crate::content_ids::{
    AbilityId, ClassId, ExpertiseId, FormId, SpeciesId, TalentId, TemplateId,
};
use crate::limits;
use crate::progression::{HdLedger, HdSource, HitDie};
use crate::resources::{ByResource, Resource, Resources};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CreatureRecord {
    #[serde(deserialize_with = "named_map")]
    build: BuildRecord,
    build_epoch: BuildEpoch,
    earned_progress: crate::earned_growth::EarnedProgress,
    restorable_losses: super::RestorableLosses,
    injury: u64,
    dead: bool,
    #[serde(deserialize_with = "named_map")]
    health_recovery: HealthRecoveryRecord,
    nutrition: crate::nutrition::Nutrition,
    received_health_loss_remaining: u64,
    pools: ByResource<PoolRecord>,
    holds: Bounded<HoldRecord, { limits::COST_HOLDS }>,
    conditions: Bounded<crate::conditions::ConditionInstance, { limits::CONDITIONS }>,
    next_condition_generation: u64,
    cooldowns: Bounded<CooldownRecord, { limits::CATALOG_ENTRIES }>,
}
impl<'de> serde::Deserialize<'de> for CreatureRecord {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            #[serde(deserialize_with = "named_map")]
            build: BuildRecord,
            build_epoch: BuildEpoch,
            earned_progress: crate::earned_growth::EarnedProgress,
            restorable_losses: super::RestorableLosses,
            injury: u64,
            dead: bool,
            #[serde(deserialize_with = "named_map")]
            health_recovery: HealthRecoveryRecord,
            nutrition: crate::nutrition::Nutrition,
            received_health_loss_remaining: u64,
            pools: ByResource<PoolRecord>,
            holds: Bounded<HoldRecord, { limits::COST_HOLDS }>,
            conditions: Bounded<crate::conditions::ConditionInstance, { limits::CONDITIONS }>,
            next_condition_generation: u64,
            cooldowns: Bounded<CooldownRecord, { limits::CATALOG_ENTRIES }>,
        }
        let fields: Fields = crate::bounded::named_map(deserializer)?;
        Ok(Self {
            build: fields.build,
            build_epoch: fields.build_epoch,
            earned_progress: fields.earned_progress,
            restorable_losses: fields.restorable_losses,
            injury: fields.injury,
            dead: fields.dead,
            health_recovery: fields.health_recovery,
            nutrition: fields.nutrition,
            received_health_loss_remaining: fields.received_health_loss_remaining,
            pools: fields.pools,
            holds: fields.holds,
            conditions: fields.conditions,
            next_condition_generation: fields.next_condition_generation,
            cooldowns: fields.cooldowns,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
struct CooldownRecord {
    ability: AbilityId,
    remaining: u64,
}
impl<'de> Deserialize<'de> for CooldownRecord {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            ability: AbilityId,
            remaining: u64,
        }
        let fields: Fields = named_map(deserializer)?;
        Ok(Self {
            ability: fields.ability,
            remaining: fields.remaining,
        })
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
struct HealthRecoveryRecord {
    elapsed: u64,
    nutrition: crate::health_recovery::RecoveryNutrition,
    context: RecoveryContextRecord,
}

/// Persistence owns its strict named encoding independently of live resolver facts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum RecoveryContextRecord {
    Known { sensed_hostile: bool },
    Pending { base_period: u64 },
}
impl<'de> Deserialize<'de> for RecoveryContextRecord {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            Known { sensed_hostile: bool },
            Pending { base_period: u64 },
        }
        match named_map(deserializer)? {
            Fields::Known { sensed_hostile } => Ok(Self::Known { sensed_hostile }),
            Fields::Pending { base_period }
                if (1..=limits::NUMERIC as u64).contains(&base_period) =>
            {
                Ok(Self::Pending { base_period })
            }
            Fields::Pending { .. } => Err(serde::de::Error::custom(
                "invalid pending recovery denominator",
            )),
        }
    }
}
impl From<crate::health_recovery::RecoveryContext> for RecoveryContextRecord {
    fn from(context: crate::health_recovery::RecoveryContext) -> Self {
        match context {
            crate::health_recovery::RecoveryContext::Known { sensed_hostile } => {
                Self::Known { sensed_hostile }
            }
            crate::health_recovery::RecoveryContext::Pending { base_period } => {
                Self::Pending { base_period }
            }
        }
    }
}
impl From<RecoveryContextRecord> for crate::health_recovery::RecoveryContext {
    fn from(context: RecoveryContextRecord) -> Self {
        match context {
            RecoveryContextRecord::Known { sensed_hostile } => Self::Known { sensed_hostile },
            RecoveryContextRecord::Pending { base_period } => Self::Pending { base_period },
        }
    }
}
impl<'de> Deserialize<'de> for HealthRecoveryRecord {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            elapsed: u64,
            nutrition: crate::health_recovery::RecoveryNutrition,
            context: RecoveryContextRecord,
        }
        let fields: Fields = named_map(deserializer)?;
        Ok(Self {
            elapsed: fields.elapsed,
            nutrition: fields.nutrition,
            context: fields.context,
        })
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PoolRecord {
    balance: u32,
    recovery_elapsed: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct HoldRecord {
    owner: u64,
    #[serde(deserialize_with = "named_map")]
    cost: CostBundleRecord,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CostPhaseRecord {
    start: u32,
    resolution: u32,
}
fn read_cost_phase<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Option<CostPhaseRecord>, D::Error> {
    #[derive(Deserialize)]
    struct Named(#[serde(deserialize_with = "named_map")] CostPhaseRecord);
    required_option::<Named, D>(d).map(|value| value.map(|named| named.0))
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CostBundleRecord {
    #[serde(deserialize_with = "read_cost_phase")]
    stamina: Option<CostPhaseRecord>,
    #[serde(deserialize_with = "read_cost_phase")]
    focus: Option<CostPhaseRecord>,
    #[serde(deserialize_with = "read_cost_phase")]
    mana: Option<CostPhaseRecord>,
}
impl CostBundleRecord {
    fn capture(bundle: crate::costs::CostBundle) -> Self {
        let mut phases = ByResource::from_fn(|_| None);
        for cost in bundle.entries() {
            *phases.get_mut(cost.resource) = Some(CostPhaseRecord {
                start: cost.start,
                resolution: cost.resolution,
            });
        }
        Self {
            stamina: phases.stamina,
            focus: phases.focus,
            mana: phases.mana,
        }
    }
    fn restore(&self) -> Result<crate::costs::CostBundle, crate::costs::CostError> {
        let mut bundle: Option<crate::costs::CostBundle> = None;
        for (resource, phases) in [
            (Resource::Stamina, self.stamina),
            (Resource::Focus, self.focus),
            (Resource::Mana, self.mana),
        ] {
            if let Some(phases) = phases {
                let next = crate::costs::CostBundle::single(crate::costs::ResourceCost {
                    resource,
                    start: phases.start,
                    resolution: phases.resolution,
                })?;
                bundle = Some(match bundle {
                    Some(existing) => existing.checked_add(next)?,
                    None => next,
                });
            }
        }
        bundle.ok_or(crate::costs::CostError::Invalid)
    }
}

impl CreatureRecord {
    pub fn capture(creature: &super::CreatureState) -> Self {
        let resources = creature.costs().resources();
        let pool = |resource| PoolRecord {
            balance: resources.balance(resource),
            recovery_elapsed: resources.recovery_elapsed(resource),
        };
        Self {
            build: BuildRecord::capture(creature.build()),
            build_epoch: creature.build_epoch(),
            earned_progress: creature.earned_progress().clone(),
            restorable_losses: creature.restorable_losses(),
            injury: creature.health().injury(),
            dead: creature.health().dead(),
            health_recovery: HealthRecoveryRecord {
                elapsed: creature.health_recovery().elapsed(),
                nutrition: creature.health_recovery().nutrition(),
                context: creature.recovery_context().into(),
            },
            nutrition: creature.nutrition(),
            received_health_loss_remaining: creature.received_health_loss_remaining(),
            pools: ByResource {
                stamina: pool(Resource::Stamina),
                focus: pool(Resource::Focus),
                mana: pool(Resource::Mana),
            },
            holds: Bounded(
                creature
                    .costs()
                    .reservations()
                    .iter()
                    .map(|(id, cost)| HoldRecord {
                        owner: id.0,
                        cost: CostBundleRecord::capture(*cost),
                    })
                    .collect(),
            ),
            conditions: Bounded(creature.conditions().instances().cloned().collect()),
            next_condition_generation: creature.conditions().next_generation(),
            cooldowns: Bounded(
                creature
                    .cooldowns()
                    .entries()
                    .map(|(ability, remaining)| CooldownRecord {
                        ability: ability.clone(),
                        remaining: remaining.get(),
                    })
                    .collect(),
            ),
        }
    }
    pub fn restore(
        &self,
        content: Arc<Content>,
        equipment: Vec<super::EquipmentSource>,
    ) -> Result<super::CreatureState, super::CreatureStateError> {
        use super::CreatureStateError as Error;
        let build = self.build.restore(content).map_err(Error::Build)?;
        let conditions = crate::conditions::ConditionState::from_recorded(
            self.conditions.0.clone(),
            self.next_condition_generation,
            build.ruleset(),
        )
        .map_err(Error::Condition)?;
        let mut creature = super::CreatureState::from_recorded_with_costs(
            build,
            self.build_epoch,
            self.restorable_losses,
            self.injury,
            self.dead,
            conditions,
            equipment,
            |maxima, rules| {
                let pools = ByResource {
                    stamina: &self.pools.stamina,
                    focus: &self.pools.focus,
                    mana: &self.pools.mana,
                };
                let resources = Resources::from_recorded(
                    maxima,
                    pools.map(|pool| pool.balance),
                    pools.map(|pool| pool.recovery_elapsed),
                    rules,
                )
                .map_err(Error::Resource)?;
                let holds = self
                    .holds
                    .0
                    .iter()
                    .map(|hold| Ok((crate::IntentionId(hold.owner), hold.cost.restore()?)))
                    .collect::<Result<Vec<_>, crate::costs::CostError>>()
                    .map_err(Error::Cost)?;
                crate::costs::CostLedger::from_recorded(resources, holds).map_err(Error::Cost)
            },
        )?;
        creature.restore_health_recovery(
            self.health_recovery.elapsed,
            self.health_recovery.nutrition,
            self.health_recovery.context.into(),
        )?;
        creature.restore_nutrition(self.nutrition)?;
        creature.restore_received_health_loss(self.received_health_loss_remaining)?;
        creature.replace_earned_progress(self.earned_progress.clone())?;
        creature.restore_cooldowns(
            self.cooldowns
                .0
                .iter()
                .map(|row| (row.ability.clone(), row.remaining))
                .collect(),
        )?;
        Ok(creature)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuildRecord {
    species: SpeciesId,
    future_growth: crate::earned_growth::FutureGrowth,
    starting_skills: [Skill; 2],
    restoration: RestorationRecord,
    initial_attributes: ByAttribute<u16>,
    intellect_learning: intellect::Learning,
    binding: BindingRecord,
    #[serde(deserialize_with = "required_option")]
    health_root_seed: Option<u64>,
    hd: Bounded<HdRecord, { limits::HIT_DICE }>,
    drained_hd: Bounded<DrainedHdRecord, { limits::HIT_DICE }>,
    choices: Bounded<ChoiceRecord, { limits::HIT_DICE }>,
    templates: Bounded<TemplateId, { limits::TEMPLATES }>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum BindingRecord {
    Intellect,
    Willpower,
    Awareness,
    Presence,
}
impl BindingRecord {
    fn capture(value: ManaBinding) -> Self {
        match value {
            ManaBinding::Intellect => Self::Intellect,
            ManaBinding::Willpower => Self::Willpower,
            ManaBinding::Awareness => Self::Awareness,
            ManaBinding::Presence => Self::Presence,
        }
    }
    fn restore(self) -> ManaBinding {
        match self {
            Self::Intellect => ManaBinding::Intellect,
            Self::Willpower => ManaBinding::Willpower,
            Self::Awareness => ManaBinding::Awareness,
            Self::Presence => ManaBinding::Presence,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "id",
    rename_all = "snake_case",
    deny_unknown_fields
)]
enum SourceRecord {
    Racial,
    Class(ClassId),
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
struct HdRecord {
    #[serde(deserialize_with = "named_map")]
    source: SourceRecord,
    health_seed: u64,
    #[serde(deserialize_with = "named_map")]
    health_origin: HealthOriginRecord,
    health_improvement: u16,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct HealthOriginRecord {
    templates: Bounded<TemplateId, { limits::TEMPLATES }>,
    kind: crate::content_ids::TypeId,
    die: u16,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
struct DrainedHdRecord {
    ordinal: crate::HdIndex,
    #[serde(deserialize_with = "named_map")]
    hit_die: HdRecord,
}
impl<'de> Deserialize<'de> for HdRecord {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            #[serde(deserialize_with = "named_map")]
            source: SourceRecord,
            health_seed: u64,
            #[serde(deserialize_with = "named_map")]
            health_origin: HealthOriginRecord,
            health_improvement: u16,
        }
        let fields: Fields = named_map(deserializer)?;
        Ok(Self {
            source: fields.source,
            health_seed: fields.health_seed,
            health_origin: fields.health_origin,
            health_improvement: fields.health_improvement,
        })
    }
}
impl<'de> Deserialize<'de> for DrainedHdRecord {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            ordinal: crate::HdIndex,
            hit_die: HdRecord,
        }
        let fields: Fields = named_map(deserializer)?;
        Ok(Self {
            ordinal: fields.ordinal,
            hit_die: fields.hit_die,
        })
    }
}
impl HdRecord {
    fn capture(die: &HitDie) -> Self {
        let origin = die
            .health_origin()
            .expect("owned build binds every original Health die");
        Self {
            source: SourceRecord::capture(die.source()),
            health_seed: die.health_seed(),
            health_origin: HealthOriginRecord {
                templates: Bounded(origin.templates.clone()),
                kind: origin.kind.clone(),
                die: origin.die,
            },
            health_improvement: die.health_improvement(),
        }
    }
    fn restore(&self, ordinal: usize) -> Result<HitDie, crate::progression::HdError> {
        HitDie::from_recorded(
            self.source.restore(),
            self.health_seed,
            crate::progression::HealthOrigin {
                templates: self.health_origin.templates.0.clone(),
                kind: self.health_origin.kind.clone(),
                die: self.health_origin.die,
            },
            self.health_improvement,
            ordinal,
        )
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ChoiceRecord {
    purchases: Bounded<PurchaseRecord, { limits::POINTS_PER_HIT_DIE }>,
    #[serde(deserialize_with = "required_option")]
    attribute: Option<Attribute>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
struct PurchaseRecord {
    pool: crate::content_ids::PointPoolId,
    selection: SelectionRecord,
}
impl<'de> Deserialize<'de> for PurchaseRecord {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            pool: crate::content_ids::PointPoolId,
            selection: SelectionRecord,
        }
        let fields: Fields = named_map(deserializer)?;
        Ok(Self {
            pool: fields.pool,
            selection: fields.selection,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum SelectionRecord {
    SkillRank { skill: Skill },
    Expertise { expertise: ExpertiseId },
    Talent { talent: TalentId },
    TalentForm { talent: TalentId, form: FormId },
}
impl<'de> Deserialize<'de> for SelectionRecord {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            SkillRank { skill: Skill },
            Expertise { expertise: ExpertiseId },
            Talent { talent: TalentId },
            TalentForm { talent: TalentId, form: FormId },
        }
        Ok(match named_map(deserializer)? {
            Fields::SkillRank { skill } => Self::SkillRank { skill },
            Fields::Expertise { expertise } => Self::Expertise { expertise },
            Fields::Talent { talent } => Self::Talent { talent },
            Fields::TalentForm { talent, form } => Self::TalentForm { talent, form },
        })
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum RestorationRecord {
    ChooseAgain {},
    Authored {
        entries: Bounded<PlannedRecord, { limits::HIT_DICE }>,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PlannedRecord {
    source: SourceRecord,
    choices: ChoiceRecord,
}
impl SourceRecord {
    fn capture(source: HdSource) -> Self {
        match source {
            HdSource::Racial => Self::Racial,
            HdSource::Class(id) => Self::Class(id),
        }
    }
    fn restore(&self) -> HdSource {
        match self {
            Self::Racial => HdSource::Racial,
            Self::Class(id) => HdSource::Class(id.clone()),
        }
    }
}
impl ChoiceRecord {
    pub(crate) fn capture(choice: &HdChoices) -> Self {
        Self {
            attribute: choice.attribute,
            purchases: Bounded(
                choice
                    .purchases
                    .iter()
                    .map(|purchase| PurchaseRecord {
                        pool: purchase.pool.clone(),
                        selection: match &purchase.selection {
                            PurchaseSelection::SkillRank(skill) => {
                                SelectionRecord::SkillRank { skill: *skill }
                            }
                            PurchaseSelection::Expertise(expertise) => SelectionRecord::Expertise {
                                expertise: expertise.clone(),
                            },
                            PurchaseSelection::Talent(talent) => SelectionRecord::Talent {
                                talent: talent.clone(),
                            },
                            PurchaseSelection::TalentForm { talent, form } => {
                                SelectionRecord::TalentForm {
                                    talent: talent.clone(),
                                    form: form.clone(),
                                }
                            }
                        },
                    })
                    .collect(),
            ),
        }
    }
    pub(crate) fn restore(&self) -> HdChoices {
        HdChoices {
            attribute: self.attribute,
            purchases: self
                .purchases
                .0
                .iter()
                .map(|purchase| AdvancementPurchase {
                    pool: purchase.pool.clone(),
                    selection: match &purchase.selection {
                        SelectionRecord::SkillRank { skill } => {
                            PurchaseSelection::SkillRank(*skill)
                        }
                        SelectionRecord::Expertise { expertise } => {
                            PurchaseSelection::Expertise(expertise.clone())
                        }
                        SelectionRecord::Talent { talent } => {
                            PurchaseSelection::Talent(talent.clone())
                        }
                        SelectionRecord::TalentForm { talent, form } => {
                            PurchaseSelection::TalentForm {
                                talent: talent.clone(),
                                form: form.clone(),
                            }
                        }
                    },
                })
                .collect(),
        }
    }
}
impl BuildRecord {
    pub fn capture(build: &CreatureBuild) -> Self {
        Self {
            species: build.species().id.clone(),
            future_growth: build.future_growth().clone(),
            starting_skills: *build.starting_skills(),
            restoration: match build.restoration_policy() {
                RestorationPolicy::ChooseAgain => RestorationRecord::ChooseAgain {},
                RestorationPolicy::Authored(entries) => RestorationRecord::Authored {
                    entries: Bounded(
                        entries
                            .iter()
                            .map(|entry| PlannedRecord {
                                source: SourceRecord::capture(entry.source.clone()),
                                choices: ChoiceRecord::capture(&entry.choices),
                            })
                            .collect(),
                    ),
                },
            },
            intellect_learning: intellect::Learning::capture(
                build
                    .intellect_learning
                    .as_ref()
                    .expect("validated birth learning"),
            ),
            initial_attributes: ByAttribute::from_fn(|attribute| {
                build.initial_attributes().get(attribute)
            }),
            binding: BindingRecord::capture(build.binding()),
            health_root_seed: build.ledger().health_root_seed(),
            hd: Bounded(
                build
                    .ledger()
                    .entries()
                    .iter()
                    .map(HdRecord::capture)
                    .collect(),
            ),
            drained_hd: Bounded(
                build
                    .ledger()
                    .drained()
                    .map(|(ordinal, die)| DrainedHdRecord {
                        ordinal: crate::HdIndex(ordinal as u64),
                        hit_die: HdRecord::capture(die),
                    })
                    .collect(),
            ),
            choices: Bounded(build.choices().iter().map(ChoiceRecord::capture).collect()),
            templates: Bounded(build.template_ids().to_vec()),
        }
    }
    pub fn restore(&self, content: Arc<Content>) -> Result<CreatureBuild, BuildError> {
        let mut ledger = HdLedger::from_recorded(
            self.hd
                .0
                .iter()
                .enumerate()
                .map(|(ordinal, die)| die.restore(ordinal))
                .collect::<Result<Vec<_>, _>>()
                .map_err(BuildError::from)?,
            self.health_root_seed,
        )
        .map_err(BuildError::from)?;
        let mut previous = None;
        let drained = self
            .drained_hd
            .0
            .iter()
            .map(|row| {
                let ordinal = usize::try_from(row.ordinal.get())
                    .map_err(|_| BuildError::InvalidDefinition)?;
                if previous.is_some_and(|previous| ordinal <= previous) {
                    return Err(BuildError::InvalidDefinition);
                }
                previous = Some(ordinal);
                Ok((
                    ordinal,
                    row.hit_die.restore(ordinal).map_err(BuildError::from)?,
                ))
            })
            .collect::<Result<Vec<_>, BuildError>>()?;
        ledger.restore_drained(drained).map_err(BuildError::from)?;
        let choices = self.choices.0.iter().map(ChoiceRecord::restore).collect();
        let mut build = CreatureBuild::from_recorded(
            content,
            self.species.clone(),
            Attributes::new(self.initial_attributes).map_err(|_| BuildError::InvalidDefinition)?,
            ledger,
            self.binding.restore(),
            BuildAdvancement {
                starting_skills: self.starting_skills,
                choices,
                restoration: match &self.restoration {
                    RestorationRecord::ChooseAgain {} => RestorationPolicy::ChooseAgain,
                    RestorationRecord::Authored { entries } => RestorationPolicy::Authored(
                        entries
                            .0
                            .iter()
                            .map(|entry| PlannedAdvancement {
                                source: entry.source.restore(),
                                choices: entry.choices.restore(),
                            })
                            .collect(),
                    ),
                },
            },
            self.templates.0.clone(),
        )?;
        build.intellect_learning = Some(Arc::new(self.intellect_learning.restore(&build)?));
        build.derive()?;
        build.set_future_growth(self.future_growth.clone())?;
        Ok(build)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::creatures::{CreatureState, EquipmentSource};
    use crate::rules::{Modifier, ModifierOp, Scope, Stat};

    fn subject(equipped: bool, conditioned: bool) -> (CreatureState, Vec<EquipmentSource>) {
        let mut creature = CreatureState::new(crate::test_creatures::build(
            crate::test_creatures::species(),
        ))
        .unwrap();
        let equipment = if equipped {
            vec![EquipmentSource {
                weapon_profile: None,
                item: crate::ItemId(1),
                socket: crate::EquipmentSlotId(0),
                modifiers: vec![Modifier {
                    scope: Scope::Always,
                    op: ModifierOp::Add {
                        stat: Stat::ResourceMax {
                            resource: Resource::Stamina,
                        },
                        value: 4,
                    },
                }],
                attack: Some(crate::test_creatures::melee(
                    7,
                    11,
                    crate::combat::DamageType::Keen,
                    3,
                )),
            }]
        } else {
            vec![]
        };
        creature.set_equipment(equipment.clone()).unwrap();
        if conditioned {
            creature
                .apply_condition(
                    &crate::content_ids::ConditionId::new("fear").unwrap(),
                    crate::ActorId(7),
                    40,
                )
                .unwrap();
        }
        creature.damage(2);
        creature
            .start_cost(
                crate::IntentionId(1),
                crate::costs::CostBundle::single(crate::costs::ResourceCost {
                    resource: Resource::Stamina,
                    start: 1,
                    resolution: 1,
                })
                .unwrap(),
            )
            .unwrap();
        creature.advance_active(3);
        (creature, equipment)
    }

    #[test]
    fn empty_condition_restart_preserves_generation_and_rejects_expired_token_candidate() {
        let (mut original, equipment) = subject(false, false);
        let id = crate::content_ids::ConditionId::new("readied").unwrap();
        let rules = original.build().ruleset().clone();
        original
            .apply_condition(&id, crate::ActorId(7), 40)
            .unwrap();
        let stale = original
            .conditions()
            .targetless_attack_tokens(
                &rules,
                crate::ActorId(1),
                &AbilityId::new("basic_melee").unwrap(),
            )
            .pop()
            .unwrap();
        original.advance_active(40);
        assert!(original.conditions().is_empty());
        let record = CreatureRecord::capture(&original);
        assert_eq!(record.next_condition_generation, 1);
        let encoded = serde_json::to_vec(&record).unwrap();
        let decoded: CreatureRecord = serde_json::from_slice(&encoded).unwrap();
        let mut restored = decoded
            .restore(original.build().content().clone(), equipment)
            .unwrap();
        assert_eq!(restored, original);
        restored
            .apply_condition(&id, crate::ActorId(7), 40)
            .unwrap();
        let fresh = restored
            .conditions()
            .qualifying_tokens(&rules, crate::ActorId(1), false)
            .pop()
            .unwrap();
        assert_eq!(fresh.generation, 1);
        assert_eq!(fresh.order, stale.order);
        let before = restored.clone();
        assert_eq!(
            restored.reserve_tokens(std::slice::from_ref(&stale), crate::ActorId(2)),
            Err(crate::conditions::ConditionError::InvalidReservation)
        );
        assert_eq!(restored, before);
        restored
            .reserve_tokens(std::slice::from_ref(&fresh), crate::ActorId(2))
            .unwrap();
        let held = restored.clone();
        restored.consume_tokens(&[stale], crate::ActorId(2));
        assert_eq!(restored, held);
        restored.consume_tokens(&[fresh], crate::ActorId(2));
        assert!(restored.conditions().is_empty());
        assert_eq!(
            CreatureRecord::capture(&restored).next_condition_generation,
            2
        );
    }

    #[test]
    fn restoration_derives_each_source_set_once() {
        let mut folds = vec![];
        for (equipped, conditioned) in [(false, false), (true, false), (true, true)] {
            let (mut original, equipment) = subject(equipped, conditioned);
            let record = CreatureRecord::capture(&original);
            let before = crate::creatures::derive::fold_count();
            let mut restored = record
                .restore(original.build().content().clone(), equipment)
                .unwrap();
            folds.push(crate::creatures::derive::fold_count() - before);
            assert_eq!(restored, original);
            assert_eq!(CreatureRecord::capture(&restored), record);
            // Expiry must still return to the independently validated baseline.
            assert_eq!(restored.advance_active(50), original.advance_active(50));
            assert_eq!(restored, original);
        }
        // One permanent-build validation, one equipped baseline, and only a
        // conditioned actor needs a distinct current-source fold.
        assert_eq!(folds, [2, 2, 3]);
    }

    #[test]
    fn restored_resources_use_the_conditioned_fold_and_expire_to_the_baseline() {
        let (base, _) = subject(false, false);
        let mut document = base.build().ruleset().document().clone();
        document
            .conditions
            .iter_mut()
            .find(|c| c.id.as_str() == "fear")
            .unwrap()
            .modifiers
            .push(Modifier {
                scope: Scope::Always,
                op: ModifierOp::Add {
                    stat: Stat::ResourceMax {
                        resource: Resource::Stamina,
                    },
                    value: 6,
                },
            });
        let content = Content::new(
            crate::rules::Ruleset::validate(document).unwrap(),
            base.build().content().catalog().clone(),
        )
        .unwrap();
        let build = BuildRecord::capture(base.build())
            .restore(content.clone())
            .unwrap();
        let mut original = CreatureState::new(build).unwrap();
        let baseline = original.costs().resources().maximum(Resource::Stamina);
        original
            .apply_condition(
                &crate::content_ids::ConditionId::new("fear").unwrap(),
                crate::ActorId(7),
                40,
            )
            .unwrap();
        original.restore_resource(Resource::Stamina, u32::MAX);
        assert_eq!(
            original.costs().resources().balance(Resource::Stamina),
            baseline + 6
        );
        original
            .start_cost(
                crate::IntentionId(2),
                crate::costs::CostBundle::single(crate::costs::ResourceCost {
                    resource: Resource::Stamina,
                    start: 0,
                    resolution: baseline + 1,
                })
                .unwrap(),
            )
            .unwrap();
        let record = CreatureRecord::capture(&original);
        let mut restored = record.restore(content.clone(), vec![]).unwrap();
        assert_eq!(restored, original);
        let expired = restored.advance_active(40);
        assert_eq!(expired, original.advance_active(40));
        assert_eq!(expired.canceled, [crate::IntentionId(2)]);
        assert_eq!(restored, original);
        assert_eq!(
            restored.costs().resources().balance(Resource::Stamina),
            baseline
        );
        let mut forged = record;
        forged.conditions.0.clear();
        assert!(forged.restore(content, vec![]).is_err());
    }

    #[test]
    fn restored_conditions_cannot_mask_an_invalid_equipment_baseline() {
        let (base, _) = subject(false, false);
        let mut document = base.build().ruleset().document().clone();
        document.constants.derived_attribute_cap = 10;
        let attribute = Stat::Attribute {
            attribute: Attribute::Strength,
        };
        document
            .conditions
            .iter_mut()
            .find(|c| c.id.as_str() == "fear")
            .unwrap()
            .modifiers
            .push(Modifier {
                scope: Scope::Always,
                op: ModifierOp::Set {
                    stat: attribute.clone(),
                    value: 10,
                },
            });
        let content = Content::new(
            crate::rules::Ruleset::validate(document).unwrap(),
            base.build().content().catalog().clone(),
        )
        .unwrap();
        let build = BuildRecord::capture(base.build())
            .restore(content.clone())
            .unwrap();
        let mut original = CreatureState::new(build).unwrap();
        original
            .apply_condition(
                &crate::content_ids::ConditionId::new("fear").unwrap(),
                crate::ActorId(7),
                40,
            )
            .unwrap();
        let equipment = vec![EquipmentSource {
            weapon_profile: None,
            item: crate::ItemId(1),
            socket: crate::EquipmentSlotId(0),
            attack: None,
            modifiers: vec![Modifier {
                scope: Scope::Always,
                op: ModifierOp::Add {
                    stat: attribute,
                    value: 1000,
                },
            }],
        }];
        let mut sources = crate::creatures::state::equipment_sources(&equipment).unwrap();
        sources.extend(original.conditions().sources(original.build().ruleset()));
        assert!(
            crate::creatures::derive::derive_with_sources(original.build(), &sources, &equipment,)
                .is_ok(),
            "the condition must conceal the invalid equipment-only source set"
        );
        assert_eq!(
            CreatureRecord::capture(&original).restore(content, equipment.clone()),
            Err(crate::creatures::CreatureStateError::Build(
                BuildError::DerivedLimit
            ))
        );
        assert_eq!(
            CreatureState::from_recorded(
                original.build().clone(),
                original.build_epoch(),
                original.restorable_losses(),
                0,
                false,
                original.costs().clone(),
                original.conditions().clone(),
                equipment,
            ),
            Err(crate::creatures::CreatureStateError::Build(
                BuildError::DerivedLimit
            ))
        );
    }

    #[test]
    fn reused_restore_folds_still_reject_corrupt_mutable_records() {
        for conditioned in [false, true] {
            let (original, equipment) = subject(true, conditioned);
            let record = CreatureRecord::capture(&original);
            let content = original.build().content().clone();
            let mut invalid = Vec::new();
            let mut changed = record.clone();
            changed.injury = u64::MAX;
            invalid.push(changed);
            let mut changed = record.clone();
            changed.dead = true;
            invalid.push(changed);
            let mut changed = record.clone();
            changed.pools.stamina.balance = u32::MAX;
            invalid.push(changed);
            let mut changed = record.clone();
            changed.pools.stamina.recovery_elapsed = u64::MAX;
            invalid.push(changed);
            let mut changed = record.clone();
            changed.holds.0[0].owner = 0;
            invalid.push(changed);
            let mut changed = record.clone();
            changed.holds.0.push(changed.holds.0[0].clone());
            invalid.push(changed);
            let mut changed = record.clone();
            changed.holds.0[0].cost.stamina.as_mut().unwrap().resolution = u32::MAX;
            invalid.push(changed);
            let mut changed = record.clone();
            changed.holds.0[0].cost.stamina.as_mut().unwrap().resolution =
                changed.pools.stamina.balance + 1;
            invalid.push(changed);
            let mut changed = record.clone();
            changed.build.initial_attributes.strength = 6;
            invalid.push(changed);
            for (index, changed) in invalid.into_iter().enumerate() {
                assert!(
                    changed.restore(content.clone(), equipment.clone()).is_err(),
                    "accepted corruption {index}, conditioned={conditioned}"
                );
            }
            let mut wrong_costs = original.costs().clone();
            wrong_costs.set_maxima(ByResource::from_fn(|_| 0));
            assert!(CreatureState::from_recorded(
                original.build().clone(),
                original.build_epoch(),
                original.restorable_losses(),
                original.health().injury(),
                false,
                wrong_costs,
                original.conditions().clone(),
                equipment.clone(),
            )
            .is_err());
            let mut duplicate = equipment.clone();
            duplicate.push(equipment[0].clone());
            assert!(record.restore(content.clone(), duplicate).is_err());
            if conditioned {
                let mut immune = equipment;
                immune[0].modifiers.push(Modifier {
                    scope: Scope::Always,
                    op: ModifierOp::Immunity {
                        selector: crate::rules::Selector::Descriptor {
                            descriptor: crate::content_ids::DescriptorId::new("fear").unwrap(),
                        },
                    },
                });
                assert!(record.restore(content.clone(), immune).is_err());
                let mut changed = record;
                changed.conditions.0[0].remaining = 0;
                assert!(changed.restore(content, vec![]).is_err());
            }
        }
    }
    #[test]
    fn mixed_cost_bundle_record_restores_without_repaying_and_rejects_malformed_slots() {
        let (mut original, equipment) = subject(false, false);
        original.cancel_cost(crate::IntentionId(1));
        let bundle = crate::costs::CostBundle::single(crate::costs::ResourceCost {
            resource: Resource::Stamina,
            start: 0,
            resolution: 1,
        })
        .unwrap()
        .checked_add(
            crate::costs::CostBundle::single(crate::costs::ResourceCost {
                resource: Resource::Focus,
                start: 1,
                resolution: 1,
            })
            .unwrap(),
        )
        .unwrap();
        original.start_cost(crate::IntentionId(2), bundle).unwrap();
        let record = CreatureRecord::capture(&original);
        let encoded = serde_json::to_value(&record).unwrap();
        assert_eq!(
            encoded["holds"][0]["cost"],
            serde_json::json!({
                "stamina":{"start":0,"resolution":1}, "focus":{"start":1,"resolution":1}, "mana":null
            })
        );
        let decoded: CreatureRecord = serde_json::from_value(encoded.clone()).unwrap();
        let mut restored = decoded
            .restore(original.build().content().clone(), equipment.clone())
            .unwrap();
        assert_eq!(restored, original);
        assert_eq!(restored.finish_cost(crate::IntentionId(2)), Ok(bundle));
        original.finish_cost(crate::IntentionId(2)).unwrap();
        assert_eq!(restored, original);
        for slot in ["stamina", "focus", "mana"] {
            let mut invalid = encoded.clone();
            invalid["holds"][0]["cost"]
                .as_object_mut()
                .unwrap()
                .remove(slot);
            assert!(serde_json::from_value::<CreatureRecord>(invalid).is_err());
        }
        for value in [
            serde_json::json!([]),
            serde_json::json!({"start":0}),
            serde_json::json!({"start":0,"resolution":1,"extra":0}),
        ] {
            let mut invalid = encoded.clone();
            invalid["holds"][0]["cost"]["stamina"] = value;
            assert!(serde_json::from_value::<CreatureRecord>(invalid).is_err());
        }
        let mut empty = encoded;
        empty["holds"][0]["cost"] = serde_json::json!({"stamina":null,"focus":null,"mana":null});
        let empty: CreatureRecord = serde_json::from_value(empty).unwrap();
        assert!(empty
            .restore(original.build().content().clone(), equipment)
            .is_err());
        assert!(serde_json::from_str::<CostBundleRecord>(
            r#"{"stamina":null,"focus":null,"focus":null,"mana":null}"#
        )
        .is_err());
    }
}
