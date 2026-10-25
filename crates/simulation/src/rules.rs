//! Validated immutable content. TOML parsing and content hashing belong to the host.
use crate::HdCount;
mod advancement;
mod cancellation;
pub(crate) use cancellation::expand_cancellable_source;
pub use cancellation::CancellationInvisibilityShape;
mod forms;
mod growth;
mod hallucination;
mod interaction;
mod strikes;
pub use advancement::AdvancementPolicy;
pub use forms::FormDef;
pub use growth::{GrowthPolicy, ThreatOverride, ThreatPolicy, ThreatSource};
pub(crate) use interaction::EffectiveInteractionError;
pub use interaction::Interaction;
#[cfg(test)]
mod supply_tests;
pub use strikes::{
    append_natural_riders, validate_rider_shapes, validate_strike_horizon, validate_strike_shapes,
    RiderCheckDef, RiderCondition, RiderDef, RiderTrigger, StrikeDef, StrikeSource,
};

use crate::content_ids::Interner;
use crate::{
    attributes::{Attribute, Skill},
    combat::DamageType,
    content_ids::*,
    dice::DicePool,
    resources::Resource,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RulesetDocument {
    pub id: RulesetId,
    pub constants: Constants,
    pub point_pools: Vec<PointPoolDef>,
    #[serde(deserialize_with = "strikes::bounded_vec::<_, _, { crate::limits::CATALOG_ENTRIES }>")]
    pub supplies: Vec<SupplyDef>,
    pub abilities: Vec<AbilityDef>,
    pub forms: Vec<FormDef>,
    pub ability_tags: Vec<AbilityTagDef>,
    pub talents: Vec<TalentDef>,
    #[serde(deserialize_with = "strikes::bounded_vec::<_, _, { crate::limits::CATALOG_ENTRIES }>")]
    pub expertises: Vec<crate::expertise::ExpertiseDef>,
    pub creature_types: Vec<TypeDef>,
    pub subtypes: Vec<SubtypeDef>,
    pub classes: Vec<ClassDef>,
    pub conditions: Vec<ConditionDef>,
    pub descriptors: Vec<NamedDescriptor>,
    pub flags: Vec<FlagDef>,
    pub resources: Vec<ResourceDef>,
    pub health_recovery: HealthRecoveryDef,
    pub nutrition: crate::nutrition::NutritionPolicy,
    pub nutrition_condition_trigger: crate::nutrition::NutritionConditionTrigger,
    pub advancement: AdvancementPolicy,
    pub growth: GrowthPolicy,
    pub equipment_slots: Vec<EquipmentSlotDef>,
}
impl<'de> serde::Deserialize<'de> for RulesetDocument {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            id: RulesetId,
            constants: Constants,
            point_pools: Vec<PointPoolDef>,
            #[serde(
                deserialize_with = "strikes::bounded_vec::<_, _, { crate::limits::CATALOG_ENTRIES }>"
            )]
            supplies: Vec<SupplyDef>,
            abilities: Vec<AbilityDef>,
            #[serde(
                deserialize_with = "strikes::bounded_vec::<_, _, { crate::limits::CATALOG_ENTRIES }>"
            )]
            forms: Vec<FormDef>,
            ability_tags: Vec<AbilityTagDef>,
            talents: Vec<TalentDef>,
            #[serde(
                deserialize_with = "strikes::bounded_vec::<_, _, { crate::limits::CATALOG_ENTRIES }>"
            )]
            expertises: Vec<crate::expertise::ExpertiseDef>,
            creature_types: Vec<TypeDef>,
            subtypes: Vec<SubtypeDef>,
            classes: Vec<ClassDef>,
            conditions: Vec<ConditionDef>,
            descriptors: Vec<NamedDescriptor>,
            flags: Vec<FlagDef>,
            resources: Vec<ResourceDef>,
            health_recovery: HealthRecoveryDef,
            nutrition: crate::nutrition::NutritionPolicy,
            nutrition_condition_trigger: crate::nutrition::NutritionConditionTrigger,
            advancement: AdvancementPolicy,
            growth: GrowthPolicy,
            equipment_slots: Vec<EquipmentSlotDef>,
        }
        let fields: Fields = crate::bounded::named_map(deserializer)?;
        Ok(Self {
            id: fields.id,
            constants: fields.constants,
            point_pools: fields.point_pools,
            supplies: fields.supplies,
            abilities: fields.abilities,
            forms: fields.forms,
            ability_tags: fields.ability_tags,
            talents: fields.talents,
            expertises: fields.expertises,
            creature_types: fields.creature_types,
            subtypes: fields.subtypes,
            classes: fields.classes,
            conditions: fields.conditions,
            descriptors: fields.descriptors,
            flags: fields.flags,
            resources: fields.resources,
            health_recovery: fields.health_recovery,
            nutrition: fields.nutrition,
            nutrition_condition_trigger: fields.nutrition_condition_trigger,
            advancement: fields.advancement,
            growth: fields.growth,
            equipment_slots: fields.equipment_slots,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Constants {
    pub base_defense: i32,
    pub attribute_cap: u16,
    pub derived_attribute_cap: u16,
    pub attribute_opportunity_interval: u16,
    pub racial_point_grants: Vec<PointGrant>,
    pub duration_scale_numerator: u16,
    pub duration_scale_offset: u16,
    pub default_abilities: Vec<AbilityId>,
    pub item_timing: ItemTiming,
    pub default_anatomy: crate::AnatomySpec,
}
/// Point economies are rules data; each purchase spends one point in its named pool.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PurchaseKind {
    SkillRank,
    Talent,
    Expertise,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PointPoolDef {
    pub id: PointPoolId,
    pub name: String,
    pub allowed: Vec<PurchaseKind>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PointGrant {
    pub pool: PointPoolId,
    pub points: u16,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
/// Rules-owned compatibility, capabilities, presentation and turn multipliers.
pub struct EquipmentSlotDef {
    pub id: EquipmentSlot,
    pub name: String,
    pub equipped_phrase: String,
    pub compatible_classes: Vec<crate::ItemClass>,
    pub requires_attack: bool,
    pub capacity: crate::equipment_capacity::EquipmentCapacityUse,
    pub equip_turns: u64,
    pub unequip_turns: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ItemTiming {
    pub consume_turns: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceDef {
    pub resource: Resource,
    pub recovery_period: u64,
    pub capacity: ResourceCapacity,
}
/// A bounded linear HD curve; all constants are authored rules content.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HealthRecoveryDef {
    pub base_period: u64,
    pub reduction_per_hd: u64,
    pub minimum_period: u64,
}
impl HealthRecoveryDef {
    pub fn period(&self, hit_dice: HdCount) -> u64 {
        self.base_period
            .saturating_sub(
                self.reduction_per_hd
                    .saturating_mul(hit_dice.get().saturating_sub(1)),
            )
            .max(self.minimum_period)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceCapacity {
    pub base: u32,
    pub attribute: CapacityAttribute,
    pub required_flag: Option<FlagId>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CapacityAttribute {
    Named { attribute: Attribute },
    Binding,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NamedDescriptor {
    pub id: DescriptorId,
    pub name: String,
    /// Authored omission means no secondary resistance policy.
    pub resistance: Option<DescriptorResistance>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DescriptorResistance {
    pub skill: Skill,
    pub difficulty: i32,
}
impl<'de> Deserialize<'de> for DescriptorResistance {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            skill: Skill,
            difficulty: i32,
        }
        let fields: Fields = crate::bounded::named_map(d)?;
        if !(0..=crate::limits::NUMERIC).contains(&i64::from(fields.difficulty)) {
            return Err(serde::de::Error::custom(
                "invalid descriptor resistance difficulty",
            ));
        }
        Ok(Self {
            skill: fields.skill,
            difficulty: fields.difficulty,
        })
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FlagDef {
    pub id: FlagId,
    pub name: String,
    pub modifiers: Vec<Modifier>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TypeDef {
    pub id: TypeId,
    pub name: String,
    pub health_die: u16,
    pub modifiers: Vec<Modifier>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubtypeDef {
    pub id: SubtypeId,
    pub name: String,
    pub mechanics_deferred: bool,
    pub modifiers: Vec<Modifier>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClassDef {
    pub id: ClassId,
    pub name: String,
    pub health_die: u16,
    pub point_grants: Vec<PointGrant>,
    pub modifiers: Vec<Modifier>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TalentDef {
    pub id: TalentId,
    pub name: String,
    pub prerequisites: Vec<TalentId>,
    pub requirement: Requirement,
    pub modifiers: Vec<Modifier>,
    pub form_choices: Vec<FormId>,
    pub expertise_grants: Vec<ExpertiseId>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Requirement {
    All { requirements: Vec<Requirement> },
    Any { requirements: Vec<Requirement> },
    TotalHd { minimum: HdCount },
    RacialHd { minimum: HdCount },
    ClassLevel { class: ClassId, minimum: HdCount },
    TypeIn { types: Vec<TypeId> },
    SubtypeIn { subtypes: Vec<SubtypeId> },
    OwnedTalent { talent: TalentId },
    SpeciesFlag { flag: FlagId, present: bool },
    SkillRank { skill: SkillSelector, minimum: u16 },
    AttributeMinimum { attribute: Attribute, minimum: u16 },
    Flag { flag: FlagId, present: bool },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SkillSelector {
    Named { skill: Skill },
    NaturalAttack,
    SelectedWeapon,
}
impl<'de> Deserialize<'de> for SkillSelector {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // Empty struct records reject unknown fields that internally tagged unit
        // variants would otherwise discard before source validation.
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Record {
            Named { skill: Skill },
            NaturalAttack {},
            SelectedWeapon {},
        }
        Ok(match Record::deserialize(deserializer)? {
            Record::Named { skill } => Self::Named { skill },
            Record::NaturalAttack {} => Self::NaturalAttack,
            Record::SelectedWeapon {} => Self::SelectedWeapon,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DefenseKind {
    Physical,
    Cognitive,
    Spiritual,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AbilityStat {
    HoldDamageIncrement,
    Dice,
    FlatBonus,
    CheckBonus,
    CheckEdge,
    Difficulty,
    Duration,
    Range,
    Preparation,
    Recovery,
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Stat {
    Attribute {
        attribute: Attribute,
    },
    Defense {
        defense: DefenseKind,
    },
    Health,
    HealthRecoveryPeriod,
    HealthRecoveryPeriodPercent,
    HealthRecoveryResetPercent,
    AttackDamageRollBonus,
    NutritionRateThirtieths,
    ResourceMax {
        resource: Resource,
    },
    Ability {
        ability: AbilityId,
        stat: AbilityStat,
    },
    AbilityTag {
        tag: AbilityTagId,
        stat: AbilityStat,
    },
    Edge,
    CheckEdge,
    IncomingEdge {
        defense: Option<DefenseKind>,
    },
}

impl<'de> Deserialize<'de> for Stat {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // Serde's internally tagged unit variants ignore extra fields. Empty
        // struct variants enforce the strict authoring boundary without changing
        // the public enum constructors or canonical serialized representation.
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Recorded {
            Attribute {
                attribute: Attribute,
            },
            Defense {
                defense: DefenseKind,
            },
            Health {},
            HealthRecoveryPeriod {},
            HealthRecoveryPeriodPercent {},
            HealthRecoveryResetPercent {},
            AttackDamageRollBonus {},
            NutritionRateThirtieths {},
            ResourceMax {
                resource: Resource,
            },
            Ability {
                ability: AbilityId,
                stat: AbilityStat,
            },
            AbilityTag {
                tag: AbilityTagId,
                stat: AbilityStat,
            },
            Edge {},
            CheckEdge {},
            IncomingEdge {
                defense: Option<DefenseKind>,
            },
        }
        Ok(match Recorded::deserialize(deserializer)? {
            Recorded::Attribute { attribute } => Self::Attribute { attribute },
            Recorded::Defense { defense } => Self::Defense { defense },
            Recorded::Health {} => Self::Health,
            Recorded::HealthRecoveryPeriod {} => Self::HealthRecoveryPeriod,
            Recorded::HealthRecoveryPeriodPercent {} => Self::HealthRecoveryPeriodPercent,
            Recorded::HealthRecoveryResetPercent {} => Self::HealthRecoveryResetPercent,
            Recorded::AttackDamageRollBonus {} => Self::AttackDamageRollBonus,
            Recorded::NutritionRateThirtieths {} => Self::NutritionRateThirtieths,
            Recorded::ResourceMax { resource } => Self::ResourceMax { resource },
            Recorded::Ability { ability, stat } => Self::Ability { ability, stat },
            Recorded::AbilityTag { tag, stat } => Self::AbilityTag { tag, stat },
            Recorded::Edge {} => Self::Edge,
            Recorded::CheckEdge {} => Self::CheckEdge,
            Recorded::IncomingEdge { defense } => Self::IncomingEdge { defense },
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Selector {
    Category { category: DamageType },
    Descriptor { descriptor: DescriptorId },
}
// A sum type prevents nonsensical stat/operator pairs (for example Health + immunity).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ModifierOp {
    /// Add an attribute contribution without feeding one nominated defense.
    AttributeWithoutDefense {
        attribute: Attribute,
        defense: DefenseKind,
        value: i64,
    },
    /// Signed adjustment combines with ordinary and talent DR before its zero floor.
    SignedReduction {
        selector: Selector,
        amount: i32,
    },
    SustainAttribute {
        attribute: Attribute,
    },
    ProtectArmor {},
    /// Strongest source wins; this rate never stacks with duplicate sources.
    HealthRecoveryRate {
        multiplier: u16,
    },
    ActionDuration {
        value: crate::action_duration::ActionDuration,
    },
    Perception {
        value: crate::perception::PerceptionValue,
    },
    Locomotion {
        value: crate::locomotion::LocomotionValue,
    },
    Add {
        stat: Stat,
        value: i64,
    },
    Set {
        stat: Stat,
        value: i64,
    },
    Flag {
        flag: FlagId,
    },
    Immunity {
        selector: Selector,
    },
    Reduction {
        selector: Selector,
        amount: u32,
    },
    /// Shares the strongest category-only talent bucket, including when a timed
    /// condition carries the protection. Ordinary reduction remains additive.
    TalentReduction {
        category: DamageType,
        #[serde(deserialize_with = "read_reduction_amount")]
        amount: u32,
    },
    GrantAbility {
        ability: AbilityId,
    },
}
impl<'de> Deserialize<'de> for ModifierOp {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            /// Add an attribute contribution without feeding one nominated defense.
            AttributeWithoutDefense {
                attribute: Attribute,
                defense: DefenseKind,
                value: i64,
            },
            /// Signed adjustment combines with ordinary and talent DR before its zero floor.
            SignedReduction {
                selector: Selector,
                amount: i32,
            },
            SustainAttribute {
                attribute: Attribute,
            },
            ProtectArmor {},
            /// Strongest source wins; this rate never stacks with duplicate sources.
            HealthRecoveryRate {
                multiplier: u16,
            },
            ActionDuration {
                value: crate::action_duration::ActionDuration,
            },
            Perception {
                value: crate::perception::PerceptionValue,
            },
            Locomotion {
                value: crate::locomotion::LocomotionValue,
            },
            Add {
                stat: Stat,
                value: i64,
            },
            Set {
                stat: Stat,
                value: i64,
            },
            Flag {
                flag: FlagId,
            },
            Immunity {
                selector: Selector,
            },
            Reduction {
                selector: Selector,
                amount: u32,
            },
            TalentReduction {
                category: DamageType,
                #[serde(deserialize_with = "read_reduction_amount")]
                amount: u32,
            },
            GrantAbility {
                ability: AbilityId,
            },
        }
        Ok(match crate::bounded::named_map(deserializer)? {
            Fields::AttributeWithoutDefense {
                attribute,
                defense,
                value,
            } => Self::AttributeWithoutDefense {
                attribute,
                defense,
                value,
            },
            Fields::SignedReduction { selector, amount } => {
                Self::SignedReduction { selector, amount }
            }
            Fields::SustainAttribute { attribute } => Self::SustainAttribute { attribute },
            Fields::ProtectArmor {} => Self::ProtectArmor {},
            Fields::HealthRecoveryRate { multiplier } => Self::HealthRecoveryRate { multiplier },
            Fields::ActionDuration { value } => Self::ActionDuration { value },
            Fields::Perception { value } => Self::Perception { value },
            Fields::Locomotion { value } => Self::Locomotion { value },
            Fields::Add { stat, value } => Self::Add { stat, value },
            Fields::Set { stat, value } => Self::Set { stat, value },
            Fields::Flag { flag } => Self::Flag { flag },
            Fields::Immunity { selector } => Self::Immunity { selector },
            Fields::Reduction { selector, amount } => Self::Reduction { selector, amount },
            Fields::TalentReduction { category, amount } => {
                Self::TalentReduction { category, amount }
            }
            Fields::GrantAbility { ability } => Self::GrantAbility { ability },
        })
    }
}

fn read_reduction_amount<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<u32, D::Error> {
    let amount = u32::deserialize(deserializer)?;
    if i64::from(amount) > crate::limits::NUMERIC {
        return Err(serde::de::Error::custom("reduction exceeds numeric bound"));
    }
    Ok(amount)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    Always,
    VersusSource,
    WhileNoSensedHostile,
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Modifier {
    #[serde(default)]
    pub cancellable: bool,
    pub scope: Scope,
    pub op: ModifierOp,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AbilityTagDef {
    pub id: AbilityTagId,
    pub name: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AbilityDef {
    #[serde(default)]
    pub cancellable: bool,
    pub interaction: Interaction,
    #[serde(default)]
    pub invocation: crate::abilities::InvocationKind,
    pub id: AbilityId,
    pub tags: Vec<AbilityTagId>,
    pub name: String,
    pub basic_attack: bool,
    /// Source-sign selection for the action owner's outgoing checks and shared budget.
    /// Authored omission is ordinary; the independent stored adapter requires it.
    #[serde(default)]
    pub outgoing_edge_policy: OutgoingEdgePolicy,
    pub cost: Option<Cost>,
    /// Authoring may omit this field; saved compiled rules use a required-null adapter.
    #[serde(default)]
    pub cooldown: Option<std::num::NonZeroU64>,
    #[serde(default, deserialize_with = "read_supply_costs")]
    pub supply_costs: Vec<SupplyCost>,
    pub timing: Timing,
    pub reach: Reach,
    pub strikes: StrikeSource,
    pub ai: AiHints,
}
impl<'de> serde::Deserialize<'de> for AbilityDef {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            #[serde(default)]
            cancellable: bool,
            interaction: Interaction,
            id: AbilityId,
            #[serde(default)]
            invocation: crate::abilities::InvocationKind,
            tags: Vec<AbilityTagId>,
            name: String,
            basic_attack: bool,
            #[serde(default)]
            outgoing_edge_policy: OutgoingEdgePolicy,
            cost: Option<Cost>,
            #[serde(default)]
            cooldown: Option<std::num::NonZeroU64>,
            #[serde(default, deserialize_with = "read_supply_costs")]
            supply_costs: Vec<SupplyCost>,
            timing: Timing,
            reach: Reach,
            strikes: StrikeSource,
            ai: AiHints,
        }
        let fields: Fields = crate::bounded::named_map(deserializer)?;
        Ok(Self {
            cancellable: fields.cancellable,
            interaction: fields.interaction,
            invocation: fields.invocation,
            id: fields.id,
            tags: fields.tags,
            name: fields.name,
            basic_attack: fields.basic_attack,
            outgoing_edge_policy: fields.outgoing_edge_policy,
            cost: fields.cost,
            cooldown: fields.cooldown,
            supply_costs: fields.supply_costs,
            timing: fields.timing,
            reach: fields.reach,
            strikes: fields.strikes,
            ai: fields.ai,
        })
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum OutgoingEdgePolicy {
    #[default]
    Ordinary,
    IgnoreNegative,
}
impl<'de> Deserialize<'de> for OutgoingEdgePolicy {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            Ordinary {},
            IgnoreNegative {},
        }
        Ok(match crate::bounded::named_map(deserializer)? {
            Fields::Ordinary {} => Self::Ordinary,
            Fields::IgnoreNegative {} => Self::IgnoreNegative,
        })
    }
}
impl OutgoingEdgePolicy {
    pub(crate) fn allows(self, amount: i64) -> bool {
        self == Self::Ordinary || amount >= 0
    }
    pub(crate) fn for_role(self, role: CheckActor) -> Self {
        match role {
            CheckActor::User => self,
            CheckActor::Target => Self::Ordinary,
        }
    }
    /// Validated next-check tokens contain only unscoped signed check-edge Adds.
    /// Qualification uses surviving contributions, before any family cancellation.
    pub(crate) fn selects_token(self, condition: &ConditionDef) -> bool {
        self == Self::Ordinary || condition.modifiers.iter().any(|modifier| {
            matches!(modifier.op, ModifierOp::Add { value, .. } if self.allows(value))
        })
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SupplyDef {
    pub id: SupplyId,
    pub name: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SupplyCost {
    pub supply: SupplyId,
    pub quantity: u64,
    pub access: SupplyAccess,
    pub spend_at: SupplySpendAt,
}
impl<'de> Deserialize<'de> for SupplyDef {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            id: SupplyId,
            name: String,
        }
        let value: Fields = crate::bounded::named_map(deserializer)?;
        Ok(Self {
            id: value.id,
            name: value.name,
        })
    }
}
impl<'de> Deserialize<'de> for SupplyCost {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            supply: SupplyId,
            quantity: u64,
            access: SupplyAccess,
            spend_at: SupplySpendAt,
        }
        let value: Fields = crate::bounded::named_map(deserializer)?;
        Ok(Self {
            supply: value.supply,
            quantity: value.quantity,
            access: value.access,
            spend_at: value.spend_at,
        })
    }
}
/// Eager allocation bound shared by authored costs and frozen action operands.
pub(crate) fn read_supply_costs<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<SupplyCost>, D::Error> {
    strikes::bounded_vec::<_, _, { crate::limits::SUPPLY_COSTS }>(deserializer)
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SupplyAccess {
    CarriedOnly,
    CarriedOrGroundTouch,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SupplySpendAt {
    FirstResolution,
    ReplacementCommit,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cost {
    pub resource: Resource,
    pub start: u32,
    pub resolution: u32,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Timing {
    pub source: TimingSource,
    pub scaled_by: Option<Attribute>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TimingSource {
    Fixed { prepare: u64, recover: u64 },
    SelectedWeapon,
    SelectedWeaponAdjusted { preparation: u64, recovery: u64 },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Reach {
    Melee,
    /// Short/long distances come from the exact effective delivery profile.
    Projectile,
    Sight {
        range: u16,
    },
    MutualSight {
        range: u16,
    },
    /// Complete physical contact flight; independent of sight radius.
    UntilObstruction,
    SelfOnly,
    SelfOrAlly {
        range: AllyRange,
    },
}
impl<'de> Deserialize<'de> for Reach {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            Melee,
            Projectile {},
            Sight { range: u16 },
            MutualSight { range: u16 },
            UntilObstruction {},
            SelfOnly,
            SelfOrAlly { range: AllyRange },
        }
        Ok(match crate::bounded::named_map(d)? {
            Fields::Melee => Self::Melee,
            Fields::Projectile {} => Self::Projectile,
            Fields::Sight { range } => Self::Sight { range },
            Fields::MutualSight { range } => Self::MutualSight { range },
            Fields::UntilObstruction {} => Self::UntilObstruction,
            Fields::SelfOnly => Self::SelfOnly,
            Fields::SelfOrAlly { range } => Self::SelfOrAlly { range },
        })
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AllyRange {
    Touch,
    Sight { range: u16 },
}
impl<'de> Deserialize<'de> for AllyRange {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            Touch {},
            Sight { range: u16 },
        }
        Ok(match crate::bounded::named_map(deserializer)? {
            Fields::Touch {} => Self::Touch,
            Fields::Sight { range } => Self::Sight { range },
        })
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AiHints {
    pub priority: i16,
    pub hostile: bool,
    pub minimum_resource_percent: u8,
    pub range_preference: RangePreference,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RangePreference {
    Any,
    OutsideMelee,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckActor {
    User,
    Target,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckDef {
    pub actor: CheckActor,
    pub skill: SkillSelector,
    pub against: Against,
}
impl CheckDef {
    /// Structural participation survives the freezing of a defense into a fixed
    /// threshold. A user's personal difficulty alone does not involve a subject.
    pub(crate) fn target_participates(&self) -> bool {
        self.actor == CheckActor::Target || !matches!(self.against, Against::Difficulty { .. })
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Against {
    Defense { defense: DefenseKind },
    Difficulty { formula: Formula },
    Opposed { skill: SkillSelector },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Formula {
    pub base: i32,
    pub skill: Option<SkillSelector>,
    pub attribute: Option<Attribute>,
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiceExpr {
    pub count: u16,
    pub sides: u16,
    pub bonus: i32,
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum DamageExpr {
    SelectedWeapon {
        impact_bonus: u32,
    },
    Components {
        #[serde(
            deserialize_with = "strikes::bounded_vec::<_, _, { crate::limits::DAMAGE_COMPONENTS }>"
        )]
        components: Vec<DamageComponentExpr>,
    },
}
/// Shared operand construction for authored capture and effective validation.
/// Fixed authored values stay fixed; only rolled component pools are augmented.
pub(crate) fn compile_damage_components(
    components: &[DamageComponentExpr],
    extra_dice: u16,
    flat_bonus: i32,
) -> Result<crate::damage::DamageSpec, crate::damage::DamageError> {
    use crate::damage::{DamageComponent, DamageError, DamageSpec};
    let values = components
        .iter()
        .map(|component| {
            Ok(match &component.amount {
                AmountExpr::Fixed { amount } => DamageComponent::fixed(
                    component.category,
                    component.descriptor.clone(),
                    *amount,
                ),
                AmountExpr::Rolled { dice } => DamageComponent::rolled(
                    component.category,
                    component.descriptor.clone(),
                    DicePool::new(dice.count, dice.sides, dice.bonus)
                        .and_then(|pool| pool.augmented(extra_dice, flat_bonus))
                        .map_err(|_| DamageError::InvalidSpec)?,
                ),
            })
        })
        .collect::<Result<Vec<_>, DamageError>>()?;
    let primary = values.first().map(DamageComponent::key);
    DamageSpec::new(values, primary)
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DamageComponentExpr {
    pub category: DamageType,
    pub descriptor: Option<DescriptorId>,
    pub amount: AmountExpr,
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AmountExpr {
    Fixed {
        amount: u32,
    },
    Rolled {
        #[serde(deserialize_with = "crate::bounded::named_map")]
        dice: DiceExpr,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HealingOverflow {
    None,
    ImproveEarliestHitDie,
    ImproveEarliestHitDiceByExcess,
}
impl<'de> Deserialize<'de> for HealingOverflow {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match String::deserialize(deserializer)?.as_str() {
            "none" => Ok(Self::None),
            "improve_earliest_hit_die" => Ok(Self::ImproveEarliestHitDie),
            "improve_earliest_hit_dice_by_excess" => Ok(Self::ImproveEarliestHitDiceByExcess),
            _ => Err(serde::de::Error::custom("unknown healing overflow policy")),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EffectRecipient {
    Target,
    Source,
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Effect {
    Theft {
        selection: crate::theft::TheftSelection,
        relocation: Option<crate::theft::RelocationGate>,
    },
    AlterEquipment {
        selection: crate::EquippedSelection,
        alteration: crate::EquipmentAlteration,
    },
    CorrodeArmor {
        slot: crate::EquipmentSlot,
        amount: u32,
    },
    ApplyLinkedHold {
        policy: crate::linked_hold::HoldPolicy,
    },
    HoldDamage {
        category: crate::combat::DamageType,
        descriptor: Option<crate::content_ids::DescriptorId>,
    },
    DrainLatestHitDie,
    RestoreHitDie,
    TeleportRandom,
    TeleportToRayOrigin,
    RoomLight,
    DetectMonsters {
        condition: ConditionId,
    },
    DetectFood,
    MapTopology,
    DetectMagic,
    DrainLife,
    GainStrength,
    RaiseLevel,
    /// Target-only permanent special suppression and current concealment cleanup.
    CancelSpecials,
    HasteDose {
        condition: ConditionId,
        rest: ConditionId,
    },
    ArrestDescent,
    StepSelfOneCell,
    PushTargetOneCell {
        direction: crate::abilities::PushDirection,
    },
    Damage {
        damage: DamageExpr,
    },
    Heal {
        amount: AmountExpr,
    },
    HealPerHitDie {
        sides: u16,
        bonus: i32,
        overflow: HealingOverflow,
    },
    Nourish {
        amount: AmountExpr,
    },
    LoseAttribute {
        attribute: Attribute,
        amount: AmountExpr,
    },
    RestoreAttribute {
        attribute: Attribute,
        amount: AmountExpr,
    },
    LoseMaximumHealth {
        amount: AmountExpr,
    },
    RestoreMaximumHealth {
        amount: AmountExpr,
    },
    ApplyCondition {
        condition: ConditionId,
        duration: u64,
    },
    /// Establish a condition until removed, or remove the active `cancels`
    /// condition instead. Cancellation is ordinary removal, independent of
    /// immunity to establishing `condition`. Delivery and whole-payload
    /// protections still gate either operation.
    ApplyPersistentCondition {
        condition: ConditionId,
        #[serde(deserialize_with = "crate::bounded::required_option")]
        cancels: Option<ConditionId>,
    },
    ApplyConditionForTurns {
        condition: ConditionId,
        duration: AmountExpr,
    },
    RemoveCondition {
        condition: ConditionId,
    },
    Restore {
        resource: Resource,
        amount: AmountExpr,
    },
    SearchAdjacentGuises,
    ObservePublicPlan,
    ApplyQualifiedConditionToSource {
        condition: ConditionId,
        duration: u64,
        ability_tag: AbilityTagId,
    },
}
impl Effect {
    pub fn recipient(&self) -> EffectRecipient {
        match self {
            Self::RoomLight
            | Self::DetectMonsters { .. }
            | Self::DetectFood
            | Self::MapTopology
            | Self::DetectMagic
            | Self::DrainLife
            | Self::AlterEquipment { .. }
            | Self::SearchAdjacentGuises
            | Self::ObservePublicPlan
            | Self::ApplyQualifiedConditionToSource { .. } => EffectRecipient::Source,
            _ => EffectRecipient::Target,
        }
    }
}
impl<'de> Deserialize<'de> for Effect {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            Theft {
                selection: crate::theft::TheftSelection,
                #[serde(deserialize_with = "crate::bounded::required_option")]
                relocation: Option<crate::theft::RelocationGate>,
            },
            AlterEquipment {
                selection: crate::EquippedSelection,
                alteration: crate::EquipmentAlteration,
            },
            CorrodeArmor {
                slot: crate::EquipmentSlot,
                #[serde(deserialize_with = "crate::armor::read_corrosion_amount")]
                amount: u32,
            },
            ApplyLinkedHold {
                policy: crate::linked_hold::HoldPolicy,
            },
            HoldDamage {
                category: crate::combat::DamageType,
                descriptor: Option<crate::content_ids::DescriptorId>,
            },
            DrainLatestHitDie {},
            RestoreHitDie {},
            TeleportRandom {},
            TeleportToRayOrigin {},
            RoomLight {},
            DetectMonsters {
                condition: ConditionId,
            },
            DetectFood {},
            MapTopology {},
            DetectMagic {},
            DrainLife {},
            GainStrength {},
            RaiseLevel {},
            CancelSpecials {},
            HasteDose {
                condition: ConditionId,
                rest: ConditionId,
            },
            ArrestDescent {},
            StepSelfOneCell {},
            PushTargetOneCell {
                direction: crate::abilities::PushDirection,
            },
            Damage {
                damage: DamageExpr,
            },
            Heal {
                amount: AmountExpr,
            },
            HealPerHitDie {
                sides: u16,
                bonus: i32,
                overflow: HealingOverflow,
            },
            Nourish {
                amount: AmountExpr,
            },
            LoseAttribute {
                attribute: Attribute,
                amount: AmountExpr,
            },
            RestoreAttribute {
                attribute: Attribute,
                amount: AmountExpr,
            },
            LoseMaximumHealth {
                amount: AmountExpr,
            },
            RestoreMaximumHealth {
                amount: AmountExpr,
            },
            ApplyCondition {
                condition: ConditionId,
                duration: u64,
            },
            ApplyPersistentCondition {
                condition: ConditionId,
                #[serde(deserialize_with = "crate::bounded::required_option")]
                cancels: Option<ConditionId>,
            },
            ApplyConditionForTurns {
                condition: ConditionId,
                duration: AmountExpr,
            },
            RemoveCondition {
                condition: ConditionId,
            },
            Restore {
                resource: Resource,
                amount: AmountExpr,
            },
            SearchAdjacentGuises {},
            ObservePublicPlan {},
            ApplyQualifiedConditionToSource {
                condition: ConditionId,
                duration: u64,
                ability_tag: AbilityTagId,
            },
        }
        Ok(match crate::bounded::named_map(deserializer)? {
            Fields::Theft {
                selection,
                relocation,
            } => Self::Theft {
                selection,
                relocation,
            },
            Fields::AlterEquipment {
                selection,
                alteration,
            } => Self::AlterEquipment {
                selection,
                alteration,
            },
            Fields::CorrodeArmor { slot, amount } => Self::CorrodeArmor { slot, amount },
            Fields::ApplyLinkedHold { policy } => Self::ApplyLinkedHold { policy },
            Fields::HoldDamage {
                category,
                descriptor,
            } => Self::HoldDamage {
                category,
                descriptor,
            },
            Fields::DrainLatestHitDie {} => Self::DrainLatestHitDie,
            Fields::RestoreHitDie {} => Self::RestoreHitDie,
            Fields::TeleportRandom {} => Self::TeleportRandom,
            Fields::TeleportToRayOrigin {} => Self::TeleportToRayOrigin,
            Fields::RoomLight {} => Self::RoomLight,
            Fields::DetectMonsters { condition } => Self::DetectMonsters { condition },
            Fields::DetectFood {} => Self::DetectFood,
            Fields::MapTopology {} => Self::MapTopology,
            Fields::DetectMagic {} => Self::DetectMagic,
            Fields::DrainLife {} => Self::DrainLife,
            Fields::GainStrength {} => Self::GainStrength,
            Fields::RaiseLevel {} => Self::RaiseLevel,
            Fields::CancelSpecials {} => Self::CancelSpecials,
            Fields::HasteDose { condition, rest } => Self::HasteDose { condition, rest },
            Fields::ArrestDescent {} => Self::ArrestDescent,
            Fields::StepSelfOneCell {} => Self::StepSelfOneCell,
            Fields::PushTargetOneCell { direction } => Self::PushTargetOneCell { direction },
            Fields::Damage { damage } => Self::Damage { damage },
            Fields::Heal { amount } => Self::Heal { amount },
            Fields::HealPerHitDie {
                sides,
                bonus,
                overflow,
            } => Self::HealPerHitDie {
                sides,
                bonus,
                overflow,
            },
            Fields::Nourish { amount } => Self::Nourish { amount },
            Fields::LoseAttribute { attribute, amount } => {
                Self::LoseAttribute { attribute, amount }
            }
            Fields::RestoreAttribute { attribute, amount } => {
                Self::RestoreAttribute { attribute, amount }
            }
            Fields::LoseMaximumHealth { amount } => Self::LoseMaximumHealth { amount },
            Fields::RestoreMaximumHealth { amount } => Self::RestoreMaximumHealth { amount },
            Fields::ApplyCondition {
                condition,
                duration,
            } => Self::ApplyCondition {
                condition,
                duration,
            },
            Fields::ApplyPersistentCondition { condition, cancels } => {
                Self::ApplyPersistentCondition { condition, cancels }
            }
            Fields::ApplyConditionForTurns {
                condition,
                duration,
            } => Self::ApplyConditionForTurns {
                condition,
                duration,
            },
            Fields::RemoveCondition { condition } => Self::RemoveCondition { condition },
            Fields::Restore { resource, amount } => Self::Restore { resource, amount },
            Fields::SearchAdjacentGuises {} => Self::SearchAdjacentGuises,
            Fields::ObservePublicPlan {} => Self::ObservePublicPlan,
            Fields::ApplyQualifiedConditionToSource {
                condition,
                duration,
                ability_tag,
            } => Self::ApplyQualifiedConditionToSource {
                condition,
                duration,
                ability_tag,
            },
        })
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ConditionDef {
    /// Pure, temporary current-floor monster-chart capability.
    pub detecting_monsters: bool,
    /// Opt-in pure, target-wide temporary visual disorientation.
    pub hallucinating: bool,
    pub id: ConditionId,
    pub family: ConditionFamilyId,
    pub lifecycle: ConditionLifecycle,
    pub name: String,
    pub modifiers: Vec<Modifier>,
    pub stacking: Stacking,
    pub source_scoped: bool,
    pub descriptors: Vec<DescriptorId>,
    pub capacity: Eviction,
    pub timing: ConditionTiming,
    pub restrictions: Vec<ConditionRestriction>,
    pub preparation: ConditionPreparation,
}
impl ConditionDef {
    pub(crate) fn same_mechanics(&self, other: &Self) -> bool {
        // Display text may change without reinterpreting a live application.
        let mut previous = self.clone();
        previous.name.clone_from(&other.name);
        previous == *other
    }
    /// Only ordinary outgoing tokens can carry an opponent/tag qualification.
    pub(crate) fn supports_qualification(&self) -> bool {
        self.lifecycle == ConditionLifecycle::NextCheck && !self.source_scoped
    }
    /// These entries must survive capacity pressure. Their typed operations do
    /// not participate in the fallible derived-stat fold; ordinary Keep sources
    /// can still carry modifiers and retain the normal eviction policy.
    pub(crate) fn protected_lifecycle(&self) -> bool {
        !self.restrictions.is_empty()
            || self.preparation == ConditionPreparation::Pause
            || self.timing.protection().is_some()
    }
}
impl<'de> Deserialize<'de> for ConditionDef {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            #[serde(default)]
            detecting_monsters: bool,
            #[serde(default)]
            hallucinating: bool,
            id: ConditionId,
            family: ConditionFamilyId,
            lifecycle: ConditionLifecycle,
            name: String,
            modifiers: Vec<Modifier>,
            stacking: Stacking,
            source_scoped: bool,
            descriptors: Vec<DescriptorId>,
            capacity: Eviction,
            timing: ConditionTiming,
            restrictions: crate::bounded::Bounded<ConditionRestriction, 3>,
            preparation: ConditionPreparation,
        }
        let fields: Fields = crate::bounded::named_map(deserializer)?;
        Ok(Self {
            detecting_monsters: fields.detecting_monsters,
            hallucinating: fields.hallucinating,
            id: fields.id,
            family: fields.family,
            lifecycle: fields.lifecycle,
            name: fields.name,
            modifiers: fields.modifiers,
            stacking: fields.stacking,
            source_scoped: fields.source_scoped,
            descriptors: fields.descriptors,
            capacity: fields.capacity,
            timing: fields.timing,
            restrictions: fields.restrictions.0,
            preparation: fields.preparation,
        })
    }
}

/// Positive, bounded actor-active duration. No wall-clock or region catch-up.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct ConditionTicks(u64);
impl ConditionTicks {
    pub fn new(ticks: u64) -> Result<Self, crate::conditions::ConditionError> {
        if ticks == 0 || ticks > crate::limits::NUMERIC as u64 {
            Err(crate::conditions::ConditionError::InvalidDuration)
        } else {
            Ok(Self(ticks))
        }
    }
    pub fn get(self) -> u64 {
        self.0
    }
}
impl<'de> Deserialize<'de> for ConditionTicks {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::new(u64::deserialize(deserializer)?)
            .map_err(|_| serde::de::Error::custom("Invalid active condition duration"))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ConditionTiming {
    /// No clock advances this condition; only an explicit ending removes it.
    UntilRemoved,
    /// Explicit Timed+Keep opt-in: finite actor-active removal or no expiry.
    ActorActiveOrUntilRemoved,
    ActorActive {
        on_expiry: ConditionExpiry,
    },
}
impl ConditionTiming {
    pub const fn expiring() -> Self {
        Self::ActorActive {
            on_expiry: ConditionExpiry::Remove,
        }
    }
    pub const fn permits_finite(self) -> bool {
        !matches!(self, Self::UntilRemoved)
    }
    pub const fn permits_persistent(self) -> bool {
        matches!(self, Self::UntilRemoved | Self::ActorActiveOrUntilRemoved)
    }
    pub(crate) fn protection(self) -> Option<ConditionTicks> {
        match self {
            Self::UntilRemoved
            | Self::ActorActiveOrUntilRemoved
            | Self::ActorActive {
                on_expiry: ConditionExpiry::Remove,
            } => None,
            Self::ActorActive {
                on_expiry: ConditionExpiry::Protect { duration },
            } => Some(duration),
        }
    }
}
impl<'de> Deserialize<'de> for ConditionTiming {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            UntilRemoved {},
            ActorActiveOrUntilRemoved {},
            ActorActive { on_expiry: ConditionExpiry },
        }
        Ok(match crate::bounded::named_map(deserializer)? {
            Fields::UntilRemoved {} => Self::UntilRemoved,
            Fields::ActorActiveOrUntilRemoved {} => Self::ActorActiveOrUntilRemoved,
            Fields::ActorActive { on_expiry } => Self::ActorActive { on_expiry },
        })
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ConditionExpiry {
    Remove,
    Protect { duration: ConditionTicks },
}
impl<'de> Deserialize<'de> for ConditionExpiry {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            Remove {},
            Protect { duration: ConditionTicks },
        }
        Ok(match crate::bounded::named_map(deserializer)? {
            Fields::Remove {} => Self::Remove,
            Fields::Protect { duration } => Self::Protect { duration },
        })
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConditionRestriction {
    VoluntaryActions,
    OrdinaryMovement,
    MovementAbilities,
}
impl<'de> Deserialize<'de> for ConditionRestriction {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match String::deserialize(deserializer)?.as_str() {
            "voluntary_actions" => Ok(Self::VoluntaryActions),
            "ordinary_movement" => Ok(Self::OrdinaryMovement),
            "movement_abilities" => Ok(Self::MovementAbilities),
            _ => Err(serde::de::Error::custom("Unknown condition restriction")),
        }
    }
}
impl ConditionRestriction {
    pub(crate) fn blocks(self, action: &crate::Action) -> bool {
        match self {
            Self::VoluntaryActions => true,
            Self::OrdinaryMovement => matches!(action, crate::Action::Move(_)),
            Self::MovementAbilities => matches!(
                action,
                crate::Action::UseAbility {
                    invocation: crate::abilities::AbilityInvocation {
                        aim: crate::abilities::AbilityAim::SelfStep { .. },
                        ..
                    },
                    ..
                }
            ),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConditionPreparation {
    Preserve,
    Pause,
}
impl<'de> Deserialize<'de> for ConditionPreparation {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match String::deserialize(deserializer)?.as_str() {
            "preserve" => Ok(Self::Preserve),
            "pause" => Ok(Self::Pause),
            _ => Err(serde::de::Error::custom(
                "Unknown condition preparation response",
            )),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConditionLifecycle {
    Timed,
    NextCheck,
    NextCheckAgainst,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Stacking {
    Keep,
    Refresh,
    Longest,
    /// Add another application's duration, bounded by the ordinary timer limit.
    Extend,
    Stack {
        maximum: u16,
    },
}
impl<'de> Deserialize<'de> for Stacking {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            Keep {},
            Refresh {},
            Longest {},
            Extend {},
            Stack { maximum: u16 },
        }
        Ok(match crate::bounded::named_map(deserializer)? {
            Fields::Keep {} => Self::Keep,
            Fields::Refresh {} => Self::Refresh,
            Fields::Longest {} => Self::Longest,
            Fields::Extend {} => Self::Extend,
            Fields::Stack { maximum } => Self::Stack { maximum },
        })
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Eviction {
    Oldest,
    Reject,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RulesetError {
    InvalidId(String),
    DuplicateId(String),
    UnknownReference { category: &'static str, id: String },
    Bound { field: String },
    Cycle { category: &'static str, id: String },
    DuplicateResource,
    MissingResource,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ruleset {
    document: RulesetDocument,
    talent_order: Vec<TalentId>,
    indexes: Indexes,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Indexes {
    supplies: BTreeMap<SupplyId, usize>,
    abilities: BTreeMap<AbilityId, usize>,
    forms: BTreeMap<FormId, usize>,
    ability_tags: BTreeMap<AbilityTagId, usize>,
    tag_members: BTreeMap<AbilityTagId, Vec<AbilityId>>,
    talents: BTreeMap<TalentId, usize>,
    expertises: BTreeMap<ExpertiseId, usize>,
    creature_types: BTreeMap<TypeId, usize>,
    subtypes: BTreeMap<SubtypeId, usize>,
    classes: BTreeMap<ClassId, usize>,
    conditions: BTreeMap<ConditionId, usize>,
    flags: BTreeMap<FlagId, usize>,
    descriptors: BTreeMap<DescriptorId, usize>,
    equipment_slots: BTreeMap<EquipmentSlot, usize>,
    point_pools: BTreeMap<PointPoolId, usize>,
}
impl Ruleset {
    pub fn document(&self) -> &RulesetDocument {
        &self.document
    }
    pub fn id(&self) -> &RulesetId {
        &self.document.id
    }
    pub fn constants(&self) -> &Constants {
        &self.document.constants
    }
    pub fn talent_order(&self) -> &[TalentId] {
        &self.talent_order
    }
    pub fn ability(&self, id: &AbilityId) -> Option<&AbilityDef> {
        self.indexes
            .abilities
            .get(id)
            .and_then(|&index| self.document.abilities.get(index))
    }
    pub fn supply(&self, id: &SupplyId) -> Option<&SupplyDef> {
        self.indexes
            .supplies
            .get(id)
            .and_then(|&index| self.document.supplies.get(index))
    }
    pub fn ability_tag(&self, id: &AbilityTagId) -> Option<&AbilityTagDef> {
        self.indexes
            .ability_tags
            .get(id)
            .and_then(|&index| self.document.ability_tags.get(index))
    }
    /// Membership is compiled once; modifier folds do not rescan the catalog.
    pub fn abilities_with_tag(&self, id: &AbilityTagId) -> Option<&[AbilityId]> {
        self.indexes.tag_members.get(id).map(Vec::as_slice)
    }
    pub fn expertise(&self, id: &ExpertiseId) -> Option<&crate::expertise::ExpertiseDef> {
        self.indexes
            .expertises
            .get(id)
            .and_then(|&index| self.document.expertises.get(index))
    }
    pub fn talent(&self, id: &TalentId) -> Option<&TalentDef> {
        self.indexes
            .talents
            .get(id)
            .and_then(|&index| self.document.talents.get(index))
    }
    pub fn form(&self, id: &FormId) -> Option<&FormDef> {
        self.indexes
            .forms
            .get(id)
            .and_then(|&index| self.document.forms.get(index))
    }
    pub fn creature_type(&self, id: &TypeId) -> Option<&TypeDef> {
        self.indexes
            .creature_types
            .get(id)
            .and_then(|&index| self.document.creature_types.get(index))
    }
    pub fn subtype(&self, id: &SubtypeId) -> Option<&SubtypeDef> {
        self.indexes
            .subtypes
            .get(id)
            .and_then(|&index| self.document.subtypes.get(index))
    }
    pub fn class(&self, id: &ClassId) -> Option<&ClassDef> {
        self.indexes
            .classes
            .get(id)
            .and_then(|&index| self.document.classes.get(index))
    }
    pub fn condition(&self, id: &ConditionId) -> Option<&ConditionDef> {
        self.indexes
            .conditions
            .get(id)
            .and_then(|&index| self.document.conditions.get(index))
    }
    /// Indexed immutable content lookup; inspection never scans descriptor catalogs.
    pub fn descriptor(&self, id: &DescriptorId) -> Option<&NamedDescriptor> {
        crate::diagnostics::descriptor_lookup();
        self.indexes
            .descriptors
            .get(id)
            .and_then(|&index| self.document.descriptors.get(index))
    }
    /// One payload has one optional resistance roll, never an order-dependent
    /// conjunction of descriptor checks. Immunity is still checked separately.
    pub fn validate_payload_descriptors(
        &self,
        descriptors: &[DescriptorId],
    ) -> Result<(), RulesetError> {
        bound(
            descriptors.len() <= crate::limits::DESCRIPTORS_PER_EFFECT,
            "descriptors",
        )?;
        let mut seen = BTreeSet::new();
        let mut resisted = 0;
        for id in descriptors {
            if !seen.insert(id) {
                return Err(RulesetError::DuplicateId(id.to_string()));
            }
            let definition = self
                .descriptor(id)
                .ok_or_else(|| RulesetError::UnknownReference {
                    category: "descriptor",
                    id: id.to_string(),
                })?;
            resisted += usize::from(definition.resistance.is_some());
        }
        bound(resisted <= 1, "multiple_payload_resistances")
    }
    pub fn descriptor_resistance(
        &self,
        descriptors: &[DescriptorId],
    ) -> Result<Option<DescriptorResistance>, RulesetError> {
        self.validate_payload_descriptors(descriptors)?;
        Ok(descriptors
            .iter()
            .find_map(|id| self.descriptor(id).and_then(|d| d.resistance)))
    }
    /// Resistance receipts describe the payload target. Source utilities have
    /// no hostile recipient and cannot borrow the selected target's skill.
    pub fn validate_payload_recipients(
        &self,
        descriptors: &[DescriptorId],
        recipients: impl IntoIterator<Item = EffectRecipient>,
    ) -> Result<(), RulesetError> {
        let policy = self.descriptor_resistance(descriptors)?;
        bound(
            policy.is_none()
                || recipients
                    .into_iter()
                    .all(|role| role == EffectRecipient::Target),
            "resistance_target_payload",
        )
    }
    pub fn flag(&self, id: &FlagId) -> Option<&FlagDef> {
        self.indexes
            .flags
            .get(id)
            .and_then(|&index| self.document.flags.get(index))
    }
    pub fn equipment_slot(&self, id: &EquipmentSlot) -> Option<&EquipmentSlotDef> {
        self.indexes
            .equipment_slots
            .get(id)
            .and_then(|&index| self.document.equipment_slots.get(index))
    }
    pub fn point_pool(&self, id: &PointPoolId) -> Option<&PointPoolDef> {
        self.indexes
            .point_pools
            .get(id)
            .and_then(|&index| self.document.point_pools.get(index))
    }
    pub fn point_grants(&self, source: &crate::progression::HdSource) -> Option<&[PointGrant]> {
        match source {
            crate::progression::HdSource::Racial => Some(&self.constants().racial_point_grants),
            crate::progression::HdSource::Class(id) => {
                self.class(id).map(|class| class.point_grants.as_slice())
            }
        }
    }
    pub fn validate(mut document: RulesetDocument) -> Result<Arc<Self>, RulesetError> {
        let mut pool = Interner::default();
        macro_rules! catalog {
            ($field:ident) => {{
                bound(
                    document.$field.len() <= crate::limits::CATALOG_ENTRIES,
                    stringify!($field),
                )?;
                let mut seen = BTreeSet::new();
                for def in &mut document.$field {
                    check_id(def.id.as_str())?;
                    if !seen.insert(def.id.clone()) {
                        return Err(RulesetError::DuplicateId(def.id.to_string()));
                    }
                    bound(
                        !def.name.trim().is_empty()
                            && def.name.len() <= crate::limits::DISPLAY_NAME_BYTES
                            && !def.name.chars().any(char::is_control),
                        "display_name",
                    )?;
                    def.id.intern(&mut pool);
                }
                seen
            }};
        }
        check_id(document.id.as_str())?;
        document.id.intern(&mut pool);
        let point_pools = catalog!(point_pools);
        for definition in &document.point_pools {
            let unique: BTreeSet<_> = definition.allowed.iter().collect();
            bound(
                !unique.is_empty() && unique.len() == definition.allowed.len(),
                "point_pool_kinds",
            )?;
        }
        let abilities = catalog!(abilities);
        let forms = catalog!(forms);
        for form in &mut document.forms {
            reference(&abilities, &form.ability, "ability")?;
            form.ability.intern(&mut pool);
        }
        let supplies = catalog!(supplies);
        let ability_tags = catalog!(ability_tags);
        let talents = catalog!(talents);
        let expertises = catalog!(expertises);
        let types = catalog!(creature_types);
        let subtypes = catalog!(subtypes);
        let classes = catalog!(classes);
        let conditions = catalog!(conditions);
        hallucination::validate_hallucination_conditions(&document.conditions)?;
        crate::detection_lifecycle::validate_conditions(&document.conditions)?;
        let condition_definitions = document.conditions.clone();
        let persistent_conditions = document
            .conditions
            .iter()
            .filter(|condition| condition.timing.permits_persistent())
            .map(|condition| condition.id.clone())
            .collect();
        let qualified_conditions = document
            .conditions
            .iter()
            .filter(|condition| condition.supports_qualification())
            .map(|condition| condition.id.clone())
            .collect();
        let descriptors = catalog!(descriptors);
        for descriptor in &document.descriptors {
            bound(
                descriptor.resistance.is_none_or(|policy| {
                    (0..=crate::limits::NUMERIC).contains(&i64::from(policy.difficulty))
                }),
                "descriptor_resistance",
            )?;
        }
        let flags = catalog!(flags);
        let equipment_slots = catalog!(equipment_slots);
        let equipment_slot_definitions = document
            .equipment_slots
            .iter()
            .map(|slot| (slot.id.clone(), slot))
            .collect();
        let refs = References {
            abilities: &abilities,
            ability_tags: &ability_tags,
            types: &types,
            subtypes: &subtypes,
            talents: &talents,
            classes: &classes,
            conditions: &conditions,
            condition_definitions: &condition_definitions,
            qualified_conditions: &qualified_conditions,
            persistent_conditions: &persistent_conditions,
            descriptors: &descriptors,
            flags: &flags,
            equipment_slots: &equipment_slot_definitions,
        };
        let c = &mut document.constants;
        bound(
            (1..=crate::limits::NUMERIC as u64).contains(&c.item_timing.consume_turns),
            "item_timing",
        )?;
        bound(
            c.default_anatomy.slots.len() <= crate::limits::EQUIPMENT_SOCKETS,
            "default_anatomy",
        )?;
        for id in &mut c.default_anatomy.slots {
            reference(&equipment_slots, id, "equipment_slot")?;
            id.intern(&mut pool);
        }
        bound(
            c.default_anatomy
                .valid_with(|id| equipment_slot_definitions.get(id).copied()),
            "default_anatomy",
        )?;
        c.default_anatomy.capacities.intern(&mut pool);
        for slot in &document.equipment_slots {
            bound(slot.capacity.valid(), "equipment_capacity")?;
            bound(
                !slot.equipped_phrase.trim().is_empty()
                    && slot.equipped_phrase.len() <= crate::limits::DISPLAY_NAME_BYTES
                    && !slot.equipped_phrase.chars().any(char::is_control),
                "equipment_phrase",
            )?;
            let classes: BTreeSet<_> = slot.compatible_classes.iter().collect();
            bound(
                !classes.is_empty() && classes.len() == slot.compatible_classes.len(),
                "equipment_classes",
            )?;
            for turns in [slot.equip_turns, slot.unequip_turns] {
                bound(
                    (1..=crate::limits::NUMERIC as u64).contains(&turns),
                    "equipment_timing",
                )?;
            }
        }

        bound(
            (-crate::limits::STAT_MAGNITUDE..=crate::limits::STAT_MAGNITUDE)
                .contains(&i64::from(c.base_defense)),
            "base_defense",
        )?;
        bound(
            c.attribute_cap > 0
                && c.attribute_cap <= c.derived_attribute_cap
                && c.derived_attribute_cap <= crate::limits::DERIVED_ATTRIBUTE,
            "attribute_caps",
        )?;
        bound(c.attribute_opportunity_interval > 0, "advancement")?;
        validate_point_grants(&mut c.racial_point_grants, &point_pools, &mut pool)?;
        bound(
            c.duration_scale_numerator > 0 && c.duration_scale_offset > 0,
            "duration_scale",
        )?;
        for id in &mut c.default_abilities {
            reference(&abilities, id, "ability")?;
            id.intern(&mut pool);
        }
        bound(document.nutrition.valid(), "nutrition")?;
        bound(
            document.nutrition_condition_trigger.valid(&document),
            "nutrition_condition_trigger",
        )?;
        let recovery = &document.health_recovery;
        document.advancement.validate()?;
        document.growth.validate()?;
        reference(
            &supplies,
            &document.advancement.respec_ration.supply,
            "supply",
        )?;
        document.advancement.respec_ration.supply.intern(&mut pool);
        bound(
            (1..=crate::limits::NUMERIC as u64).contains(&recovery.base_period)
                && (1..=recovery.base_period).contains(&recovery.minimum_period)
                && recovery.reduction_per_hd <= crate::limits::NUMERIC as u64,
            "health_recovery",
        )?;
        let mut resources = Vec::new();
        for d in &mut document.resources {
            bound(
                i64::from(d.capacity.base) <= crate::limits::NUMERIC,
                "resource_capacity",
            )?;
            if let Some(flag) = &mut d.capacity.required_flag {
                reference(&flags, flag, "flag")?;
                flag.intern(&mut pool);
            }
            if resources.contains(&d.resource) {
                return Err(RulesetError::DuplicateResource);
            }
            resources.push(d.resource);
            bound(
                d.recovery_period > 0 && d.recovery_period <= crate::limits::NUMERIC as u64,
                "recovery_period",
            )?;
        }
        if Resource::ALL.iter().any(|r| !resources.contains(r)) {
            return Err(RulesetError::MissingResource);
        }
        for d in &mut document.creature_types {
            health_die(d.health_die)?;
            refs.modifiers(&mut d.modifiers, &mut pool)?;
        }
        for d in &mut document.subtypes {
            refs.modifiers(&mut d.modifiers, &mut pool)?;
        }
        for d in &mut document.classes {
            health_die(d.health_die)?;
            validate_point_grants(&mut d.point_grants, &point_pools, &mut pool)?;
            refs.modifiers(&mut d.modifiers, &mut pool)?;
        }
        for d in &mut document.flags {
            refs.modifiers(&mut d.modifiers, &mut pool)?;
        }
        for d in &mut document.talents {
            bound(
                d.expertise_grants.len() <= crate::limits::MODIFIERS_PER_SOURCE,
                "talent_expertise_grants",
            )?;
            let mut seen_expertises = BTreeSet::new();
            for expertise in &mut d.expertise_grants {
                reference(&expertises, expertise, "expertise")?;
                if !seen_expertises.insert(expertise.clone()) {
                    return Err(RulesetError::DuplicateId(expertise.to_string()));
                }
                expertise.intern(&mut pool);
            }
            bound(
                d.form_choices.len() <= crate::limits::FORMS_PER_TALENT,
                "form_choices",
            )?;
            bound(
                d.form_choices.is_empty()
                    || d.modifiers.len() < crate::limits::MODIFIERS_PER_SOURCE,
                "form_grant_capacity",
            )?;
            let mut chosen = BTreeSet::new();
            for form in &mut d.form_choices {
                reference(&forms, form, "form")?;
                if !chosen.insert(form.clone()) {
                    return Err(RulesetError::DuplicateId(form.to_string()));
                }
                form.intern(&mut pool);
            }
            bound(
                d.prerequisites.len() <= crate::limits::MODIFIERS_PER_SOURCE,
                "prerequisites",
            )?;
            let mut seen = BTreeSet::new();
            for id in &mut d.prerequisites {
                reference(&talents, id, "talent")?;
                if !seen.insert(id.clone()) {
                    return Err(RulesetError::DuplicateId(id.to_string()));
                }
                id.intern(&mut pool);
            }
            refs.requirement(&mut d.requirement, &mut pool, 0, &mut 0)?;
            refs.modifiers(&mut d.modifiers, &mut pool)?;
        }
        for d in &mut document.conditions {
            d.family.intern(&mut pool);
            bound(
                !d.timing.permits_persistent()
                    || (d.lifecycle == ConditionLifecycle::Timed && d.stacking == Stacking::Keep),
                "persistent_condition_shape",
            )?;
            let unique: BTreeSet<_> = d.restrictions.iter().copied().collect();
            bound(
                d.restrictions.len() <= 3 && unique.len() == d.restrictions.len(),
                "condition_restrictions",
            )?;
            d.restrictions.sort();
            bound(
                d.preparation != ConditionPreparation::Pause || !d.restrictions.is_empty(),
                "condition_pause_scope",
            )?;
            // Protected operations cannot be removed by capacity/reconciliation
            // pressure. Only the bounded confusion probability is also safe:
            // it cannot invalidate numeric stats, reach, grants or interactions.
            // Other modifier folds still require an ordinary evictable source.
            // Restrictions without follow-up protection can extend their bounded
            // duration; protection-bearing entries remain nonrefreshing.
            bound(
                !d.protected_lifecycle()
                    || (d.lifecycle == ConditionLifecycle::Timed
                        && !d.source_scoped
                        && (d.stacking == Stacking::Keep
                            || (d.stacking == Stacking::Extend
                                && d.timing.protection().is_none()))
                        && d.modifiers.iter().all(|modifier| {
                            matches!(
                                modifier,
                                Modifier {
                                    scope: Scope::Always,
                                    op: ModifierOp::Locomotion {
                                        value: crate::locomotion::LocomotionValue::Confused { .. }
                                    },
                                    ..
                                }
                            )
                        })),
                "protected_condition_shape",
            )?;
            bound(
                d.lifecycle == ConditionLifecycle::Timed
                    || (!d.source_scoped && !matches!(d.stacking, Stacking::Stack { .. })),
                "token_shape",
            )?;
            refs.modifiers(&mut d.modifiers, &mut pool)?;
            if d.lifecycle != ConditionLifecycle::Timed {
                bound(
                    !d.modifiers.is_empty()
                        && d.modifiers.iter().all(|modifier| {
                            modifier.scope == Scope::Always
                                && matches!(
                                    (&d.lifecycle, &modifier.op),
                                    (
                                        ConditionLifecycle::NextCheck,
                                        ModifierOp::Add {
                                            stat: Stat::CheckEdge,
                                            value: -1 | 1
                                        }
                                    ) | (
                                        ConditionLifecycle::NextCheckAgainst,
                                        ModifierOp::Add {
                                            stat: Stat::IncomingEdge { defense: None },
                                            value: -1 | 1
                                        }
                                    )
                                )
                        }),
                    "token_modifiers",
                )?;
            }
            if let Stacking::Stack { maximum } = d.stacking {
                bound(
                    maximum > 0
                        && usize::from(maximum) <= crate::limits::CONDITIONS
                        && usize::from(maximum).saturating_mul(d.modifiers.len())
                            <= crate::limits::MODIFIERS_PER_SOURCE,
                    "condition_stack",
                )?;
            }
            bound(
                d.source_scoped || d.modifiers.iter().all(|m| m.scope != Scope::VersusSource),
                "condition_scope",
            )?;
            refs.descriptors(&mut d.descriptors, &mut pool)?;
        }
        let mut families = BTreeMap::new();
        for definition in &document.conditions {
            if let Some(previous) = families.insert(&definition.family, definition.lifecycle) {
                bound(
                    previous == ConditionLifecycle::Timed
                        && definition.lifecycle == ConditionLifecycle::Timed,
                    "token_family",
                )?;
            }
        }
        bound(
            document.abilities.iter().filter(|d| d.basic_attack).count() <= 1,
            "basic_attack",
        )?;
        for d in &mut document.abilities {
            bound(d.spatial_shape_valid(), "ability_spatial_shape")?;
            let detects = match &d.strikes {
                StrikeSource::Authored { strikes } => strikes
                    .iter()
                    .flat_map(|strike| {
                        strike
                            .on_success
                            .iter()
                            .chain(&strike.on_failure)
                            .chain(strike.riders.iter().flat_map(|rider| &rider.effects))
                    })
                    .any(crate::detection_lifecycle::is_effect),
                StrikeSource::NaturalMelee { additional_riders } => additional_riders
                    .iter()
                    .flat_map(|rider| &rider.effects)
                    .any(crate::detection_lifecycle::is_effect),
                StrikeSource::SelectedMelee | StrikeSource::Projectile { .. } => false,
            };
            bound(
                !detects
                    || (d.invocation == crate::abilities::InvocationKind::ActorTarget
                        && d.reach == Reach::SelfOnly
                        && d.interaction == Interaction::NonHostile
                        && matches!(&d.strikes, StrikeSource::Authored { strikes }
                    if matches!(strikes.as_slice(), [StrikeDef { offset:0, unlock:None, check:None,
                        on_success, on_failure, riders, .. }] if on_success.len() == 1
                            && on_success.iter().all(crate::detection_lifecycle::is_effect)
                            && on_failure.is_empty() && riders.is_empty()))),
                "detection_self_only",
            )?;
            let lights = match &d.strikes {
                StrikeSource::Authored { strikes } => strikes.iter().any(|strike| {
                    strike
                        .on_success
                        .iter()
                        .chain(&strike.on_failure)
                        .chain(strike.riders.iter().flat_map(|rider| &rider.effects))
                        .any(|effect| matches!(effect, Effect::RoomLight))
                }),
                StrikeSource::NaturalMelee { additional_riders } => additional_riders
                    .iter()
                    .flat_map(|rider| &rider.effects)
                    .any(|effect| matches!(effect, Effect::RoomLight)),
                StrikeSource::SelectedMelee | StrikeSource::Projectile { .. } => false,
            };
            bound(
                !lights
                    || (d.invocation == crate::abilities::InvocationKind::ActorTarget
                        && d.reach == Reach::SelfOnly
                        && d.interaction == Interaction::NonHostile),
                "room_light_self_only",
            )?;
            bound(
                !lights
                    || matches!(&d.strikes, StrikeSource::Authored { strikes }
                if matches!(strikes.as_slice(), [StrikeDef { offset: 0, unlock: None, check: None, on_failure, riders, .. }]
                    if on_failure.is_empty() && riders.is_empty())),
                "room_light_checkless",
            )?;
            let drains = match &d.strikes {
                StrikeSource::Authored { strikes } => strikes
                    .iter()
                    .flat_map(|s| {
                        s.on_success
                            .iter()
                            .chain(&s.on_failure)
                            .chain(s.riders.iter().flat_map(|r| &r.effects))
                    })
                    .any(|effect| matches!(effect, Effect::DrainLife)),
                StrikeSource::NaturalMelee { additional_riders } => additional_riders
                    .iter()
                    .flat_map(|r| &r.effects)
                    .any(|effect| matches!(effect, Effect::DrainLife)),
                StrikeSource::SelectedMelee | StrikeSource::Projectile { .. } => false,
            };
            bound(
                !drains
                    || (d.invocation == crate::abilities::InvocationKind::ActorTarget
                        && d.reach == Reach::SelfOnly
                        && d.interaction == Interaction::NonHostile
                        && matches!(&d.strikes, StrikeSource::Authored { strikes }
                    if matches!(strikes.as_slice(), [StrikeDef { offset:0, unlock:None, check:None, on_success, on_failure, riders, .. }]
                        if matches!(on_success.as_slice(), [Effect::DrainLife]) && on_failure.is_empty() && riders.is_empty()))),
                "drain_life_self_only",
            )?;
            // Haste overdose is a self-consumption consequence, not a route
            // around direct-harm validation for targeted restrictions. Keep the
            // dose in the existing ordered checkless authored self payload.
            let haste_dose = match &d.strikes {
                StrikeSource::Authored { strikes } => strikes
                    .iter()
                    .flat_map(|s| {
                        s.on_success
                            .iter()
                            .chain(&s.on_failure)
                            .chain(s.riders.iter().flat_map(|r| &r.effects))
                    })
                    .any(|effect| matches!(effect, Effect::HasteDose { .. })),
                StrikeSource::NaturalMelee { additional_riders } => additional_riders
                    .iter()
                    .flat_map(|r| &r.effects)
                    .any(|effect| matches!(effect, Effect::HasteDose { .. })),
                StrikeSource::SelectedMelee | StrikeSource::Projectile { .. } => false,
            };
            bound(
                !haste_dose
                    || (d.invocation == crate::abilities::InvocationKind::ActorTarget
                        && d.reach == Reach::SelfOnly
                        && d.interaction == Interaction::NonHostile
                        && matches!(&d.strikes, StrikeSource::Authored { strikes }
                    if matches!(strikes.as_slice(), [StrikeDef { offset:0, unlock:None, check:None, on_success, on_failure, riders, .. }]
                        if on_success.iter().any(|effect| matches!(effect, Effect::HasteDose { .. })) && on_failure.is_empty() && riders.is_empty()))),
                "haste_dose_self_only",
            )?;
            let searches = match &d.strikes {
                StrikeSource::Authored { strikes } => strikes.iter().any(|strike| {
                    strike
                        .on_success
                        .iter()
                        .chain(&strike.on_failure)
                        .chain(strike.riders.iter().flat_map(|rider| &rider.effects))
                        .any(|effect| matches!(effect, Effect::SearchAdjacentGuises))
                }),
                StrikeSource::NaturalMelee { additional_riders } => additional_riders
                    .iter()
                    .flat_map(|rider| &rider.effects)
                    .any(|effect| matches!(effect, Effect::SearchAdjacentGuises)),
                StrikeSource::SelectedMelee | StrikeSource::Projectile { .. } => false,
            };
            bound(
                !searches
                    || (d.reach == Reach::SelfOnly && d.interaction == Interaction::NonHostile),
                "search_self_only",
            )?;
            bound(
                !searches
                    || matches!(&d.strikes, StrikeSource::Authored { strikes }
                if matches!(strikes.as_slice(), [StrikeDef { offset: 0, unlock: None, check: None, on_success, on_failure, riders, .. }]
                    if matches!(on_success.as_slice(), [Effect::SearchAdjacentGuises]) && on_failure.is_empty() && riders.is_empty())),
                "search_single_check",
            )?;
            if let StrikeSource::Authored { strikes } = &d.strikes {
                bound(
                    !strikes.iter().any(|strike| {
                        strike
                            .on_success
                            .iter()
                            .any(|effect| matches!(effect, Effect::HoldDamage { .. }))
                    }) || (d.reach == Reach::Melee
                        && d.interaction == Interaction::HostileToTarget),
                    "hold_damage_melee",
                )?;
            }
            if let TimingSource::SelectedWeaponAdjusted {
                preparation,
                recovery,
            } = d.timing.source
            {
                bound(
                    preparation <= crate::limits::NUMERIC as u64
                        && recovery <= crate::limits::NUMERIC as u64,
                    "timing_adjustment",
                )?;
            }
            bound(
                d.reach != Reach::UntilObstruction
                    || (d.invocation == crate::abilities::InvocationKind::DirectionalContactRay
                        && crate::abilities::ray_definition_shape_valid(d)),
                "until_obstruction_delivery",
            )?;
            let cancellation = match &d.strikes {
                StrikeSource::Authored { strikes } => strikes.iter().any(|strike| {
                    strike
                        .on_success
                        .iter()
                        .chain(&strike.on_failure)
                        .chain(strike.riders.iter().flat_map(|rider| &rider.effects))
                        .any(|effect| matches!(effect, Effect::CancelSpecials))
                }),
                StrikeSource::NaturalMelee { additional_riders } => additional_riders
                    .iter()
                    .flat_map(|rider| &rider.effects)
                    .any(|effect| matches!(effect, Effect::CancelSpecials)),
                _ => false,
            };
            bound(
                !cancellation
                    || (d.reach == Reach::UntilObstruction
                        && crate::abilities::ray_definition_shape_valid(d)),
                "cancellation_contact_source",
            )?;
            bound(
                d.tags.len() <= crate::limits::TAGS_PER_ABILITY,
                "ability_tags",
            )?;
            let mut seen = BTreeSet::new();
            for tag in &mut d.tags {
                reference(&ability_tags, tag, "ability_tag")?;
                if !seen.insert(tag.clone()) {
                    return Err(RulesetError::DuplicateId(tag.to_string()));
                }
                tag.intern(&mut pool);
            }
            bound(
                !d.basic_attack || !matches!(d.reach, Reach::SelfOnly | Reach::SelfOrAlly { .. }),
                "basic_attack_reach",
            )?;
            bound(d.ai.minimum_resource_percent <= 100, "ai_resource_percent")?;
            if let TimingSource::Fixed { prepare, recover } = d.timing.source {
                bound(
                    prepare > 0
                        && recover > 0
                        && prepare <= crate::limits::NUMERIC as u64
                        && recover <= crate::limits::NUMERIC as u64,
                    "timing",
                )?;
            }
            if let Some(cooldown) = d.cooldown {
                bound(cooldown.get() <= crate::limits::NUMERIC as u64, "cooldown")?;
            }
            bound(
                d.supply_costs.len() <= crate::limits::SUPPLY_COSTS,
                "supply_costs",
            )?;
            bound(
                d.supply_costs.windows(2).all(|pair| {
                    (pair[0].spend_at, &pair[0].supply) < (pair[1].spend_at, &pair[1].supply)
                }),
                "supply_cost_order",
            )?;
            for cost in &mut d.supply_costs {
                reference(&supplies, &cost.supply, "supply")?;
                cost.supply.intern(&mut pool);
                bound(
                    (1..=crate::limits::NUMERIC as u64).contains(&cost.quantity),
                    "supply_quantity",
                )?;
                bound(
                    cost.spend_at == SupplySpendAt::FirstResolution,
                    "ability_supply_stage",
                )?;
            }
            if let Some(cost) = &d.cost {
                bound(
                    u64::from(cost.start) + u64::from(cost.resolution)
                        <= crate::limits::NUMERIC as u64,
                    "cost",
                )?;
            }
            if let Reach::Sight { range }
            | Reach::MutualSight { range }
            | Reach::SelfOrAlly {
                range: AllyRange::Sight { range },
            } = d.reach
            {
                bound(range > 0 && range <= crate::limits::SIGHT_RANGE, "range")?;
            }
            match &mut d.strikes {
                StrikeSource::Authored { strikes } => {
                    refs.strikes(strikes, &mut pool)?;
                    if let TimingSource::Fixed { prepare, recover } = d.timing.source {
                        validate_strike_horizon(strikes, prepare, recover)?;
                    }
                }
                StrikeSource::NaturalMelee { additional_riders } => {
                    refs.riders(additional_riders, &mut pool)?;
                }
                StrikeSource::SelectedMelee | StrikeSource::Projectile { .. } => {}
            }
        }
        let talent_order = topological(
            document
                .talents
                .iter()
                .map(|d| {
                    let mut dependencies = d.prerequisites.clone();
                    d.requirement.talent_dependencies(&mut dependencies);
                    dependencies.sort();
                    dependencies.dedup();
                    (d.id.clone(), dependencies)
                })
                .collect(),
            "talent",
        )?;
        let flag_graph = document
            .flags
            .iter()
            .map(|d| {
                (
                    d.id.clone(),
                    d.modifiers
                        .iter()
                        .filter_map(|m| match &m.op {
                            ModifierOp::Flag { flag } => Some(flag.clone()),
                            ModifierOp::Add { .. }
                            | ModifierOp::Set { .. }
                            | ModifierOp::Immunity { .. }
                            | ModifierOp::Reduction { .. }
                            | ModifierOp::TalentReduction { .. }
                            | ModifierOp::SignedReduction { .. }
                            | ModifierOp::AttributeWithoutDefense { .. }
                            | ModifierOp::SustainAttribute { .. }
                            | ModifierOp::ProtectArmor {}
                            | ModifierOp::HealthRecoveryRate { .. }
                            | ModifierOp::GrantAbility { .. }
                            | ModifierOp::ActionDuration { .. }
                            | ModifierOp::Locomotion { .. }
                            | ModifierOp::Perception { .. } => None,
                        })
                        .collect(),
                )
            })
            .collect();
        topological(flag_graph, "flag")?;
        macro_rules! index {
            ($field:ident) => {
                document
                    .$field
                    .iter()
                    .enumerate()
                    .map(|(i, d)| (d.id.clone(), i))
                    .collect()
            };
        }
        let mut tag_members: BTreeMap<_, Vec<_>> = ability_tags
            .iter()
            .map(|tag| (tag.clone(), Vec::new()))
            .collect();
        for ability in &document.abilities {
            for tag in &ability.tags {
                tag_members
                    .entry(tag.clone())
                    .or_default()
                    .push(ability.id.clone());
            }
        }
        for members in tag_members.values_mut() {
            members.sort();
        }
        let indexes = Indexes {
            supplies: index!(supplies),
            abilities: index!(abilities),
            forms: index!(forms),
            ability_tags: index!(ability_tags),
            tag_members,
            talents: index!(talents),
            expertises: index!(expertises),
            creature_types: index!(creature_types),
            subtypes: index!(subtypes),
            classes: index!(classes),
            conditions: index!(conditions),
            flags: index!(flags),
            descriptors: index!(descriptors),
            equipment_slots: index!(equipment_slots),
            point_pools: index!(point_pools),
        };
        let rules = Self {
            document,
            talent_order,
            indexes,
        };
        for modifiers in rules
            .document
            .creature_types
            .iter()
            .map(|d| &d.modifiers)
            .chain(rules.document.subtypes.iter().map(|d| &d.modifiers))
            .chain(rules.document.classes.iter().map(|d| &d.modifiers))
            .chain(rules.document.talents.iter().map(|d| &d.modifiers))
            .chain(rules.document.flags.iter().map(|d| &d.modifiers))
        {
            bound(
                modifiers
                    .iter()
                    .all(|modifier| modifier.scope != Scope::VersusSource),
                "permanent_modifier_scope",
            )?;
            rules.expanded_modifier_count(modifiers)?;
            rules.validate_projectile_range_modifiers(modifiers)?;
        }
        for definition in &rules.document.talents {
            rules.validate_edge_source(&definition.modifiers)?;
        }
        for definition in &rules.document.conditions {
            rules.validate_edge_source(&definition.modifiers)?;
            rules.validate_projectile_range_modifiers(&definition.modifiers)?;
            let expanded = rules.expanded_modifier_count(&definition.modifiers)?;
            let stacks = match definition.stacking {
                Stacking::Stack { maximum } => usize::from(maximum),
                Stacking::Keep | Stacking::Refresh | Stacking::Longest | Stacking::Extend => 1,
            };
            bound(
                stacks.saturating_mul(expanded) <= crate::limits::MODIFIERS_PER_SOURCE,
                "condition_stack_expansion",
            )?;
        }
        for ability in &rules.document.abilities {
            match &ability.strikes {
                StrikeSource::Authored { strikes } => {
                    rules.validate_interaction(ability.interaction, strikes)?
                }
                StrikeSource::NaturalMelee { additional_riders } => {
                    rules.validate_rider_interaction(ability.interaction, additional_riders)?
                }
                StrikeSource::SelectedMelee | StrikeSource::Projectile { .. } => bound(
                    ability.interaction == Interaction::HostileToTarget,
                    "non_hostile_direct_harm",
                )?,
            }
        }
        Ok(Arc::new(rules))
    }
}
fn bound(ok: bool, field: &str) -> Result<(), RulesetError> {
    if ok {
        Ok(())
    } else {
        Err(RulesetError::Bound {
            field: field.into(),
        })
    }
}
fn check_id(id: &str) -> Result<(), RulesetError> {
    if valid_id(id) {
        Ok(())
    } else {
        Err(RulesetError::InvalidId(id.into()))
    }
}
fn health_die(sides: u16) -> Result<(), RulesetError> {
    bound(
        (2..=crate::limits::DICE_SIDES).contains(&sides),
        "health_die",
    )
}
fn reference<T: Ord + std::fmt::Display>(
    ids: &BTreeSet<T>,
    id: &T,
    category: &'static str,
) -> Result<(), RulesetError> {
    if ids.contains(id) {
        Ok(())
    } else {
        Err(RulesetError::UnknownReference {
            category,
            id: id.to_string(),
        })
    }
}
fn topological<T: Ord + Clone + std::fmt::Display>(
    mut graph: BTreeMap<T, Vec<T>>,
    category: &'static str,
) -> Result<Vec<T>, RulesetError> {
    let mut result = Vec::new();
    let mut completed = BTreeSet::new();
    while !graph.is_empty() {
        let next = graph
            .iter()
            .find(|(_, dependencies)| dependencies.iter().all(|id| completed.contains(id)))
            .map(|(id, _)| id.clone());
        let Some(next) = next else {
            return Err(RulesetError::Cycle {
                category,
                id: graph
                    .keys()
                    .next()
                    .map(ToString::to_string)
                    .unwrap_or_default(),
            });
        };
        graph.remove(&next);
        completed.insert(next.clone());
        result.push(next);
    }
    Ok(result)
}
struct References<'a> {
    abilities: &'a BTreeSet<AbilityId>,
    ability_tags: &'a BTreeSet<AbilityTagId>,
    types: &'a BTreeSet<TypeId>,
    subtypes: &'a BTreeSet<SubtypeId>,
    talents: &'a BTreeSet<TalentId>,
    classes: &'a BTreeSet<ClassId>,
    conditions: &'a BTreeSet<ConditionId>,
    condition_definitions: &'a [ConditionDef],
    qualified_conditions: &'a BTreeSet<ConditionId>,
    persistent_conditions: &'a BTreeSet<ConditionId>,
    descriptors: &'a BTreeSet<DescriptorId>,
    flags: &'a BTreeSet<FlagId>,
    equipment_slots: &'a BTreeMap<EquipmentSlot, &'a EquipmentSlotDef>,
}
impl References<'_> {
    fn equipment_slot(&self, id: &EquipmentSlot) -> Result<&EquipmentSlotDef, RulesetError> {
        self.equipment_slots
            .get(id)
            .copied()
            .ok_or_else(|| RulesetError::UnknownReference {
                category: "equipment_slot",
                id: id.to_string(),
            })
    }

    fn descriptors(
        &self,
        values: &mut [DescriptorId],
        pool: &mut Interner,
    ) -> Result<(), RulesetError> {
        bound(
            values.len() <= crate::limits::DESCRIPTORS_PER_EFFECT,
            "descriptors",
        )?;
        let mut seen = BTreeSet::new();
        for id in values {
            reference(self.descriptors, id, "descriptor")?;
            if !seen.insert(id.clone()) {
                return Err(RulesetError::DuplicateId(id.to_string()));
            }
            id.intern(pool);
        }
        Ok(())
    }
    fn selector(&self, selector: &mut Selector, pool: &mut Interner) -> Result<(), RulesetError> {
        if let Selector::Descriptor { descriptor } = selector {
            reference(self.descriptors, descriptor, "descriptor")?;
            descriptor.intern(pool);
        }
        Ok(())
    }
    fn modifiers(&self, values: &mut [Modifier], pool: &mut Interner) -> Result<(), RulesetError> {
        validate_modifier_shapes(values)?;
        for modifier in values {
            match &mut modifier.op {
                ModifierOp::Add { stat, .. } | ModifierOp::Set { stat, .. } => match stat {
                    Stat::Ability { ability, .. } => {
                        reference(self.abilities, ability, "ability")?;
                        ability.intern(pool);
                    }
                    Stat::AbilityTag { tag, .. } => {
                        reference(self.ability_tags, tag, "ability_tag")?;
                        tag.intern(pool);
                    }
                    Stat::Attribute { .. }
                    | Stat::Defense { .. }
                    | Stat::Health
                    | Stat::HealthRecoveryPeriod
                    | Stat::HealthRecoveryPeriodPercent
                    | Stat::HealthRecoveryResetPercent
                    | Stat::AttackDamageRollBonus
                    | Stat::NutritionRateThirtieths
                    | Stat::ResourceMax { .. }
                    | Stat::Edge
                    | Stat::CheckEdge
                    | Stat::IncomingEdge { .. } => {}
                },
                ModifierOp::Flag { flag } => {
                    reference(self.flags, flag, "flag")?;
                    flag.intern(pool);
                }
                ModifierOp::Immunity { selector } => self.selector(selector, pool)?,
                ModifierOp::Reduction { selector, .. }
                | ModifierOp::SignedReduction { selector, .. } => self.selector(selector, pool)?,
                ModifierOp::TalentReduction { .. }
                | ModifierOp::AttributeWithoutDefense { .. }
                | ModifierOp::SustainAttribute { .. }
                | ModifierOp::ProtectArmor {}
                | ModifierOp::HealthRecoveryRate { .. }
                | ModifierOp::ActionDuration { .. }
                | ModifierOp::Locomotion { .. }
                | ModifierOp::Perception { .. } => {}
                ModifierOp::GrantAbility { ability } => {
                    reference(self.abilities, ability, "ability")?;
                    ability.intern(pool);
                }
            }
        }
        Ok(())
    }
    fn requirement(
        &self,
        value: &mut Requirement,
        pool: &mut Interner,
        depth: usize,
        nodes: &mut usize,
    ) -> Result<(), RulesetError> {
        *nodes += 1;
        bound(
            depth <= crate::limits::REQUIREMENT_DEPTH && *nodes <= crate::limits::REQUIREMENT_NODES,
            "requirement_size",
        )?;
        match value {
            Requirement::All { requirements } | Requirement::Any { requirements } => {
                for child in requirements {
                    self.requirement(child, pool, depth + 1, nodes)?;
                }
            }
            Requirement::ClassLevel { class, .. } => {
                reference(self.classes, class, "class")?;
                class.intern(pool);
            }
            Requirement::TypeIn { types } => {
                bound(!types.is_empty(), "type_requirement")?;
                for id in types {
                    reference(self.types, id, "type")?;
                    id.intern(pool);
                }
            }
            Requirement::SubtypeIn { subtypes } => {
                bound(!subtypes.is_empty(), "subtype_requirement")?;
                bound(
                    subtypes.len() <= crate::limits::CATALOG_ENTRIES,
                    "subtype_requirement",
                )?;
                let mut seen = BTreeSet::new();
                for id in subtypes {
                    reference(self.subtypes, id, "subtype")?;
                    if !seen.insert(id.clone()) {
                        return Err(RulesetError::DuplicateId(id.to_string()));
                    }
                    id.intern(pool);
                }
            }
            Requirement::OwnedTalent { talent } => {
                reference(self.talents, talent, "talent")?;
                talent.intern(pool);
            }
            Requirement::Flag { flag, .. } | Requirement::SpeciesFlag { flag, .. } => {
                reference(self.flags, flag, "flag")?;
                flag.intern(pool);
            }
            Requirement::TotalHd { .. } | Requirement::RacialHd { .. } => {}
            Requirement::SkillRank { skill, minimum } => {
                bound(
                    !matches!(skill, SkillSelector::SelectedWeapon),
                    "permanent_skill_requirement",
                )?;
                bound(
                    i64::from(*minimum) <= crate::limits::STAT_MAGNITUDE,
                    "requirement_minimum",
                )?;
            }
            Requirement::AttributeMinimum { minimum, .. } => bound(
                *minimum <= crate::limits::DERIVED_ATTRIBUTE,
                "requirement_minimum",
            )?,
        }
        Ok(())
    }
    fn effects(&self, values: &mut [Effect], pool: &mut Interner) -> Result<(), RulesetError> {
        bound(values.len() <= crate::limits::EFFECTS_PER_BRANCH, "effects")?;
        bound(
            values.len() == 1
                || !values
                    .iter()
                    .any(|effect| matches!(effect, Effect::CorrodeArmor { .. })),
            "corrosion_isolated_payload",
        )?;
        bound(
            values.first().is_none_or(|first| {
                values
                    .iter()
                    .all(|effect| effect.recipient() == first.recipient())
            }),
            "mixed_effect_recipients",
        )?;
        for value in values {
            match value {
                Effect::Theft {
                    selection,
                    relocation,
                } => {
                    if let crate::theft::TheftSelection::Currency { amount, .. } = selection {
                        bound(amount.valid(), "theft_quantity")?;
                    }
                    if let Some(gate) = relocation {
                        reference(self.flags, &gate.flag, "flag")?;
                        gate.flag.intern(pool);
                    }
                }
                Effect::AlterEquipment {
                    selection,
                    alteration,
                } => {
                    bound(selection.accepts(*alteration), "equipment_alteration")?;
                    if let crate::EquippedSelection::Slot { slot, class, .. } = selection {
                        bound(
                            self.equipment_slot(slot)?
                                .compatible_classes
                                .contains(class),
                            "equipment_selection",
                        )?;
                        slot.intern(pool);
                    }
                }
                Effect::CorrodeArmor { slot, amount } => {
                    bound(
                        (1..=crate::limits::NUMERIC as u32).contains(amount),
                        "corrosion amount",
                    )?;
                    self.equipment_slot(slot)?;
                    slot.intern(pool);
                }
                Effect::ApplyLinkedHold { policy } => {
                    bound(policy.valid(), "hold_policy")?;
                    reference(self.flags, &policy.sustaining, "flag")?;
                    policy.sustaining.intern(pool);
                }
                Effect::HoldDamage { descriptor, .. } => {
                    if let Some(id) = descriptor {
                        reference(self.descriptors, id, "descriptor")?;
                        id.intern(pool);
                    }
                }
                Effect::StepSelfOneCell | Effect::PushTargetOneCell { .. } => {}
                Effect::Damage {
                    damage: DamageExpr::SelectedWeapon { impact_bonus },
                } => bound(
                    i64::from(*impact_bonus) <= crate::limits::NUMERIC,
                    "impact_bonus",
                )?,
                Effect::Damage {
                    damage: DamageExpr::Components { components },
                } => {
                    bound(
                        !components.is_empty()
                            && components.len() <= crate::limits::DAMAGE_COMPONENTS,
                        "damage_components",
                    )?;
                    let mut checked = Vec::with_capacity(components.len());
                    for component in components {
                        if let Some(id) = &mut component.descriptor {
                            reference(self.descriptors, id, "descriptor")?;
                            id.intern(pool);
                        }
                        checked.push(match &component.amount {
                            AmountExpr::Fixed { amount } => crate::damage::DamageComponent::fixed(
                                component.category,
                                component.descriptor.clone(),
                                *amount,
                            ),
                            AmountExpr::Rolled { dice } => {
                                let dice = DicePool::new(dice.count, dice.sides, dice.bonus)
                                    .map_err(|_| RulesetError::Bound {
                                        field: "dice".into(),
                                    })?;
                                crate::damage::DamageComponent::rolled(
                                    component.category,
                                    component.descriptor.clone(),
                                    dice,
                                )
                            }
                        });
                    }
                    crate::damage::DamageSpec::new(checked, None).map_err(|_| {
                        RulesetError::Bound {
                            field: "damage_components".into(),
                        }
                    })?;
                }
                Effect::HealPerHitDie { sides, bonus, .. } => {
                    DicePool::new(1, *sides, *bonus).map_err(|_| RulesetError::Bound {
                        field: "heal_per_hit_die".into(),
                    })?;
                }
                Effect::DetectMonsters { condition } => {
                    reference(self.conditions, condition, "monster_detection_condition")?;
                    bound(
                        self.condition_definitions.iter().any(|definition| {
                            &definition.id == condition && definition.detecting_monsters
                        }),
                        "monster_detection_condition",
                    )?;
                    condition.intern(pool);
                }
                Effect::HasteDose { condition, rest } => {
                    reference(self.conditions, condition, "haste_condition")?;
                    reference(self.conditions, rest, "rest_condition")?;
                    bound(
                        self.condition_definitions
                            .iter()
                            .find(|definition| &definition.id == condition)
                            .zip(
                                self.condition_definitions
                                    .iter()
                                    .find(|definition| &definition.id == rest),
                            )
                            .is_some_and(|(haste, rest)| {
                                crate::abilities::haste_dose_conditions_valid(haste, rest)
                            }),
                        "haste_dose_conditions",
                    )?;
                    condition.intern(pool);
                    rest.intern(pool);
                }
                Effect::ApplyConditionForTurns {
                    condition,
                    duration,
                } => {
                    reference(self.conditions, condition, "condition")?;
                    condition.intern(pool);
                    bound(
                        self.condition_definitions.iter().any(|definition| {
                            &definition.id == condition && definition.timing.permits_finite()
                        }),
                        "condition_timing",
                    )?;
                    bound(positive_duration_shape(duration), "condition_duration")?;
                }
                Effect::ApplyPersistentCondition { condition, cancels } => {
                    reference(
                        self.persistent_conditions,
                        condition,
                        "persistent_condition",
                    )?;
                    condition.intern(pool);
                    if let Some(opposite) = cancels {
                        reference(self.persistent_conditions, opposite, "persistent_condition")?;
                        bound(opposite != condition, "opposite_condition")?;
                        opposite.intern(pool);
                    }
                }
                Effect::RemoveCondition { condition } => {
                    reference(self.conditions, condition, "condition")?;
                    condition.intern(pool);
                }
                Effect::Heal { amount }
                | Effect::Nourish { amount }
                | Effect::LoseAttribute { amount, .. }
                | Effect::RestoreAttribute { amount, .. }
                | Effect::LoseMaximumHealth { amount }
                | Effect::RestoreMaximumHealth { amount }
                | Effect::Restore { amount, .. } => match amount {
                    AmountExpr::Fixed { amount } => {
                        bound(*amount <= crate::limits::NUMERIC as u32, "effect amount")?
                    }
                    AmountExpr::Rolled { dice } => valid_dice(dice)?,
                },
                Effect::ApplyCondition {
                    condition,
                    duration,
                }
                | Effect::ApplyQualifiedConditionToSource {
                    condition,
                    duration,
                    ..
                } => {
                    reference(self.conditions, condition, "condition")?;
                    condition.intern(pool);
                    bound(
                        self.condition_definitions.iter().any(|definition| {
                            &definition.id == condition && definition.timing.permits_finite()
                        }),
                        "condition_timing",
                    )?;
                    bound(
                        *duration > 0 && *duration <= crate::limits::NUMERIC as u64,
                        "condition_duration",
                    )?;
                }
                Effect::SearchAdjacentGuises
                | Effect::ObservePublicPlan
                | Effect::DrainLatestHitDie
                | Effect::RestoreHitDie
                | Effect::TeleportRandom
                | Effect::TeleportToRayOrigin
                | Effect::RoomLight
                | Effect::DetectFood
                | Effect::MapTopology
                | Effect::DetectMagic
                | Effect::DrainLife
                | Effect::GainStrength
                | Effect::RaiseLevel
                | Effect::CancelSpecials
                | Effect::ArrestDescent => {}
            }
            if let Effect::ApplyQualifiedConditionToSource {
                condition,
                ability_tag,
                ..
            } = value
            {
                bound(
                    self.qualified_conditions.contains(condition),
                    "qualified_condition",
                )?;
                reference(self.ability_tags, ability_tag, "ability_tag")?;
                ability_tag.intern(pool);
            }
        }
        Ok(())
    }
}
/// Context-free structural checks for scenario and item sources. Catalog references
/// must additionally pass `Ruleset::validate_modifiers` when content is attached.
pub fn validate_modifier_shapes(values: &[Modifier]) -> Result<(), RulesetError> {
    bound(
        values.len() <= crate::limits::MODIFIERS_PER_SOURCE,
        "modifiers",
    )?;
    for modifier in values {
        bound(
            match modifier.scope {
                Scope::Always => true,
                Scope::VersusSource => matches!(
                    &modifier.op,
                    ModifierOp::Add {
                        stat: Stat::Edge | Stat::CheckEdge | Stat::IncomingEdge { defense: None },
                        ..
                    } | ModifierOp::Set {
                        stat: Stat::Edge | Stat::CheckEdge | Stat::IncomingEdge { defense: None },
                        ..
                    }
                ),
                Scope::WhileNoSensedHostile => matches!(
                    &modifier.op,
                    ModifierOp::Add {
                        stat: Stat::HealthRecoveryPeriod | Stat::HealthRecoveryPeriodPercent,
                        ..
                    } | ModifierOp::Set {
                        stat: Stat::HealthRecoveryPeriod | Stat::HealthRecoveryPeriodPercent,
                        ..
                    }
                ),
            },
            "modifier_scope",
        )?;
        match &modifier.op {
            ModifierOp::Add { stat, value } | ModifierOp::Set { stat, value } => {
                valid_modifier_value(stat, *value)?
            }
            ModifierOp::AttributeWithoutDefense { value, .. } => bound(
                (-crate::limits::STAT_MAGNITUDE..=crate::limits::STAT_MAGNITUDE).contains(value),
                "attribute_without_defense",
            )?,
            ModifierOp::SignedReduction { amount, .. } => bound(
                (-crate::limits::NUMERIC..=crate::limits::NUMERIC).contains(&i64::from(*amount)),
                "signed_reduction",
            )?,
            ModifierOp::HealthRecoveryRate { multiplier } => {
                bound((1..=10000).contains(multiplier), "health_recovery_rate")?
            }
            ModifierOp::SustainAttribute { .. } | ModifierOp::ProtectArmor {} => {}
            ModifierOp::Reduction { amount, .. } | ModifierOp::TalentReduction { amount, .. } => {
                bound(i64::from(*amount) <= crate::limits::NUMERIC, "reduction")?
            }
            ModifierOp::Locomotion {
                value:
                    crate::locomotion::LocomotionValue::Erratic {
                        chance: Some(chance),
                    }
                    | crate::locomotion::LocomotionValue::Confused {
                        chance: Some(chance),
                    },
            } => bound(chance.valid(), "movement_chance")?,
            ModifierOp::Perception {
                value:
                    crate::perception::PerceptionValue::ObjectGuise {
                        appearance, edge, ..
                    },
            } => bound(
                edge.unsigned_abs() <= crate::limits::NUMERIC as u64
                    && appearance
                        .as_ref()
                        .is_none_or(|appearance| appearance.valid()),
                "guise_appearance",
            )?,
            ModifierOp::Perception {
                value: crate::perception::PerceptionValue::ContinualStealth { edge },
            } => bound(
                u64::from(*edge) <= crate::limits::NUMERIC as u64,
                "stealth_edge",
            )?,
            ModifierOp::Perception {
                value:
                    crate::perception::PerceptionValue::Aggravation {
                        interval,
                        chance_denominator,
                        stealth_disadvantage,
                    },
            } => bound(
                crate::perception::AggravationPolicy {
                    interval: *interval,
                    chance_denominator: *chance_denominator,
                    stealth_disadvantage: *stealth_disadvantage,
                }
                .valid(),
                "aggravation",
            )?,
            ModifierOp::Perception {
                value: crate::perception::PerceptionValue::PeriodicSearch { interval },
            } => bound(
                (1..=crate::limits::NUMERIC as u64).contains(interval),
                "periodic_search",
            )?,
            ModifierOp::Perception { .. }
            | ModifierOp::ActionDuration { .. }
            | ModifierOp::Locomotion { .. }
            | ModifierOp::Flag { .. }
            | ModifierOp::Immunity { .. }
            | ModifierOp::GrantAbility { .. } => {}
        }
    }
    Ok(())
}
fn valid_modifier_value(stat: &Stat, value: i64) -> Result<(), RulesetError> {
    let (minimum, maximum) = match stat {
        Stat::Attribute { .. }
        | Stat::Defense { .. }
        | Stat::Edge
        | Stat::CheckEdge
        | Stat::IncomingEdge { .. } => (
            -crate::limits::STAT_MAGNITUDE,
            crate::limits::STAT_MAGNITUDE,
        ),
        Stat::AttackDamageRollBonus
        | Stat::NutritionRateThirtieths
        | Stat::Health
        | Stat::HealthRecoveryPeriod
        | Stat::ResourceMax { .. } => (-crate::limits::NUMERIC, crate::limits::NUMERIC),
        Stat::HealthRecoveryPeriodPercent => (-10_000, 10_000),
        Stat::HealthRecoveryResetPercent => (-100, 100),
        Stat::Ability { stat, .. } | Stat::AbilityTag { stat, .. } => match stat {
            AbilityStat::Dice => (0, i64::from(crate::limits::DICE - 1)),
            AbilityStat::Duration | AbilityStat::HoldDamageIncrement => (0, crate::limits::NUMERIC),
            AbilityStat::FlatBonus
            | AbilityStat::CheckBonus
            | AbilityStat::CheckEdge
            | AbilityStat::Difficulty
            | AbilityStat::Range => (
                -crate::limits::STAT_MAGNITUDE,
                crate::limits::STAT_MAGNITUDE,
            ),
            AbilityStat::Preparation | AbilityStat::Recovery => {
                (-crate::limits::NUMERIC, crate::limits::NUMERIC)
            }
        },
    };
    bound((minimum..=maximum).contains(&value), "modifier_value")
}
fn valid_dice(dice: &DiceExpr) -> Result<(), RulesetError> {
    bound(
        DicePool::new(dice.count, dice.sides, dice.bonus).is_ok(),
        "dice",
    )
}

/// Eligibility consumes permanent composition facts, never candidate grants or equipment.
pub trait TalentFacts {
    fn total_hd(&self) -> HdCount;
    fn racial_hd(&self) -> HdCount;
    fn class_level(&self, class: &ClassId) -> HdCount;
    fn creature_type(&self) -> &TypeId;
    fn has_subtype(&self, subtype: &SubtypeId) -> bool;
    fn species_has_flag(&self, flag: &FlagId) -> bool;
    fn skill_rank(&self, skill: &SkillSelector) -> u16;
    fn attribute(&self, attribute: Attribute) -> u16;
    fn has_flag(&self, flag: &FlagId) -> bool;
}
impl Requirement {
    /// Parent requirements use active owned talents, so a drained/dormant parent
    /// cannot keep its dependent nodes active. Equipment never enters `facts`.
    pub fn satisfied(&self, facts: &impl TalentFacts, active_talents: &BTreeSet<TalentId>) -> bool {
        match self {
            Self::All { requirements } => requirements
                .iter()
                .all(|r| r.satisfied(facts, active_talents)),
            Self::Any { requirements } => requirements
                .iter()
                .any(|r| r.satisfied(facts, active_talents)),
            Self::TotalHd { minimum } => facts.total_hd() >= *minimum,
            Self::RacialHd { minimum } => facts.racial_hd() >= *minimum,
            Self::ClassLevel { class, minimum } => facts.class_level(class) >= *minimum,
            Self::TypeIn { types } => types.contains(facts.creature_type()),
            Self::SubtypeIn { subtypes } => subtypes.iter().any(|id| facts.has_subtype(id)),
            Self::OwnedTalent { talent } => active_talents.contains(talent),
            Self::SpeciesFlag { flag, present } => facts.species_has_flag(flag) == *present,
            Self::SkillRank { skill, minimum } => facts.skill_rank(skill) >= *minimum,
            Self::AttributeMinimum { attribute, minimum } => {
                facts.attribute(*attribute) >= *minimum
            }
            Self::Flag { flag, present } => facts.has_flag(flag) == *present,
        }
    }

    fn talent_dependencies(&self, result: &mut Vec<TalentId>) {
        match self {
            Self::All { requirements } | Self::Any { requirements } => {
                for requirement in requirements {
                    requirement.talent_dependencies(result);
                }
            }
            Self::OwnedTalent { talent } => result.push(talent.clone()),
            Self::TotalHd { .. }
            | Self::RacialHd { .. }
            | Self::ClassLevel { .. }
            | Self::TypeIn { .. }
            | Self::SubtypeIn { .. }
            | Self::SpeciesFlag { .. }
            | Self::SkillRank { .. }
            | Self::AttributeMinimum { .. }
            | Self::Flag { .. } => {}
        }
    }
}
impl Ruleset {
    pub fn active_talents(
        &self,
        selected: &BTreeSet<TalentId>,
        facts: &impl TalentFacts,
    ) -> BTreeSet<TalentId> {
        let mut active = BTreeSet::new();
        for id in &self.talent_order {
            if let Some(def) = self.talent(id) {
                if selected.contains(id)
                    && def.prerequisites.iter().all(|p| active.contains(p))
                    && def.requirement.satisfied(facts, &active)
                {
                    active.insert(id.clone());
                }
            }
        }
        active
    }
}
impl Ruleset {
    /// DamageSpec proves bounded numeric shape; this checks its content references.
    pub fn validate_damage(&self, damage: &crate::damage::DamageSpec) -> Result<(), RulesetError> {
        for component in damage.components() {
            if let Some(id) = component.key().descriptor {
                if !self.indexes.descriptors.contains_key(&id) {
                    return Err(RulesetError::UnknownReference {
                        category: "descriptor",
                        id: id.to_string(),
                    });
                }
            }
        }
        Ok(())
    }
    /// Validate scenario-owned modifier sources against the same content graph.
    pub fn validate_modifiers(&self, modifiers: &[Modifier]) -> Result<(), RulesetError> {
        bound(
            modifiers
                .iter()
                .all(|modifier| modifier.scope != Scope::VersusSource),
            "permanent_modifier_scope",
        )?;
        self.validate_condition_modifiers(modifiers)
    }

    // Existing range modifiers are sight-cell adjustments, not physical feet
    // band adjustments. Reject unsupported authoring before any actor uses it.
    fn validate_projectile_range_modifiers(
        &self,
        modifiers: &[Modifier],
    ) -> Result<(), RulesetError> {
        for modifier in modifiers {
            let (ModifierOp::Add { stat, value } | ModifierOp::Set { stat, value }) = &modifier.op
            else {
                continue;
            };
            if *value == 0 {
                continue;
            }
            let projectile = match stat {
                Stat::Ability {
                    ability,
                    stat: AbilityStat::Range,
                } => self.ability(ability).is_some_and(|ability| {
                    ability.invocation == crate::abilities::InvocationKind::DirectionalProjectile
                }),
                Stat::AbilityTag {
                    tag,
                    stat: AbilityStat::Range,
                } => self.abilities_with_tag(tag).is_some_and(|members| {
                    members.iter().any(|id| {
                        self.ability(id).is_some_and(|ability| {
                            ability.invocation
                                == crate::abilities::InvocationKind::DirectionalProjectile
                        })
                    })
                }),
                _ => false,
            };
            bound(!projectile, "projectile_range_adjustment_unsupported")?;
        }
        Ok(())
    }

    /// Condition contributions carry a causer, allowing the supported scoped edge operators.
    pub fn validate_condition_modifiers(&self, modifiers: &[Modifier]) -> Result<(), RulesetError> {
        self.validate_projectile_range_modifiers(modifiers)?;
        let abilities = self.indexes.abilities.keys().cloned().collect();
        let ability_tags = self.indexes.ability_tags.keys().cloned().collect();
        let types = self.indexes.creature_types.keys().cloned().collect();
        let subtypes = self.indexes.subtypes.keys().cloned().collect();
        let talents = self.indexes.talents.keys().cloned().collect();
        let classes = self.indexes.classes.keys().cloned().collect();
        let conditions = self.indexes.conditions.keys().cloned().collect();
        let persistent_conditions = self
            .document
            .conditions
            .iter()
            .filter(|condition| condition.timing.permits_persistent())
            .map(|condition| condition.id.clone())
            .collect();
        let qualified_conditions = self
            .document
            .conditions
            .iter()
            .filter(|condition| condition.supports_qualification())
            .map(|condition| condition.id.clone())
            .collect();
        let descriptors = self
            .document
            .descriptors
            .iter()
            .map(|d| d.id.clone())
            .collect();
        let flags = self.indexes.flags.keys().cloned().collect();
        let equipment_slots = self
            .document
            .equipment_slots
            .iter()
            .map(|slot| (slot.id.clone(), slot))
            .collect();
        References {
            abilities: &abilities,
            ability_tags: &ability_tags,
            types: &types,
            subtypes: &subtypes,
            talents: &talents,
            classes: &classes,
            conditions: &conditions,
            condition_definitions: &self.document.conditions,
            qualified_conditions: &qualified_conditions,
            persistent_conditions: &persistent_conditions,
            descriptors: &descriptors,
            flags: &flags,
            equipment_slots: &equipment_slots,
        }
        .modifiers(&mut modifiers.to_vec(), &mut Interner::default())?;
        self.expanded_modifier_count(modifiers).map(|_| ())
    }

    /// Capped sources grant additive edge only. Flags cannot hide flat accuracy
    /// or overrides with undefined precedence across capped source groups.
    fn validate_edge_source(&self, modifiers: &[Modifier]) -> Result<(), RulesetError> {
        let mut pending: Vec<_> = modifiers.iter().collect();
        let mut seen = BTreeSet::new();
        while let Some(modifier) = pending.pop() {
            match &modifier.op {
                ModifierOp::Set {
                    stat:
                        Stat::Edge
                        | Stat::CheckEdge
                        | Stat::IncomingEdge { .. }
                        | Stat::Ability {
                            stat: AbilityStat::CheckEdge,
                            ..
                        }
                        | Stat::AbilityTag {
                            stat: AbilityStat::CheckEdge,
                            ..
                        },
                    ..
                } => {
                    return Err(RulesetError::Bound {
                        field: "edge_source_set".into(),
                    });
                }
                ModifierOp::Add { stat, .. } | ModifierOp::Set { stat, .. } => {
                    bound(
                        !matches!(
                            stat,
                            Stat::Ability {
                                stat: AbilityStat::CheckBonus,
                                ..
                            } | Stat::AbilityTag {
                                stat: AbilityStat::CheckBonus,
                                ..
                            }
                        ),
                        "edge_source_flat_check_bonus",
                    )?;
                }
                ModifierOp::Flag { flag } if seen.insert(flag.clone()) => {
                    pending.extend(&self.flag(flag).expect("validated flag reference").modifiers);
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// Only an incoming contextual source needs a fresh recipient observation.
    /// The validated flag graph has the same bounded once-per-flag expansion as
    /// derivation, including scoped contributions whose current net rate cancels.
    pub(crate) fn modifiers_use_recovery_context(&self, modifiers: &[Modifier]) -> bool {
        let mut pending: Vec<_> = modifiers.iter().collect();
        let mut seen = BTreeSet::new();
        while let Some(modifier) = pending.pop() {
            if modifier.scope == Scope::WhileNoSensedHostile {
                return true;
            }
            if let ModifierOp::Flag { flag } = &modifier.op {
                if seen.insert(flag) {
                    pending.extend(&self.flag(flag).expect("validated flag reference").modifiers);
                }
            }
        }
        false
    }

    /// Count the same once-per-flag implication expansion used by derivation.
    /// Checking before extending keeps even adversarial branching graphs bounded.
    fn expanded_modifier_count(&self, modifiers: &[Modifier]) -> Result<usize, RulesetError> {
        let mut count = modifiers.len();
        bound(
            count <= crate::limits::MODIFIERS_PER_SOURCE,
            "modifier_expansion",
        )?;
        let mut pending: Vec<_> = modifiers.iter().collect();
        let mut seen = BTreeSet::new();
        while let Some(modifier) = pending.pop() {
            if let ModifierOp::Flag { flag } = &modifier.op {
                if seen.insert(flag.clone()) {
                    let definition =
                        self.flag(flag)
                            .ok_or_else(|| RulesetError::UnknownReference {
                                category: "flag",
                                id: flag.to_string(),
                            })?;
                    count = count.saturating_add(definition.modifiers.len());
                    bound(
                        count <= crate::limits::MODIFIERS_PER_SOURCE,
                        "modifier_expansion",
                    )?;
                    pending.extend(definition.modifiers.iter());
                }
            }
        }
        Ok(count)
    }
    pub fn validate_effects(&self, effects: &[Effect]) -> Result<(), RulesetError> {
        crate::detection_lifecycle::validate_sole_effects(effects)?;
        bound(
            !effects
                .iter()
                .any(|effect| matches!(effect, Effect::Theft { .. })),
            "theft_checked_rider",
        )?;
        let abilities = self.indexes.abilities.keys().cloned().collect();
        let ability_tags = self.indexes.ability_tags.keys().cloned().collect();
        let types = self.indexes.creature_types.keys().cloned().collect();
        let subtypes = self.indexes.subtypes.keys().cloned().collect();
        let talents = self.indexes.talents.keys().cloned().collect();
        let classes = self.indexes.classes.keys().cloned().collect();
        let conditions = self.indexes.conditions.keys().cloned().collect();
        let persistent_conditions = self
            .document
            .conditions
            .iter()
            .filter(|condition| condition.timing.permits_persistent())
            .map(|condition| condition.id.clone())
            .collect();
        let qualified_conditions = self
            .document
            .conditions
            .iter()
            .filter(|condition| condition.supports_qualification())
            .map(|condition| condition.id.clone())
            .collect();
        let descriptors = self
            .document
            .descriptors
            .iter()
            .map(|d| d.id.clone())
            .collect();
        let flags = self.indexes.flags.keys().cloned().collect();
        let equipment_slots = self
            .document
            .equipment_slots
            .iter()
            .map(|slot| (slot.id.clone(), slot))
            .collect();
        References {
            abilities: &abilities,
            ability_tags: &ability_tags,
            types: &types,
            subtypes: &subtypes,
            talents: &talents,
            classes: &classes,
            conditions: &conditions,
            condition_definitions: &self.document.conditions,
            qualified_conditions: &qualified_conditions,
            persistent_conditions: &persistent_conditions,
            descriptors: &descriptors,
            flags: &flags,
            equipment_slots: &equipment_slots,
        }
        .effects(&mut effects.to_vec(), &mut Interner::default())
    }
    pub fn validate_strikes(&self, strikes: &[StrikeDef]) -> Result<(), RulesetError> {
        let abilities = self.indexes.abilities.keys().cloned().collect();
        let ability_tags = self.indexes.ability_tags.keys().cloned().collect();
        let types = self.indexes.creature_types.keys().cloned().collect();
        let subtypes = self.indexes.subtypes.keys().cloned().collect();
        let talents = self.indexes.talents.keys().cloned().collect();
        let classes = self.indexes.classes.keys().cloned().collect();
        let conditions = self.indexes.conditions.keys().cloned().collect();
        let persistent_conditions = self
            .document
            .conditions
            .iter()
            .filter(|condition| condition.timing.permits_persistent())
            .map(|condition| condition.id.clone())
            .collect();
        let qualified_conditions = self
            .document
            .conditions
            .iter()
            .filter(|condition| condition.supports_qualification())
            .map(|condition| condition.id.clone())
            .collect();
        let descriptors = self
            .document
            .descriptors
            .iter()
            .map(|d| d.id.clone())
            .collect();
        let flags = self.indexes.flags.keys().cloned().collect();
        let equipment_slots = self
            .document
            .equipment_slots
            .iter()
            .map(|slot| (slot.id.clone(), slot))
            .collect();
        References {
            abilities: &abilities,
            ability_tags: &ability_tags,
            types: &types,
            subtypes: &subtypes,
            talents: &talents,
            classes: &classes,
            conditions: &conditions,
            condition_definitions: &self.document.conditions,
            qualified_conditions: &qualified_conditions,
            persistent_conditions: &persistent_conditions,
            descriptors: &descriptors,
            flags: &flags,
            equipment_slots: &equipment_slots,
        }
        .strikes(&mut strikes.to_vec(), &mut Interner::default())
    }
}

pub const DEFAULT_RULESET_TOML: &str = include_str!("../data/default-ruleset.toml");

impl std::fmt::Display for RulesetError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidId(id) => write!(formatter, "invalid content ID {id}"),
            Self::DuplicateId(id) => write!(formatter, "duplicate content ID {id}"),
            Self::UnknownReference { category, id } => {
                write!(formatter, "unknown {category} reference {id}")
            }
            Self::Bound { field } => write!(formatter, "invalid or out-of-bounds {field}"),
            Self::Cycle { category, id } => {
                write!(formatter, "cyclic {category} dependency at {id}")
            }
            Self::DuplicateResource => formatter.write_str("duplicate resource definition"),
            Self::MissingResource => formatter.write_str("missing resource definition"),
        }
    }
}
impl std::error::Error for RulesetError {}

fn positive_duration_shape(value: &AmountExpr) -> bool {
    match value {
        AmountExpr::Fixed { amount } => (1..=crate::limits::NUMERIC as u32).contains(amount),
        AmountExpr::Rolled { dice } => {
            DicePool::new(dice.count, dice.sides, dice.bonus).is_ok_and(|pool| pool.minimum() > 0)
        }
    }
}
impl Effect {
    pub fn valid_shape(&self) -> bool {
        let amount = |value: &AmountExpr| match value {
            AmountExpr::Fixed { amount } => *amount <= crate::limits::NUMERIC as u32,
            AmountExpr::Rolled { dice } => {
                DicePool::new(dice.count, dice.sides, dice.bonus).is_ok()
            }
        };
        match self {
            Self::Theft { selection, .. } => match selection {
                crate::theft::TheftSelection::Currency { amount, .. } => amount.valid(),
                crate::theft::TheftSelection::MagicalUnit => true,
            },
            Self::AlterEquipment {
                selection,
                alteration,
            } => selection.accepts(*alteration),
            Self::CorrodeArmor { amount, .. } => {
                (1..=crate::limits::NUMERIC as u32).contains(amount)
            }
            Self::HasteDose { condition, rest } => condition != rest,
            Self::ApplyLinkedHold { policy } => policy.valid(),
            Self::HoldDamage { .. } => true,
            Self::StepSelfOneCell
            | Self::PushTargetOneCell { .. }
            | Self::DrainLatestHitDie
            | Self::RestoreHitDie
            | Self::TeleportRandom
            | Self::TeleportToRayOrigin
            | Self::RoomLight
            | Self::DetectMonsters { .. }
            | Self::DetectFood
            | Self::MapTopology
            | Self::DetectMagic
            | Self::DrainLife
            | Self::GainStrength
            | Self::RaiseLevel
            | Self::CancelSpecials
            | Self::ArrestDescent => true,
            Self::HealPerHitDie { sides, bonus, .. } => DicePool::new(1, *sides, *bonus).is_ok(),
            Self::ApplyConditionForTurns { duration, .. } => positive_duration_shape(duration),
            Self::ApplyPersistentCondition { condition, cancels } => {
                cancels.as_ref() != Some(condition)
            }
            Self::RemoveCondition { .. } => true,
            Self::Heal { amount: value }
            | Self::Nourish { amount: value }
            | Self::LoseAttribute { amount: value, .. }
            | Self::RestoreAttribute { amount: value, .. }
            | Self::LoseMaximumHealth { amount: value }
            | Self::RestoreMaximumHealth { amount: value }
            | Self::Restore { amount: value, .. } => amount(value),
            Self::ApplyCondition { duration, .. }
            | Self::ApplyQualifiedConditionToSource { duration, .. } => {
                (1..=crate::limits::NUMERIC as u64).contains(duration)
            }
            Self::SearchAdjacentGuises | Self::ObservePublicPlan => true,
            Self::Damage { damage } => match damage {
                DamageExpr::SelectedWeapon { impact_bonus } => {
                    *impact_bonus <= crate::limits::NUMERIC as u32
                }
                DamageExpr::Components { components } => {
                    !components.is_empty()
                        && components.len() <= crate::limits::DAMAGE_COMPONENTS
                        && components.iter().all(|component| amount(&component.amount))
                }
            },
        }
    }
}

fn validate_point_grants(
    grants: &mut [PointGrant],
    pools: &BTreeSet<PointPoolId>,
    interner: &mut Interner,
) -> Result<(), RulesetError> {
    bound(
        grants.len() <= crate::limits::POINT_POOLS_PER_SOURCE,
        "point_grants",
    )?;
    let mut seen = BTreeSet::new();
    let mut total = 0usize;
    for grant in grants {
        reference(pools, &grant.pool, "point_pool")?;
        if !seen.insert(grant.pool.clone()) {
            return Err(RulesetError::DuplicateId(grant.pool.to_string()));
        }
        total += usize::from(grant.points);
        bound(
            grant.points > 0 && total <= crate::limits::POINTS_PER_HIT_DIE,
            "point_grants",
        )?;
        grant.pool.intern(interner);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn document() -> RulesetDocument {
        toml::from_str(DEFAULT_RULESET_TOML).unwrap()
    }
    fn first_strike(doc: &mut RulesetDocument, index: usize) -> &mut StrikeDef {
        if matches!(doc.abilities[index].strikes, StrikeSource::SelectedMelee) {
            doc.abilities[index].strikes = StrikeSource::Authored {
                strikes: vec![StrikeDef {
                    cancellable: false,
                    offset: 0,
                    unlock: None,
                    check: None,
                    on_success: vec![],
                    on_failure: vec![],
                    descriptors: vec![],
                    riders: vec![],
                }],
            };
        }
        let StrikeSource::Authored { strikes } = &mut doc.abilities[index].strikes else {
            unreachable!()
        };
        &mut strikes[0]
    }
    fn ruleset() -> Arc<Ruleset> {
        Ruleset::validate(document()).unwrap()
    }
    fn repeatable_restriction_document() -> RulesetDocument {
        let mut document = document();
        document.conditions.push(ConditionDef {
            detecting_monsters: false,
            hallucinating: false,
            id: ConditionId::new("fixture_repeatable_lock").unwrap(),
            family: ConditionFamilyId::new("fixture_repeatable_lock").unwrap(),
            lifecycle: ConditionLifecycle::Timed,
            name: "Repeatable lock".into(),
            modifiers: vec![],
            stacking: Stacking::Extend,
            source_scoped: false,
            descriptors: vec![],
            capacity: Eviction::Reject,
            timing: ConditionTiming::expiring(),
            restrictions: vec![ConditionRestriction::VoluntaryActions],
            preparation: ConditionPreparation::Pause,
        });
        document
    }
    #[test]
    fn repeatable_expiring_restrictions_are_valid_rule_definitions() {
        let result = Ruleset::validate(repeatable_restriction_document());
        assert!(
            result.is_ok(),
            "bounded duration extension must be valid for target-wide restrictions without post-expiry protection: {result:?}"
        );
    }
    #[test]
    fn repeatable_restrictions_keep_protected_shape_limits() {
        for invalid in 0..7 {
            let mut document = repeatable_restriction_document();
            let definition = document.conditions.last_mut().unwrap();
            match invalid {
                0 => definition.source_scoped = true,
                1 => definition.lifecycle = ConditionLifecycle::NextCheck,
                2 => {
                    definition.modifiers.push(Modifier {
                        cancellable: false,
                        scope: Scope::Always,
                        op: ModifierOp::Add {
                            stat: Stat::Health,
                            value: 1,
                        },
                    });
                }
                3 => {
                    definition.timing = ConditionTiming::ActorActive {
                        on_expiry: ConditionExpiry::Protect {
                            duration: ConditionTicks::new(400).unwrap(),
                        },
                    };
                }
                4 => definition.stacking = Stacking::Refresh,
                5 => definition.stacking = Stacking::Longest,
                6 => definition.stacking = Stacking::Stack { maximum: 2 },
                _ => unreachable!(),
            }
            assert!(
                matches!(
                    Ruleset::validate(document),
                    Err(RulesetError::Bound { field }) if field == "protected_condition_shape"
                ),
                "invalid protected shape {invalid}"
            );
        }
    }
    #[test]
    fn basic_attack_cannot_target_only_self() {
        let mut doc = document();
        let basic = doc
            .abilities
            .iter_mut()
            .find(|ability| ability.basic_attack)
            .unwrap();
        basic.reach = Reach::SelfOnly;
        assert!(matches!(
            Ruleset::validate(doc),
            Err(RulesetError::Bound { field }) if field == "basic_attack_reach"
        ));
    }
    #[test]
    fn unsupported_scoped_modifiers_and_flag_implications_reject() {
        let invalid = Modifier {
            cancellable: false,
            scope: Scope::VersusSource,
            op: ModifierOp::Add {
                stat: Stat::Health,
                value: 1,
            },
        };
        assert!(validate_modifier_shapes(&[invalid]).is_err());
        let scoped = Modifier {
            cancellable: false,
            scope: Scope::VersusSource,
            op: ModifierOp::Add {
                stat: Stat::Edge,
                value: -1,
            },
        };
        let rules = ruleset();
        assert!(rules
            .validate_condition_modifiers(std::slice::from_ref(&scoped))
            .is_ok());
        assert!(rules
            .validate_modifiers(std::slice::from_ref(&scoped))
            .is_err());
        let mut doc = document();
        doc.flags[0].modifiers.push(scoped.clone());
        assert!(Ruleset::validate(doc).is_err());
        let mut doc = document();
        doc.conditions[0].source_scoped = false;
        doc.conditions[0].modifiers = vec![scoped];
        assert!(Ruleset::validate(doc).is_err());
    }

    #[test]
    fn condition_stacks_include_flag_expansion_in_the_source_budget() {
        let mut doc = document();
        let flag = FlagId::new("many_edges").unwrap();
        doc.flags.push(FlagDef {
            id: flag.clone(),
            name: "Many edges".into(),
            modifiers: vec![
                Modifier {
                    cancellable: false,
                    scope: Scope::Always,
                    op: ModifierOp::Add {
                        stat: Stat::Edge,
                        value: 1
                    }
                };
                64
            ],
        });
        doc.conditions.push(ConditionDef {
            detecting_monsters: false,
            hallucinating: false,
            id: ConditionId::new("marked").unwrap(),
            family: crate::content_ids::ConditionFamilyId::new("marked").unwrap(),
            lifecycle: crate::rules::ConditionLifecycle::Timed,
            timing: tor_simulation::rules::ConditionTiming::expiring(),
            restrictions: vec![],
            preparation: tor_simulation::rules::ConditionPreparation::Preserve,
            name: "Marked".into(),
            modifiers: vec![Modifier {
                cancellable: false,
                scope: Scope::Always,
                op: ModifierOp::Flag { flag },
            }],
            stacking: Stacking::Stack { maximum: 2 },
            source_scoped: true,
            descriptors: vec![],
            capacity: Eviction::Oldest,
        });
        assert!(
            matches!(Ruleset::validate(doc), Err(RulesetError::Bound { field }) if field == "condition_stack_expansion")
        );
    }

    #[test]
    fn external_sources_cannot_expand_beyond_the_source_budget() {
        let mut doc = document();
        let flag = FlagId::new("many_edges").unwrap();
        doc.flags.push(FlagDef {
            id: flag.clone(),
            name: "Many edges".into(),
            modifiers: vec![
                Modifier {
                    cancellable: false,
                    scope: Scope::Always,
                    op: ModifierOp::Add {
                        stat: Stat::Edge,
                        value: 1
                    }
                };
                64
            ],
        });
        let rules = Ruleset::validate(doc).unwrap();
        let mut source = vec![Modifier {
            cancellable: false,
            scope: Scope::Always,
            op: ModifierOp::Flag { flag },
        }];
        source.extend(vec![
            Modifier {
                cancellable: false,
                scope: Scope::Always,
                op: ModifierOp::Add {
                    stat: Stat::Edge,
                    value: 1
                }
            };
            64
        ]);
        assert!(
            matches!(rules.validate_modifiers(&source), Err(RulesetError::Bound { field }) if field == "modifier_expansion")
        );
    }

    #[test]
    fn default_content_validates_and_round_trips() {
        let rules = ruleset();
        for id in ["second_strike", "third_strike", "rending_pair"] {
            assert!(rules.talent(&TalentId::new(id).unwrap()).is_some());
        }
        assert_eq!(rules.document.forms.len(), 6);
        assert_eq!(rules.document.creature_types.len(), 15);
        assert_eq!(rules.document.subtypes.len(), 20);
        let restored: RulesetDocument =
            toml::from_str(&toml::to_string(rules.document()).unwrap()).unwrap();
        assert_eq!(Ruleset::validate(restored).unwrap(), rules);
    }
    #[test]
    fn adding_a_talent_is_data_only() {
        let mut doc = document();
        doc.talents.push(TalentDef {
            form_choices: vec![],
            expertise_grants: vec![],
            id: TalentId::new("fixture_hardiness").unwrap(),
            name: "Fixture Hardiness".into(),
            prerequisites: vec![],
            requirement: Requirement::All {
                requirements: vec![],
            },
            modifiers: vec![Modifier {
                cancellable: false,
                scope: Scope::Always,
                op: ModifierOp::Add {
                    stat: Stat::Health,
                    value: 7,
                },
            }],
        });
        let rules = Ruleset::validate(doc).unwrap();
        assert!(rules
            .talent(&TalentId::new("fixture_hardiness").unwrap())
            .is_some());
    }
    #[test]
    fn unknown_references_are_rejected() {
        let mut doc = document();
        doc.talents[0]
            .prerequisites
            .push(TalentId::new("missing").unwrap());
        assert!(matches!(
            Ruleset::validate(doc),
            Err(RulesetError::UnknownReference {
                category: "talent",
                ..
            })
        ));
        let bad = Modifier {
            cancellable: false,
            scope: Scope::Always,
            op: ModifierOp::GrantAbility {
                ability: AbilityId::new("missing").unwrap(),
            },
        };
        assert!(matches!(
            ruleset().validate_modifiers(&[bad]),
            Err(RulesetError::UnknownReference {
                category: "ability",
                ..
            })
        ));
    }
    #[test]
    fn talent_cycles_and_duplicate_declarations_are_rejected() {
        let mut doc = document();
        let mut first = crate::test_creatures::talent("fixture_cycle_first", vec![]);
        let mut second = crate::test_creatures::talent("fixture_cycle_second", vec![]);
        first.prerequisites.push(second.id.clone());
        second.prerequisites.push(first.id.clone());
        doc.talents.extend([first, second]);
        assert!(matches!(
            Ruleset::validate(doc),
            Err(RulesetError::Cycle {
                category: "talent",
                ..
            })
        ));
        let mut doc = document();
        doc.talents.push(doc.talents[0].clone());
        assert!(matches!(
            Ruleset::validate(doc),
            Err(RulesetError::DuplicateId(_))
        ));
    }
    #[test]
    fn implied_flag_cycles_are_rejected() {
        let mut doc = document();
        doc.flags[0].modifiers.push(Modifier {
            cancellable: false,
            scope: Scope::Always,
            op: ModifierOp::Flag {
                flag: FlagId::new("mindless").unwrap(),
            },
        });
        assert!(matches!(
            Ruleset::validate(doc),
            Err(RulesetError::Cycle {
                category: "flag",
                ..
            })
        ));
    }
    #[test]
    fn invalid_bounds_and_unknown_fields_are_rejected() {
        let mut doc = document();
        doc.resources[0].recovery_period = 0;
        assert!(matches!(
            Ruleset::validate(doc),
            Err(RulesetError::Bound { .. })
        ));
        let mut doc = document();
        doc.constants.attribute_opportunity_interval = 0;
        assert!(matches!(
            Ruleset::validate(doc),
            Err(RulesetError::Bound { .. })
        ));
        assert!(
            toml::from_str::<RulesetDocument>(&DEFAULT_RULESET_TOML.replace(
                "id = \"interactions-v53\"",
                "id = \"interactions-v53\"\nunknown = true"
            ))
            .is_err()
        );
    }
    #[test]
    fn invalid_composite_damage_reuses_kernel_validation() {
        let mut doc = document();
        first_strike(&mut doc, 2).on_success = vec![Effect::Damage {
            damage: DamageExpr::Components {
                components: vec![
                    DamageComponentExpr {
                        category: DamageType::Energy,
                        descriptor: Some(DescriptorId::new("fire").unwrap()),
                        amount: AmountExpr::Fixed { amount: 1 },
                    },
                    DamageComponentExpr {
                        category: DamageType::Impact,
                        descriptor: Some(DescriptorId::new("fire").unwrap()),
                        amount: AmountExpr::Fixed { amount: 1 },
                    },
                ],
            },
        }];
        assert!(matches!(
            Ruleset::validate(doc),
            Err(RulesetError::Bound { .. })
        ));
    }
    #[test]
    fn stacked_conditions_fit_the_source_modifier_budget() {
        let mut doc = document();
        doc.conditions[0].stacking = Stacking::Stack { maximum: 2 };
        doc.conditions[0].modifiers = vec![doc.conditions[0].modifiers[0].clone(); 65];
        assert!(matches!(
            Ruleset::validate(doc),
            Err(RulesetError::Bound { .. })
        ));
    }
    #[test]
    fn source_scoping_and_unsigned_ability_bonuses_are_validated() {
        let mut doc = document();
        doc.conditions[0].source_scoped = false;
        assert!(matches!(
            Ruleset::validate(doc),
            Err(RulesetError::Bound { .. })
        ));
        let invalid = Modifier {
            cancellable: false,
            scope: Scope::Always,
            op: ModifierOp::Add {
                stat: Stat::Ability {
                    ability: AbilityId::new("falter").unwrap(),
                    stat: AbilityStat::Duration,
                },
                value: -1,
            },
        };
        assert!(validate_modifier_shapes(&[invalid]).is_err());
    }
    #[test]
    fn data_defined_ability_tag_catalog_and_modifier_are_accepted() {
        let mut value = serde_json::to_value(document()).unwrap();
        value["ability_tags"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({"id": "fixture_attack", "name": "Fixture attack"}));
        value["abilities"][0]["tags"] = serde_json::json!(["fixture_attack"]);
        value["talents"][0]["modifiers"] = serde_json::json!([{
            "scope": "always", "op": {"kind": "add", "stat": {
                "kind": "ability_tag", "tag": "fixture_attack", "stat": "flat_bonus"}, "value": 1}}]);
        let parsed = serde_json::from_value::<RulesetDocument>(value);
        assert!(parsed.is_ok(), "ability tag data rejected: {parsed:?}");
        assert!(Ruleset::validate(parsed.unwrap()).is_ok());
    }
    #[test]
    fn ability_tags_reject_unknown_duplicate_and_oversized_references() {
        let tag = AbilityTagId::new("weapon_attack").unwrap();
        let missing = AbilityTagId::new("missing_tag").unwrap();
        let mut doc = document();
        doc.ability_tags.push(doc.ability_tags[0].clone());
        assert!(matches!(
            Ruleset::validate(doc),
            Err(RulesetError::DuplicateId(_))
        ));
        let mut doc = document();
        doc.abilities[0].tags.push(missing.clone());
        assert!(matches!(
            Ruleset::validate(doc),
            Err(RulesetError::UnknownReference {
                category: "ability_tag",
                ..
            })
        ));
        let mut doc = document();
        doc.abilities[0].tags = vec![tag.clone(); 2];
        assert!(matches!(
            Ruleset::validate(doc),
            Err(RulesetError::DuplicateId(_))
        ));
        let mut doc = document();
        doc.abilities[0].tags = vec![tag.clone(); crate::limits::TAGS_PER_ABILITY + 1];
        assert!(
            matches!(Ruleset::validate(doc), Err(RulesetError::Bound { field }) if field == "ability_tags")
        );
        let invalid = Modifier {
            cancellable: false,
            scope: Scope::Always,
            op: ModifierOp::Add {
                stat: Stat::AbilityTag {
                    tag: missing,
                    stat: AbilityStat::FlatBonus,
                },
                value: 1,
            },
        };
        let rules = Ruleset::validate(document()).unwrap();
        assert!(matches!(
            rules.validate_modifiers(&[invalid]),
            Err(RulesetError::UnknownReference {
                category: "ability_tag",
                ..
            })
        ));
        let mut doc = document();
        doc.abilities.reverse();
        let reordered = Ruleset::validate(doc).unwrap();
        assert_eq!(
            rules.abilities_with_tag(&tag),
            reordered.abilities_with_tag(&tag)
        );
        let mut missing = serde_json::to_value(document()).unwrap();
        missing["abilities"][0]
            .as_object_mut()
            .unwrap()
            .remove("tags");
        assert!(serde_json::from_value::<RulesetDocument>(missing).is_err());
    }
    #[test]
    fn data_defined_equipment_slot_catalog_is_accepted() {
        let mut value = serde_json::to_value(document()).unwrap();
        value["equipment_slots"] = serde_json::json!([{
            "id": "tentacle", "name": "Tentacle", "equipped_phrase": "held by a tentacle",
            "compatible_classes": ["tool"], "requires_attack": true,
            "capacity": {"kind":"none"}, "equip_turns": 2, "unequip_turns": 4
        }]);
        value["constants"]["default_anatomy"] = serde_json::json!({"slots":["tentacle", "tentacle"],"capacities":{},"primary_attack_socket":{"kind":"none"}});
        value["constants"]["item_timing"] = serde_json::json!({"consume_turns": 1});
        let parsed = serde_json::from_value::<RulesetDocument>(value);
        assert!(parsed.is_ok(), "custom slot data rejected: {parsed:?}");
        assert!(Ruleset::validate(parsed.unwrap()).is_ok());
    }
    #[test]
    fn slot_catalog_and_timing_are_strict_and_bounded() {
        let original = serde_json::to_value(document()).unwrap();
        for field in ["equip_turns", "unequip_turns"] {
            for invalid in [0, crate::limits::NUMERIC as u64 + 1] {
                let mut value = original.clone();
                value["equipment_slots"][0][field] = serde_json::json!(invalid);
                assert!(Ruleset::validate(serde_json::from_value(value).unwrap()).is_err());
            }
        }
        for invalid in [0, crate::limits::NUMERIC as u64 + 1] {
            let mut value = original.clone();
            value["constants"]["item_timing"]["consume_turns"] = serde_json::json!(invalid);
            assert!(Ruleset::validate(serde_json::from_value(value).unwrap()).is_err());
        }
        for field in [
            "id",
            "name",
            "equipped_phrase",
            "compatible_classes",
            "requires_attack",
            "capacity",
            "equip_turns",
            "unequip_turns",
        ] {
            let mut missing = original.clone();
            missing["equipment_slots"][0]
                .as_object_mut()
                .unwrap()
                .remove(field);
            assert!(serde_json::from_value::<RulesetDocument>(missing).is_err());
        }
        let mut unknown = original;
        unknown["equipment_slots"][0]["extra"] = serde_json::json!(1);
        assert!(serde_json::from_value::<RulesetDocument>(unknown).is_err());
        let mut doc = document();
        doc.equipment_slots.push(doc.equipment_slots[0].clone());
        assert!(matches!(
            Ruleset::validate(doc),
            Err(RulesetError::DuplicateId(_))
        ));
        let mut doc = document();
        doc.constants
            .default_anatomy
            .slots
            .push(EquipmentSlot::new("missing_slot").unwrap());
        assert!(matches!(
            Ruleset::validate(doc),
            Err(RulesetError::UnknownReference {
                category: "equipment_slot",
                ..
            })
        ));
        let mut doc = document();
        doc.equipment_slots[0].compatible_classes.clear();
        assert!(Ruleset::validate(doc).is_err());
        let mut doc = document();
        doc.equipment_slots[0].compatible_classes = vec![crate::ItemClass::Tool; 2];
        assert!(Ruleset::validate(doc).is_err());
        let mut doc = document();
        doc.equipment_slots[0].equipped_phrase = "\n".into();
        assert!(Ruleset::validate(doc).is_err());
    }
    #[test]
    fn authored_effect_and_descriptor_lists_obey_plan_budgets() {
        let mut doc = document();
        first_strike(&mut doc, 0).on_success = vec![
            Effect::Heal {
                amount: AmountExpr::Fixed { amount: 1 }
            };
            crate::limits::EFFECTS_PER_BRANCH + 1
        ];
        assert!(
            matches!(Ruleset::validate(doc),Err(RulesetError::Bound{field}) if field == "effects")
        );
        let mut doc = document();
        first_strike(&mut doc, 0).descriptors =
            vec![DescriptorId::new("fire").unwrap(); crate::limits::DESCRIPTORS_PER_EFFECT + 1];
        assert!(
            matches!(Ruleset::validate(doc),Err(RulesetError::Bound{field}) if field == "descriptors")
        );
    }
    #[test]
    fn effect_shape_uses_the_damage_component_budget() {
        let component = DamageComponentExpr {
            category: DamageType::Impact,
            descriptor: None,
            amount: AmountExpr::Fixed { amount: 1 },
        };
        assert!(Effect::Damage {
            damage: DamageExpr::Components {
                components: vec![component.clone(); crate::limits::DAMAGE_COMPONENTS]
            }
        }
        .valid_shape());
        assert!(!Effect::Damage {
            damage: DamageExpr::Components {
                components: vec![component; crate::limits::DAMAGE_COMPONENTS + 1]
            }
        }
        .valid_shape());
    }
    #[test]
    fn sight_reach_matches_the_engine_visibility_boundary() {
        for range in [1, 16] {
            let mut doc = document();
            doc.abilities[0].reach = Reach::Sight { range };
            assert!(Ruleset::validate(doc).is_ok());
        }
        for range in [0, 17, u16::MAX] {
            let mut doc = document();
            doc.abilities[0].reach = Reach::Sight { range };
            assert!(Ruleset::validate(doc).is_err());
        }
    }
    #[test]
    fn content_names_cannot_inject_control_characters_into_disclosure() {
        for name in ["Bad\nName", "Bad\0Name", "Bad\tName"] {
            let mut doc = document();
            doc.abilities[0].name = name.into();
            assert!(Ruleset::validate(doc).is_err());
        }
    }
    #[test]
    fn declaration_order_does_not_change_talent_evaluation_order() {
        let mut doc = document();
        doc.talents.reverse();
        assert_eq!(
            Ruleset::validate(doc).unwrap().talent_order(),
            ruleset().talent_order()
        );
    }
}

#[cfg(test)]
mod edge_source_validation_tests {
    use super::*;
    #[test]
    fn capped_edge_sources_reject_set_for_every_edge_stat_including_implied_flags() {
        for condition in [false, true] {
            for implied in [false, true] {
                for stat in [
                    Stat::Edge,
                    Stat::CheckEdge,
                    Stat::IncomingEdge {
                        defense: Some(DefenseKind::Physical),
                    },
                    Stat::Ability {
                        ability: AbilityId::new("falter").unwrap(),
                        stat: AbilityStat::CheckEdge,
                    },
                    Stat::AbilityTag {
                        tag: AbilityTagId::new("weapon_attack").unwrap(),
                        stat: AbilityStat::CheckEdge,
                    },
                ] {
                    let mut document: RulesetDocument =
                        toml::from_str(DEFAULT_RULESET_TOML).unwrap();
                    let mut modifier = Modifier {
                        cancellable: false,
                        scope: Scope::Always,
                        op: ModifierOp::Set {
                            stat: stat.clone(),
                            value: -1,
                        },
                    };
                    if implied {
                        let flag = FlagId::new("edge_override").unwrap();
                        document.flags.push(FlagDef {
                            id: flag.clone(),
                            name: "Edge override".into(),
                            modifiers: vec![modifier],
                        });
                        modifier = Modifier {
                            cancellable: false,
                            scope: Scope::Always,
                            op: ModifierOp::Flag { flag },
                        };
                    }
                    if condition {
                        document.conditions[0].modifiers.push(modifier);
                    } else {
                        document.talents[0].modifiers.push(modifier);
                    }
                    assert!(
                        matches!(
                            Ruleset::validate(document),
                            Err(RulesetError::Bound { field }) if field == "edge_source_set"
                        ),
                        "condition={condition}, implied={implied}, stat={stat:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn next_check_tokens_require_one_unscoped_check_only_family() {
        for malformed in 0..7 {
            let mut document: RulesetDocument = toml::from_str(DEFAULT_RULESET_TOML).unwrap();
            let token = document
                .conditions
                .iter_mut()
                .find(|d| d.id.as_str() == "readied")
                .unwrap();
            match malformed {
                0 => token.source_scoped = true,
                1 => token.stacking = Stacking::Stack { maximum: 2 },
                2 => token.modifiers.clear(),
                3 => {
                    token.modifiers[0].op = ModifierOp::Add {
                        stat: Stat::Edge,
                        value: 1,
                    }
                }
                4 => {
                    token.modifiers[0].op = ModifierOp::Add {
                        stat: Stat::CheckEdge,
                        value: 2,
                    }
                }
                5 => token.lifecycle = ConditionLifecycle::NextCheckAgainst,
                6 => token.family = ConditionFamilyId::new("fear").unwrap(),
                _ => unreachable!(),
            }
            assert!(
                Ruleset::validate(document).is_err(),
                "malformed token {malformed}"
            );
        }
        let mut document: RulesetDocument = toml::from_str(DEFAULT_RULESET_TOML).unwrap();
        let mut alias = document
            .conditions
            .iter()
            .find(|d| d.id.as_str() == "readied")
            .unwrap()
            .clone();
        alias.id = ConditionId::new("readied_alias").unwrap();
        document.conditions.push(alias);
        assert!(
            Ruleset::validate(document).is_err(),
            "one token definition per family"
        );
    }

    #[test]
    fn talent_flat_check_bonuses_are_rejected_by_the_loader() {
        let mut document: RulesetDocument = toml::from_str(DEFAULT_RULESET_TOML).unwrap();
        document.talents[0].modifiers.push(Modifier {
            cancellable: false,
            scope: Scope::Always,
            op: ModifierOp::Add {
                stat: Stat::Ability {
                    ability: AbilityId::new("falter").unwrap(),
                    stat: AbilityStat::CheckBonus,
                },
                value: 1,
            },
        });
        assert!(Ruleset::validate(document).is_err());
    }
    #[test]
    fn implied_flags_cannot_bypass_the_flat_check_bonus_rejection() {
        for condition in [false, true] {
            let mut document: RulesetDocument = toml::from_str(DEFAULT_RULESET_TOML).unwrap();
            let id = FlagId::new("accuracy_loophole").unwrap();
            document.flags.push(FlagDef {
                id: id.clone(),
                name: "Accuracy loophole".into(),
                modifiers: vec![Modifier {
                    cancellable: false,
                    scope: Scope::Always,
                    op: ModifierOp::Set {
                        stat: Stat::AbilityTag {
                            tag: AbilityTagId::new("weapon_attack").unwrap(),
                            stat: AbilityStat::CheckBonus,
                        },
                        value: 1,
                    },
                }],
            });
            let modifier = Modifier {
                cancellable: false,
                scope: Scope::Always,
                op: ModifierOp::Flag { flag: id },
            };
            if condition {
                document.conditions[0].modifiers.push(modifier);
            } else {
                document.talents[0].modifiers.push(modifier);
            }
            assert!(Ruleset::validate(document).is_err());
        }
    }
    #[test]
    fn condition_flat_check_bonuses_are_rejected_by_the_loader() {
        let mut document: RulesetDocument = toml::from_str(DEFAULT_RULESET_TOML).unwrap();
        document.conditions[0].modifiers.push(Modifier {
            cancellable: false,
            scope: Scope::Always,
            op: ModifierOp::Add {
                stat: Stat::Ability {
                    ability: AbilityId::new("falter").unwrap(),
                    stat: AbilityStat::CheckBonus,
                },
                value: 1,
            },
        });
        assert!(Ruleset::validate(document).is_err());
    }
}

#[cfg(test)]
mod strict_stat_tests {
    use super::*;

    #[test]
    fn rules_json_rejects_unknown_fields_on_payload_free_stats() {
        let document: RulesetDocument = toml::from_str(DEFAULT_RULESET_TOML).unwrap();
        let original = serde_json::to_value(document).unwrap();
        let mut accepted = Vec::new();
        for kind in [
            "health",
            "edge",
            "check_edge",
            "health_recovery_period",
            "health_recovery_period_percent",
            "health_recovery_reset_percent",
        ] {
            let mut value = original.clone();
            let modifiers = value["creature_types"][0]["modifiers"]
                .as_array_mut()
                .unwrap();
            modifiers.push(serde_json::json!({"scope": "always", "op": {"kind": "add", "value": 1, "stat": {"kind": kind, "private_owner": 999}}}));
            if serde_json::from_value::<RulesetDocument>(value).is_ok() {
                accepted.push(kind);
            }
        }
        assert!(
            accepted.is_empty(),
            "unknown fields accepted for {accepted:?}"
        );
    }

    #[test]
    fn rules_toml_rejects_unknown_fields_on_payload_free_stats() {
        let mut accepted = Vec::new();
        for kind in [
            "health",
            "edge",
            "check_edge",
            "health_recovery_period",
            "health_recovery_period_percent",
            "health_recovery_reset_percent",
        ] {
            let source = format!(
                "scope = 'always'\nop = {{ kind = 'add', value = 1, stat = {{ kind = '{kind}', private_owner = 999 }} }}\n"
            );
            if toml::from_str::<Modifier>(&source).is_ok() {
                accepted.push(kind);
            }
        }
        assert!(
            accepted.is_empty(),
            "unknown fields accepted for {accepted:?}"
        );
    }

    #[test]
    fn stat_forms_keep_their_json_and_toml_round_trip_shapes() {
        for stat in [
            Stat::Health,
            Stat::HealthRecoveryPeriod,
            Stat::HealthRecoveryPeriodPercent,
            Stat::HealthRecoveryResetPercent,
            Stat::Edge,
            Stat::CheckEdge,
            Stat::IncomingEdge { defense: None },
            Stat::IncomingEdge {
                defense: Some(DefenseKind::Physical),
            },
            Stat::Attribute {
                attribute: Attribute::Strength,
            },
            Stat::Defense {
                defense: DefenseKind::Cognitive,
            },
            Stat::ResourceMax {
                resource: Resource::Stamina,
            },
            Stat::Ability {
                ability: AbilityId::new("falter").unwrap(),
                stat: AbilityStat::CheckEdge,
            },
            Stat::AbilityTag {
                tag: AbilityTagId::new("weapon_attack").unwrap(),
                stat: AbilityStat::CheckBonus,
            },
        ] {
            let json = serde_json::to_value(&stat).unwrap();
            assert_eq!(serde_json::from_value::<Stat>(json.clone()).unwrap(), stat);
            let toml = toml::to_string(&stat).unwrap();
            assert_eq!(toml::from_str::<Stat>(&toml).unwrap(), stat);
            assert_eq!(
                serde_json::to_value(serde_json::from_value::<Stat>(json.clone()).unwrap())
                    .unwrap(),
                json
            );
        }
    }
}

#[cfg(test)]
#[path = "rules/strike_content_tests.rs"]
mod strike_content_tests;
