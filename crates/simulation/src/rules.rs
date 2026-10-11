//! Validated immutable content. TOML parsing and content hashing belong to the host.
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RulesetDocument {
    pub id: RulesetId,
    pub constants: Constants,
    pub abilities: Vec<AbilityDef>,
    pub talents: Vec<TalentDef>,
    pub creature_types: Vec<TypeDef>,
    pub subtypes: Vec<SubtypeDef>,
    pub classes: Vec<ClassDef>,
    pub conditions: Vec<ConditionDef>,
    pub descriptors: Vec<NamedDescriptor>,
    pub flags: Vec<FlagDef>,
    pub resources: Vec<ResourceDef>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Constants {
    pub base_defense: i32,
    pub attribute_cap: u16,
    pub derived_attribute_cap: u16,
    pub attribute_opportunity_interval: u16,
    pub racial_training_points: u16,
    pub duration_scale_numerator: u16,
    pub duration_scale_offset: u16,
    pub default_abilities: Vec<AbilityId>,
    pub item_timing: ItemTiming,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
/// Multipliers of the acting creature's turn length, before a plan is captured.
pub struct EquipmentTiming {
    pub standard_turns: u64,
    pub body_armor_turns: u64,
}
impl EquipmentTiming {
    pub fn turns(&self, slot: crate::EquipmentSlot) -> u64 {
        match slot {
            crate::EquipmentSlot::BodyArmor => self.body_armor_turns,
            _ => self.standard_turns,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ItemTiming {
    pub equip: EquipmentTiming,
    pub unequip: EquipmentTiming,
    pub consume_turns: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceDef {
    pub resource: Resource,
    pub recovery_period: u64,
    pub capacity: ResourceCapacity,
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
    pub training_points: u16,
    pub modifiers: Vec<Modifier>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TalentDef {
    pub id: TalentId,
    pub name: String,
    pub prerequisites: Vec<TalentId>,
    pub requirement: Requirement,
    pub modifiers: Vec<Modifier>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Requirement {
    All { requirements: Vec<Requirement> },
    Any { requirements: Vec<Requirement> },
    TotalHd { minimum: u16 },
    RacialHd { minimum: u16 },
    ClassLevel { class: ClassId, minimum: u16 },
    TypeIn { types: Vec<TypeId> },
    SkillRank { skill: SkillSelector, minimum: u16 },
    AttributeMinimum { attribute: Attribute, minimum: u16 },
    Flag { flag: FlagId, present: bool },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SkillSelector {
    Named { skill: Skill },
    NaturalAttack,
    SelectedWeapon,
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
    Dice,
    FlatBonus,
    CheckBonus,
    Difficulty,
    Duration,
    Range,
    Preparation,
    Recovery,
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Stat {
    Attribute {
        attribute: Attribute,
    },
    Defense {
        defense: DefenseKind,
    },
    Health,
    ResourceMax {
        resource: Resource,
    },
    Ability {
        ability: AbilityId,
        stat: AbilityStat,
    },
    Edge,
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Selector {
    Category { category: DamageType },
    Descriptor { descriptor: DescriptorId },
}
// A sum type prevents nonsensical stat/operator pairs (for example Health + immunity).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ModifierOp {
    Add { stat: Stat, value: i64 },
    Set { stat: Stat, value: i64 },
    Flag { flag: FlagId },
    Immunity { selector: Selector },
    Reduction { selector: Selector, amount: u32 },
    GrantAbility { ability: AbilityId },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    Always,
    VersusSource,
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Modifier {
    pub scope: Scope,
    pub op: ModifierOp,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AbilityDef {
    pub id: AbilityId,
    pub name: String,
    pub basic_attack: bool,
    pub cost: Option<Cost>,
    pub timing: Timing,
    pub reach: Reach,
    pub check: Option<CheckDef>,
    pub on_success: Vec<Effect>,
    pub on_failure: Vec<Effect>,
    pub descriptors: Vec<DescriptorId>,
    pub ai: AiHints,
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
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Reach {
    Melee,
    Sight { range: u16 },
    SelfOnly,
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
        components: Vec<DamageComponentExpr>,
    },
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
    Fixed { amount: u32 },
    Rolled { dice: DiceExpr },
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Effect {
    Damage {
        damage: DamageExpr,
    },
    Heal {
        amount: AmountExpr,
    },
    ApplyCondition {
        condition: ConditionId,
        duration: u64,
    },
    Restore {
        resource: Resource,
        amount: AmountExpr,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConditionDef {
    pub id: ConditionId,
    pub name: String,
    pub modifiers: Vec<Modifier>,
    pub stacking: Stacking,
    pub source_scoped: bool,
    pub descriptors: Vec<DescriptorId>,
    pub capacity: Eviction,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Stacking {
    Refresh,
    Longest,
    Stack { maximum: u16 },
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
    abilities: BTreeMap<AbilityId, usize>,
    talents: BTreeMap<TalentId, usize>,
    creature_types: BTreeMap<TypeId, usize>,
    subtypes: BTreeMap<SubtypeId, usize>,
    classes: BTreeMap<ClassId, usize>,
    conditions: BTreeMap<ConditionId, usize>,
    flags: BTreeMap<FlagId, usize>,
    descriptors: BTreeMap<DescriptorId, usize>,
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
    pub fn talent(&self, id: &TalentId) -> Option<&TalentDef> {
        self.indexes
            .talents
            .get(id)
            .and_then(|&index| self.document.talents.get(index))
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
    pub fn flag(&self, id: &FlagId) -> Option<&FlagDef> {
        self.indexes
            .flags
            .get(id)
            .and_then(|&index| self.document.flags.get(index))
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
        let abilities = catalog!(abilities);
        let talents = catalog!(talents);
        let types = catalog!(creature_types);
        let subtypes = catalog!(subtypes);
        let classes = catalog!(classes);
        let conditions = catalog!(conditions);
        let descriptors = catalog!(descriptors);
        let flags = catalog!(flags);
        let _ = subtypes;
        let refs = References {
            abilities: &abilities,
            types: &types,
            classes: &classes,
            conditions: &conditions,
            descriptors: &descriptors,
            flags: &flags,
        };
        let c = &mut document.constants;
        for turns in [
            c.item_timing.equip.standard_turns,
            c.item_timing.equip.body_armor_turns,
            c.item_timing.unequip.standard_turns,
            c.item_timing.unequip.body_armor_turns,
            c.item_timing.consume_turns,
        ] {
            bound(
                (1..=crate::limits::NUMERIC as u64).contains(&turns),
                "item_timing",
            )?;
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
        bound(
            c.attribute_opportunity_interval > 0
                && usize::from(c.racial_training_points) <= crate::limits::TRAINING_PER_HIT_DIE,
            "advancement",
        )?;
        bound(
            c.duration_scale_numerator > 0 && c.duration_scale_offset > 0,
            "duration_scale",
        )?;
        for id in &mut c.default_abilities {
            reference(&abilities, id, "ability")?;
            id.intern(&mut pool);
        }
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
            bound(
                usize::from(d.training_points) <= crate::limits::TRAINING_PER_HIT_DIE,
                "training_points",
            )?;
            refs.modifiers(&mut d.modifiers, &mut pool)?;
        }
        for d in &mut document.flags {
            refs.modifiers(&mut d.modifiers, &mut pool)?;
        }
        for d in &mut document.talents {
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
            refs.modifiers(&mut d.modifiers, &mut pool)?;
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
        bound(
            document.abilities.iter().filter(|d| d.basic_attack).count() <= 1,
            "basic_attack",
        )?;
        for d in &mut document.abilities {
            bound(
                !d.basic_attack || !matches!(d.reach, Reach::SelfOnly),
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
            if let Some(cost) = &d.cost {
                bound(
                    u64::from(cost.start) + u64::from(cost.resolution)
                        <= crate::limits::NUMERIC as u64,
                    "cost",
                )?;
            }
            if let Reach::Sight { range } = d.reach {
                bound(range > 0 && range <= crate::limits::SIGHT_RANGE, "range")?;
            }
            if let Some(CheckDef {
                against: Against::Difficulty { formula },
                ..
            }) = &d.check
            {
                bound(
                    i64::from(formula.base).abs() <= crate::limits::NUMERIC,
                    "difficulty",
                )?;
            }
            refs.descriptors(&mut d.descriptors, &mut pool)?;
            refs.effects(&mut d.on_success, &mut pool)?;
            refs.effects(&mut d.on_failure, &mut pool)?;
        }
        let talent_order = topological(
            document
                .talents
                .iter()
                .map(|d| (d.id.clone(), d.prerequisites.clone()))
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
                            | ModifierOp::GrantAbility { .. } => None,
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
        let indexes = Indexes {
            abilities: index!(abilities),
            talents: index!(talents),
            creature_types: index!(creature_types),
            subtypes: index!(subtypes),
            classes: index!(classes),
            conditions: index!(conditions),
            flags: index!(flags),
            descriptors: index!(descriptors),
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
                    .all(|modifier| modifier.scope == Scope::Always),
                "permanent_modifier_scope",
            )?;
            rules.expanded_modifier_count(modifiers)?;
        }
        for definition in &rules.document.conditions {
            let expanded = rules.expanded_modifier_count(&definition.modifiers)?;
            let stacks = match definition.stacking {
                Stacking::Stack { maximum } => usize::from(maximum),
                Stacking::Refresh | Stacking::Longest => 1,
            };
            bound(
                stacks.saturating_mul(expanded) <= crate::limits::MODIFIERS_PER_SOURCE,
                "condition_stack_expansion",
            )?;
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
    types: &'a BTreeSet<TypeId>,
    classes: &'a BTreeSet<ClassId>,
    conditions: &'a BTreeSet<ConditionId>,
    descriptors: &'a BTreeSet<DescriptorId>,
    flags: &'a BTreeSet<FlagId>,
}
impl References<'_> {
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
                ModifierOp::Add { stat, .. } | ModifierOp::Set { stat, .. } => {
                    if let Stat::Ability { ability, .. } = stat {
                        reference(self.abilities, ability, "ability")?;
                        ability.intern(pool);
                    }
                }
                ModifierOp::Flag { flag } => {
                    reference(self.flags, flag, "flag")?;
                    flag.intern(pool);
                }
                ModifierOp::Immunity { selector } => self.selector(selector, pool)?,
                ModifierOp::Reduction { selector, .. } => self.selector(selector, pool)?,
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
            Requirement::ClassLevel { class, minimum } => {
                reference(self.classes, class, "class")?;
                class.intern(pool);
                bound(
                    usize::from(*minimum) <= crate::limits::HIT_DICE,
                    "class_level",
                )?;
            }
            Requirement::TypeIn { types } => {
                bound(!types.is_empty(), "type_requirement")?;
                for id in types {
                    reference(self.types, id, "type")?;
                    id.intern(pool);
                }
            }
            Requirement::Flag { flag, .. } => {
                reference(self.flags, flag, "flag")?;
                flag.intern(pool);
            }
            Requirement::TotalHd { minimum } | Requirement::RacialHd { minimum } => bound(
                usize::from(*minimum) <= crate::limits::HIT_DICE,
                "hit_dice_requirement",
            )?,
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
        for value in values {
            match value {
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
                Effect::Heal { amount } | Effect::Restore { amount, .. } => match amount {
                    AmountExpr::Fixed { amount } => {
                        bound(*amount <= crate::limits::NUMERIC as u32, "effect amount")?
                    }
                    AmountExpr::Rolled { dice } => valid_dice(dice)?,
                },
                Effect::ApplyCondition {
                    condition,
                    duration,
                } => {
                    reference(self.conditions, condition, "condition")?;
                    condition.intern(pool);
                    bound(
                        *duration > 0 && *duration <= crate::limits::NUMERIC as u64,
                        "condition_duration",
                    )?;
                }
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
            modifier.scope == Scope::Always
                || matches!(
                    &modifier.op,
                    ModifierOp::Add {
                        stat: Stat::Edge,
                        ..
                    } | ModifierOp::Set {
                        stat: Stat::Edge,
                        ..
                    }
                ),
            "modifier_scope",
        )?;
        match &modifier.op {
            ModifierOp::Add { stat, value } | ModifierOp::Set { stat, value } => {
                valid_modifier_value(stat, *value)?
            }
            ModifierOp::Reduction { amount, .. } => {
                bound(i64::from(*amount) <= crate::limits::NUMERIC, "reduction")?
            }
            ModifierOp::Flag { .. }
            | ModifierOp::Immunity { .. }
            | ModifierOp::GrantAbility { .. } => {}
        }
    }
    Ok(())
}
fn valid_modifier_value(stat: &Stat, value: i64) -> Result<(), RulesetError> {
    let (minimum, maximum) = match stat {
        Stat::Attribute { .. } | Stat::Defense { .. } | Stat::Edge => (
            -crate::limits::STAT_MAGNITUDE,
            crate::limits::STAT_MAGNITUDE,
        ),
        Stat::Health | Stat::ResourceMax { .. } => {
            (-crate::limits::NUMERIC, crate::limits::NUMERIC)
        }
        Stat::Ability { stat, .. } => match stat {
            AbilityStat::Dice => (0, i64::from(crate::limits::DICE - 1)),
            AbilityStat::Duration => (0, crate::limits::NUMERIC),
            AbilityStat::FlatBonus
            | AbilityStat::CheckBonus
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
    fn total_hd(&self) -> u16;
    fn racial_hd(&self) -> u16;
    fn class_level(&self, class: &ClassId) -> u16;
    fn creature_type(&self) -> &TypeId;
    fn skill_rank(&self, skill: &SkillSelector) -> u16;
    fn attribute(&self, attribute: Attribute) -> u16;
    fn has_flag(&self, flag: &FlagId) -> bool;
}
impl Requirement {
    pub fn satisfied(&self, facts: &impl TalentFacts) -> bool {
        match self {
            Self::All { requirements } => requirements.iter().all(|r| r.satisfied(facts)),
            Self::Any { requirements } => requirements.iter().any(|r| r.satisfied(facts)),
            Self::TotalHd { minimum } => facts.total_hd() >= *minimum,
            Self::RacialHd { minimum } => facts.racial_hd() >= *minimum,
            Self::ClassLevel { class, minimum } => facts.class_level(class) >= *minimum,
            Self::TypeIn { types } => types.contains(facts.creature_type()),
            Self::SkillRank { skill, minimum } => facts.skill_rank(skill) >= *minimum,
            Self::AttributeMinimum { attribute, minimum } => {
                facts.attribute(*attribute) >= *minimum
            }
            Self::Flag { flag, present } => facts.has_flag(flag) == *present,
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
                    && def.requirement.satisfied(facts)
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
                .all(|modifier| modifier.scope == Scope::Always),
            "permanent_modifier_scope",
        )?;
        self.validate_condition_modifiers(modifiers)
    }

    /// Condition contributions carry a causer, allowing the supported scoped edge operators.
    pub fn validate_condition_modifiers(&self, modifiers: &[Modifier]) -> Result<(), RulesetError> {
        let abilities = self.indexes.abilities.keys().cloned().collect();
        let types = self.indexes.creature_types.keys().cloned().collect();
        let classes = self.indexes.classes.keys().cloned().collect();
        let conditions = self.indexes.conditions.keys().cloned().collect();
        let descriptors = self
            .document
            .descriptors
            .iter()
            .map(|d| d.id.clone())
            .collect();
        let flags = self.indexes.flags.keys().cloned().collect();
        References {
            abilities: &abilities,
            types: &types,
            classes: &classes,
            conditions: &conditions,
            descriptors: &descriptors,
            flags: &flags,
        }
        .modifiers(&mut modifiers.to_vec(), &mut Interner::default())?;
        self.expanded_modifier_count(modifiers).map(|_| ())
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
        let abilities = self.indexes.abilities.keys().cloned().collect();
        let types = self.indexes.creature_types.keys().cloned().collect();
        let classes = self.indexes.classes.keys().cloned().collect();
        let conditions = self.indexes.conditions.keys().cloned().collect();
        let descriptors = self
            .document
            .descriptors
            .iter()
            .map(|d| d.id.clone())
            .collect();
        let flags = self.indexes.flags.keys().cloned().collect();
        References {
            abilities: &abilities,
            types: &types,
            classes: &classes,
            conditions: &conditions,
            descriptors: &descriptors,
            flags: &flags,
        }
        .effects(&mut effects.to_vec(), &mut Interner::default())
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

impl Effect {
    pub fn valid_shape(&self) -> bool {
        let amount = |value: &AmountExpr| match value {
            AmountExpr::Fixed { amount } => *amount <= crate::limits::NUMERIC as u32,
            AmountExpr::Rolled { dice } => {
                DicePool::new(dice.count, dice.sides, dice.bonus).is_ok()
            }
        };
        match self {
            Self::Heal { amount: value } | Self::Restore { amount: value, .. } => amount(value),
            Self::ApplyCondition { duration, .. } => {
                (1..=crate::limits::NUMERIC as u64).contains(duration)
            }
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

#[cfg(test)]
mod tests {
    use super::*;
    fn document() -> RulesetDocument {
        toml::from_str(DEFAULT_RULESET_TOML).unwrap()
    }
    fn ruleset() -> Arc<Ruleset> {
        Ruleset::validate(document()).unwrap()
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
            scope: Scope::VersusSource,
            op: ModifierOp::Add {
                stat: Stat::Health,
                value: 1,
            },
        };
        assert!(validate_modifier_shapes(&[invalid]).is_err());
        let scoped = Modifier {
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
            id: ConditionId::new("marked").unwrap(),
            name: "Marked".into(),
            modifiers: vec![Modifier {
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
            scope: Scope::Always,
            op: ModifierOp::Flag { flag },
        }];
        source.extend(vec![
            Modifier {
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
        assert_eq!(rules.document.talents.len(), 27);
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
            id: TalentId::new("fixture_hardiness").unwrap(),
            name: "Fixture Hardiness".into(),
            prerequisites: vec![],
            requirement: Requirement::All {
                requirements: vec![],
            },
            modifiers: vec![Modifier {
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
        let next = doc.talents[1].id.clone();
        doc.talents[0].prerequisites.push(next);
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
                "id = \"interactions-v27\"",
                "id = \"interactions-v27\"\nunknown = true"
            ))
            .is_err()
        );
    }
    #[test]
    fn invalid_composite_damage_reuses_kernel_validation() {
        let mut doc = document();
        doc.abilities[2].on_success = vec![Effect::Damage {
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
            scope: Scope::Always,
            op: ModifierOp::Add {
                stat: Stat::Ability {
                    ability: AbilityId::new("fear").unwrap(),
                    stat: AbilityStat::Duration,
                },
                value: -1,
            },
        };
        assert!(validate_modifier_shapes(&[invalid]).is_err());
    }
    #[test]
    fn item_timing_fields_are_required_strict_and_numerically_bounded() {
        let original = serde_json::to_value(document()).unwrap();
        for (group, field) in [
            ("equip", "standard_turns"),
            ("equip", "body_armor_turns"),
            ("unequip", "standard_turns"),
            ("unequip", "body_armor_turns"),
            ("", "consume_turns"),
        ] {
            for invalid in [0, crate::limits::NUMERIC as u64 + 1] {
                let mut value = original.clone();
                let timing = &mut value["constants"]["item_timing"];
                if group.is_empty() {
                    timing[field] = serde_json::json!(invalid);
                } else {
                    timing[group][field] = serde_json::json!(invalid);
                }
                assert!(Ruleset::validate(serde_json::from_value(value).unwrap()).is_err());
            }
        }
        let mut missing = original.clone();
        missing["constants"]
            .as_object_mut()
            .unwrap()
            .remove("item_timing");
        assert!(serde_json::from_value::<RulesetDocument>(missing).is_err());
        let mut unknown = original;
        unknown["constants"]["item_timing"]["equip"]["extra"] = serde_json::json!(1);
        assert!(serde_json::from_value::<RulesetDocument>(unknown).is_err());
    }
    #[test]
    fn authored_effect_and_descriptor_lists_obey_plan_budgets() {
        let mut doc = document();
        doc.abilities[0].on_success = vec![
            Effect::Heal {
                amount: AmountExpr::Fixed { amount: 1 }
            };
            crate::limits::EFFECTS_PER_BRANCH + 1
        ];
        assert!(
            matches!(Ruleset::validate(doc),Err(RulesetError::Bound{field}) if field == "effects")
        );
        let mut doc = document();
        doc.abilities[0].descriptors =
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
