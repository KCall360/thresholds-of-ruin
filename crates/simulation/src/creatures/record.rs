//! Strict mutable creature records. Definitions live once in the save catalog.
use super::{
    AdvancementPurchase, BuildAdvancement, BuildEpoch, BuildError, Content, CreatureBuild,
    HdChoices, PlannedAdvancement, PurchaseSelection, RestorationPolicy,
};
use crate::attributes::{Attribute, Attributes, ByAttribute, ManaBinding, Skill};
use crate::bounded::{named_map, required_option, Bounded};
use crate::content_ids::{AbilityId, ClassId, FormId, SpeciesId, TalentId, TemplateId};
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
    injury: u32,
    dead: bool,
    #[serde(deserialize_with = "named_map")]
    health_recovery: HealthRecoveryRecord,
    nutrition: crate::nutrition::Nutrition,
    received_health_loss_remaining: u64,
    pools: ByResource<PoolRecord>,
    holds: Bounded<HoldRecord, { limits::COST_HOLDS }>,
    conditions: Bounded<crate::conditions::ConditionInstance, { limits::CONDITIONS }>,
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
            injury: u32,
            dead: bool,
            #[serde(deserialize_with = "named_map")]
            health_recovery: HealthRecoveryRecord,
            nutrition: crate::nutrition::Nutrition,
            received_health_loss_remaining: u64,
            pools: ByResource<PoolRecord>,
            holds: Bounded<HoldRecord, { limits::COST_HOLDS }>,
            conditions: Bounded<crate::conditions::ConditionInstance, { limits::CONDITIONS }>,
            cooldowns: Bounded<CooldownRecord, { limits::CATALOG_ENTRIES }>,
        }
        let fields: Fields = crate::bounded::named_map(deserializer)?;
        Ok(Self {
            build: fields.build,
            build_epoch: fields.build_epoch,
            earned_progress: fields.earned_progress,
            injury: fields.injury,
            dead: fields.dead,
            health_recovery: fields.health_recovery,
            nutrition: fields.nutrition,
            received_health_loss_remaining: fields.received_health_loss_remaining,
            pools: fields.pools,
            holds: fields.holds,
            conditions: fields.conditions,
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
    resource: Resource,
    start: u32,
    resolution: u32,
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
                        resource: cost.resource,
                        start: cost.start,
                        resolution: cost.resolution,
                    })
                    .collect(),
            ),
            conditions: Bounded(creature.conditions().instances().cloned().collect()),
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
            build.ruleset(),
        )
        .map_err(Error::Condition)?;
        let mut creature = super::CreatureState::from_recorded_with_costs(
            build,
            self.build_epoch,
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
                    .map(|hold| {
                        (
                            crate::IntentionId(hold.owner),
                            crate::costs::ResourceCost {
                                resource: hold.resource,
                                start: hold.start,
                                resolution: hold.resolution,
                            },
                        )
                    })
                    .collect();
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
    binding: BindingRecord,
    #[serde(deserialize_with = "required_option")]
    health_root_seed: Option<u64>,
    hd: Bounded<HdRecord, { limits::HIT_DICE }>,
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
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct HdRecord {
    source: SourceRecord,
    health_seed: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ChoiceRecord {
    purchases: Bounded<PurchaseRecord, { limits::POINTS_PER_HIT_DIE }>,
    #[serde(deserialize_with = "required_option")]
    attribute: Option<Attribute>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PurchaseRecord {
    pool: crate::content_ids::PointPoolId,
    selection: SelectionRecord,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum SelectionRecord {
    SkillRank { skill: Skill },
    Talent { talent: TalentId },
    TalentForm { talent: TalentId, form: FormId },
}
impl<'de> Deserialize<'de> for SelectionRecord {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            SkillRank { skill: Skill },
            Talent { talent: TalentId },
            TalentForm { talent: TalentId, form: FormId },
        }
        Ok(match named_map(deserializer)? {
            Fields::SkillRank { skill } => Self::SkillRank { skill },
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
    fn capture(choice: &HdChoices) -> Self {
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
    fn restore(&self) -> HdChoices {
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
                    .map(|die| HdRecord {
                        source: SourceRecord::capture(die.source()),
                        health_seed: die.health_seed(),
                    })
                    .collect(),
            ),
            choices: Bounded(build.choices().iter().map(ChoiceRecord::capture).collect()),
            templates: Bounded(build.template_ids().to_vec()),
        }
    }
    pub fn restore(&self, content: Arc<Content>) -> Result<CreatureBuild, BuildError> {
        let ledger = HdLedger::from_recorded(
            self.hd
                .0
                .iter()
                .map(|die| HitDie::new(die.source.restore(), die.health_seed))
                .collect(),
            self.health_root_seed,
        )
        .map_err(|_| BuildError::InvalidDefinition)?;
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
                crate::costs::ResourceCost {
                    resource: Resource::Stamina,
                    start: 1,
                    resolution: 1,
                },
            )
            .unwrap();
        creature.advance_active(3);
        (creature, equipment)
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
                crate::costs::ResourceCost {
                    resource: Resource::Stamina,
                    start: 0,
                    resolution: baseline + 1,
                },
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
            changed.injury = u32::MAX;
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
            changed.holds.0[0].resolution = u32::MAX;
            invalid.push(changed);
            let mut changed = record.clone();
            changed.holds.0[0].resolution = changed.pools.stamina.balance + 1;
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
}
