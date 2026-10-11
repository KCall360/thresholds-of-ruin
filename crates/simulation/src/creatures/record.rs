//! Strict mutable creature records. Definitions live once in the save catalog.
use super::{BuildError, Content, CreatureBuild, HdChoices};
use crate::attributes::{Attribute, Attributes, ByAttribute, ManaBinding, Skill};
use crate::bounded::{named_map, required_option, Bounded};
use crate::content_ids::{ClassId, SpeciesId, TalentId, TemplateId};
use crate::limits;
use crate::progression::{HdLedger, HdSource, HitDie};
use crate::resources::{ByResource, Resource, Resources};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreatureRecord {
    #[serde(deserialize_with = "named_map")]
    build: BuildRecord,
    injury: u32,
    dead: bool,
    pools: ByResource<PoolRecord>,
    holds: Bounded<HoldRecord, { limits::COST_HOLDS }>,
    conditions: Bounded<crate::conditions::ConditionInstance, { limits::CONDITIONS }>,
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
            injury: creature.health().injury(),
            dead: creature.health().dead(),
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
        super::CreatureState::from_recorded_with_costs(
            build,
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
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuildRecord {
    species: SpeciesId,
    initial_attributes: ByAttribute<u16>,
    binding: BindingRecord,
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
    training: Bounded<Skill, { limits::TRAINING_PER_HIT_DIE }>,
    #[serde(deserialize_with = "required_option")]
    attribute: Option<Attribute>,
    #[serde(deserialize_with = "required_option")]
    talent: Option<TalentId>,
}
impl BuildRecord {
    pub fn capture(build: &CreatureBuild) -> Self {
        Self {
            species: build.species().id.clone(),
            initial_attributes: ByAttribute::from_fn(|attribute| {
                build.initial_attributes().get(attribute)
            }),
            binding: BindingRecord::capture(build.binding()),
            hd: Bounded(
                build
                    .ledger()
                    .entries()
                    .iter()
                    .map(|die| HdRecord {
                        source: match die.source() {
                            HdSource::Racial => SourceRecord::Racial,
                            HdSource::Class(class) => SourceRecord::Class(class.clone()),
                        },
                        health_seed: die.health_seed(),
                    })
                    .collect(),
            ),
            choices: Bounded(
                build
                    .choices()
                    .iter()
                    .map(|choice| ChoiceRecord {
                        training: Bounded(choice.training.clone()),
                        attribute: choice.attribute,
                        talent: choice.talent.clone(),
                    })
                    .collect(),
            ),
            templates: Bounded(build.template_ids().to_vec()),
        }
    }
    pub fn restore(&self, content: Arc<Content>) -> Result<CreatureBuild, BuildError> {
        let ledger = HdLedger::new(
            self.hd
                .0
                .iter()
                .map(|die| {
                    HitDie::new(
                        match &die.source {
                            SourceRecord::Racial => HdSource::Racial,
                            SourceRecord::Class(id) => HdSource::Class(id.clone()),
                        },
                        die.health_seed,
                    )
                })
                .collect(),
        )
        .map_err(|_| BuildError::InvalidDefinition)?;
        let choices = self
            .choices
            .0
            .iter()
            .map(|choice| HdChoices {
                training: choice.training.0.clone(),
                attribute: choice.attribute,
                talent: choice.talent.clone(),
            })
            .collect();
        CreatureBuild::from_recorded(
            content,
            self.species.clone(),
            Attributes::new(self.initial_attributes).map_err(|_| BuildError::InvalidDefinition)?,
            ledger,
            self.binding.restore(),
            choices,
            self.templates.0.clone(),
        )
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
