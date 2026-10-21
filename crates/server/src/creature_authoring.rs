//! Independent author-owned creature declarations. Content names are resolved
//! against the scenario's validated ruleset; no catalog is mirrored in code.
mod strikes;
pub(crate) use strikes::bounded_vec as bounded_strike_vec;
pub use strikes::{
    Against, Check, CheckActor, Formula, Natural, Rider, RiderCheck, RiderCondition, RiderTrigger,
    SkillSelector, Strike,
};

use crate::{
    scenario_package::{AnatomySpec, DamageType},
    Failure,
};
use s::{attributes::ByAttribute, content_ids::*};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
use tor_simulation as s;

fn invalid(message: impl AsRef<str>) -> Failure {
    Failure::new(tor_protocol::ErrorCode::InvalidAction, message.as_ref())
}

pub(crate) fn named_map<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> Result<T, D::Error> {
    struct Named<T>(std::marker::PhantomData<T>);
    impl<'de, T: Deserialize<'de>> serde::de::Visitor<'de> for Named<T> {
        type Value = T;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("named authored record")
        }
        fn visit_map<A: serde::de::MapAccess<'de>>(self, fields: A) -> Result<T, A::Error> {
            T::deserialize(serde::de::value::MapAccessDeserializer::new(fields))
        }
    }
    d.deserialize_map(Named(std::marker::PhantomData))
}

pub type Attribute = String;
pub type Skill = String;
pub type ManaBinding = String;
pub type HitDieSource = String;
pub type Talent = String;

fn primitive<T: serde::de::DeserializeOwned>(value: &str, category: &str) -> Result<T, Failure> {
    serde_json::from_value(serde_json::Value::String(value.into()))
        .map_err(|_| invalid(format!("Unknown {category} {value:?}")))
}

pub(crate) fn hit_die_source(
    value: &str,
    rules: &s::rules::Ruleset,
) -> Result<s::progression::HdSource, Failure> {
    if value == "racial" {
        return Ok(s::progression::HdSource::Racial);
    }
    let id = ClassId::new(value).map_err(|_| invalid(format!("Invalid class ID {value:?}")))?;
    if rules.class(&id).is_none() {
        return Err(invalid(format!("Unknown class {value:?}")));
    }
    Ok(s::progression::HdSource::Class(id))
}
pub(crate) fn skill(value: &str) -> Result<s::attributes::Skill, Failure> {
    primitive(value, "skill")
}
pub(crate) fn attribute(value: &str) -> Result<s::attributes::Attribute, Failure> {
    primitive(value, "attribute")
}
pub(crate) fn talent(value: &str, rules: &s::rules::Ruleset) -> Result<TalentId, Failure> {
    let id = TalentId::new(value).map_err(|_| invalid(format!("Invalid talent ID {value:?}")))?;
    if rules.talent(&id).is_none() {
        return Err(invalid(format!("Unknown talent {value:?}")));
    }
    Ok(id)
}

pub(crate) fn expertise(value: &str, rules: &s::rules::Ruleset) -> Result<ExpertiseId, Failure> {
    let id =
        ExpertiseId::new(value).map_err(|_| invalid(format!("Invalid expertise ID {value:?}")))?;
    if rules.expertise(&id).is_none() {
        return Err(invalid(format!("Unknown expertise {value:?}")));
    }
    Ok(id)
}

// Generic mechanism DTOs deliberately contain authored text, not interned IDs
// or persisted actor records. Their vocabulary is independent of game content.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Modifier {
    pub scope: Scope,
    pub op: Op,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    Always,
    VersusSource,
    WhileNoSensedHostile,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Op {
    SignedReduction {
        selector: Selector,
        amount: i32,
    },
    AttributeWithoutDefense {
        attribute: String,
        defense: String,
        value: i64,
    },
    SustainAttribute {
        attribute: String,
    },
    ProtectArmor {},
    HealthRecoveryRate {
        multiplier: u16,
    },
    ActionDuration {
        value: ActionDuration,
    },
    Perception {
        value: Perception,
    },
    Locomotion {
        value: Locomotion,
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
        flag: String,
    },
    Immunity {
        selector: Selector,
    },
    Reduction {
        selector: Selector,
        amount: u32,
    },
    TalentReduction {
        category: String,
        amount: u32,
    },
    GrantAbility {
        ability: String,
    },
}
/// Source-owned action rate, separate from attribute and clock authoring.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionDuration {
    Normal,
    Haste,
    Slow,
}
impl<'de> Deserialize<'de> for ActionDuration {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match String::deserialize(deserializer)?.as_str() {
            "normal" => Ok(Self::Normal),
            "haste" => Ok(Self::Haste),
            "slow" => Ok(Self::Slow),
            _ => Err(serde::de::Error::custom("invalid authored action duration")),
        }
    }
}
/// Authoring and stored schemas remain independent of simulation capabilities.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Perception {
    Invisible {
        enabled: bool,
    },
    SeeInvisible {
        enabled: bool,
    },
    Sight {
        enabled: bool,
    },
    ObjectGuise {
        appearance: Option<GuiseAppearance>,
        skill: String,
        edge: i64,
    },
    AmbushFromGuise {
        enabled: bool,
    },
    ReadiedTouchAmbush {
        enabled: bool,
    },
    ContinualStealth {
        edge: u32,
    },
    Aggravation {
        interval: u64,
        chance_denominator: u32,
        stealth_disadvantage: u32,
    },
    PeriodicSearch {
        interval: u64,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GuiseAppearance {
    pub name: String,
    pub description: String,
    pub class: crate::scenario_package::ItemClass,
    pub asset: Option<String>,
}

// Authoring alone permits omitted movement chance as suppression (TOML has no null).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Locomotion {
    Stationary { enabled: bool },
    Flying { enabled: bool },
    Levitating { enabled: bool },
    Erratic { chance: Option<MovementChance> },
    Confused { chance: Option<MovementChance> },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MovementChance {
    pub numerator: u16,
    pub denominator: u16,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Stat {
    Attribute { attribute: String },
    Defense { defense: String },
    Health,
    HealthRecoveryPeriod,
    HealthRecoveryPeriodPercent,
    HealthRecoveryResetPercent,
    AttackDamageRollBonus,
    NutritionRateThirtieths,
    ResourceMax { resource: String },
    Ability { ability: String, stat: String },
    AbilityTag { tag: String, stat: String },
    Edge,
    CheckEdge,
    IncomingEdge { defense: Option<String> },
}

impl<'de> Deserialize<'de> for Stat {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // Serde's internally tagged unit variants ignore extra fields. Empty
        // struct variants enforce the strict authoring boundary without changing
        // the public enum constructors or canonical serialized representation.
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Recorded {
            Attribute { attribute: String },
            Defense { defense: String },
            Health {},
            HealthRecoveryPeriod {},
            HealthRecoveryPeriodPercent {},
            HealthRecoveryResetPercent {},
            AttackDamageRollBonus {},
            NutritionRateThirtieths {},
            ResourceMax { resource: String },
            Ability { ability: String, stat: String },
            AbilityTag { tag: String, stat: String },
            Edge {},
            CheckEdge {},
            IncomingEdge { defense: Option<String> },
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Selector {
    Category { category: DamageType },
    Descriptor { descriptor: String },
}
impl Modifier {
    pub(crate) fn compile(&self) -> Result<s::rules::Modifier, Failure> {
        let value = serde_json::to_value(self).map_err(|_| invalid("Invalid modifier"))?;
        serde_json::from_value(value).map_err(|error| invalid(format!("Invalid modifier: {error}")))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Melee {
    pub skill: String,
    pub bonus: i32,
    pub wind_up: u64,
    pub recovery: u64,
    pub damage: Damage,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Damage {
    pub components: Vec<Component>,
    pub primary: Option<DamageKey>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DamageKey {
    pub category: DamageType,
    pub descriptor: Option<String>,
    pub sides: Option<u16>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Component {
    pub category: DamageType,
    pub descriptor: Option<String>,
    pub amount: Amount,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Amount {
    Fixed { value: u32 },
    Rolled { count: u16, sides: u16, bonus: i32 },
}
fn descriptor(
    value: &Option<String>,
    rules: &s::rules::Ruleset,
) -> Result<Option<DescriptorId>, Failure> {
    value
        .as_ref()
        .map(|value| {
            let id = DescriptorId::new(value)
                .map_err(|_| invalid(format!("Invalid descriptor {value:?}")))?;
            if !rules
                .document()
                .descriptors
                .iter()
                .any(|definition| definition.id == id)
            {
                return Err(invalid(format!("Unknown descriptor {value:?}")));
            }
            Ok(id)
        })
        .transpose()
}
impl Damage {
    pub(crate) fn compile(
        &self,
        rules: &s::rules::Ruleset,
    ) -> Result<s::damage::DamageSpec, Failure> {
        if self.components.len() > 32 {
            return Err(invalid("Too many damage components"));
        }
        let components = self
            .components
            .iter()
            .map(|component| {
                let descriptor = descriptor(&component.descriptor, rules)?;
                Ok(match component.amount {
                    Amount::Fixed { value } => s::damage::DamageComponent::fixed(
                        component.category.into(),
                        descriptor,
                        value,
                    ),
                    Amount::Rolled {
                        count,
                        sides,
                        bonus,
                    } => s::damage::DamageComponent::rolled(
                        component.category.into(),
                        descriptor,
                        s::dice::DicePool::new(count, sides, bonus)
                            .map_err(|_| invalid("Invalid damage dice"))?,
                    ),
                })
            })
            .collect::<Result<Vec<_>, Failure>>()?;
        let primary = self
            .primary
            .as_ref()
            .map(|key| {
                Ok::<_, Failure>(s::damage::DamageKey {
                    category: key.category.into(),
                    descriptor: descriptor(&key.descriptor, rules)?,
                    sides: key.sides,
                })
            })
            .transpose()?;
        let damage = s::damage::DamageSpec::new(components, primary)
            .map_err(|_| invalid("Invalid damage definition"))?;
        Ok(damage)
    }
}
impl Melee {
    pub(crate) fn compile(
        &self,
        rules: &s::rules::Ruleset,
    ) -> Result<s::attacks::MeleeAttack, Failure> {
        let damage = self.damage.compile(rules)?;
        s::attacks::MeleeAttack::new(
            skill(&self.skill)?,
            self.bonus,
            self.wind_up,
            self.recovery,
            damage,
        )
        .map_err(|_| invalid("Invalid natural attack"))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Disposition {
    Passive,
    Mean,
}
impl<'de> Deserialize<'de> for Disposition {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        match String::deserialize(d)?.as_str() {
            "passive" => Ok(Self::Passive),
            "mean" => Ok(Self::Mean),
            _ => Err(serde::de::Error::custom("invalid Disposition")),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IdleBehavior {
    Wait,
    Explore,
}
impl<'de> Deserialize<'de> for IdleBehavior {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        match String::deserialize(d)?.as_str() {
            "wait" => Ok(Self::Wait),
            "explore" => Ok(Self::Explore),
            _ => Err(serde::de::Error::custom("invalid IdleBehavior")),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BehaviorPolicy {
    pub disposition: Disposition,
    pub idle: IdleBehavior,
    pub memory_ticks: u64,
    pub doors: bool,
    pub care: bool,
    pub equipment: bool,
    pub loot: bool,
    pub gold: bool,
    pub flee_percent: u32,
}
impl<'de> Deserialize<'de> for BehaviorPolicy {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            #[serde(default = "default_disposition")]
            disposition: Disposition,
            #[serde(default = "default_idle")]
            idle: IdleBehavior,
            #[serde(default = "default_memory")]
            memory_ticks: u64,
            #[serde(default)]
            doors: bool,
            #[serde(default)]
            care: bool,
            #[serde(default)]
            equipment: bool,
            #[serde(default)]
            loot: bool,
            #[serde(default)]
            gold: bool,
            #[serde(default = "default_flee")]
            flee_percent: u32,
        }
        let fields: Fields = named_map(d)?;
        if fields.memory_ticks > s::limits::NUMERIC as u64 || fields.flee_percent > 100 {
            return Err(serde::de::Error::custom(
                "invalid behavior memory or flee threshold",
            ));
        }
        Ok(Self {
            disposition: fields.disposition,
            idle: fields.idle,
            memory_ticks: fields.memory_ticks,
            doors: fields.doors,
            care: fields.care,
            equipment: fields.equipment,
            loot: fields.loot,
            gold: fields.gold,
            flee_percent: fields.flee_percent,
        })
    }
}
fn default_disposition() -> Disposition {
    Disposition::Passive
}
fn default_idle() -> IdleBehavior {
    IdleBehavior::Explore
}
fn default_memory() -> u64 {
    1000
}
fn default_flee() -> u32 {
    25
}
impl Default for BehaviorPolicy {
    fn default() -> Self {
        Self {
            disposition: default_disposition(),
            idle: default_idle(),
            memory_ticks: default_memory(),
            doors: false,
            care: false,
            equipment: false,
            loot: false,
            gold: false,
            flee_percent: default_flee(),
        }
    }
}
impl BehaviorPolicy {
    pub(crate) fn compile(self) -> s::behavior::BehaviorPolicy {
        s::behavior::BehaviorPolicy {
            disposition: match self.disposition {
                Disposition::Passive => s::behavior::Disposition::Passive,
                Disposition::Mean => s::behavior::Disposition::Mean,
            },
            idle: match self.idle {
                IdleBehavior::Wait => s::behavior::IdleBehavior::Wait,
                IdleBehavior::Explore => s::behavior::IdleBehavior::Explore,
            },
            memory_ticks: self.memory_ticks,
            doors: self.doors,
            care: self.care,
            equipment: self.equipment,
            loot: self.loot,
            gold: self.gold,
            flee_percent: self.flee_percent,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Species {
    pub behavior: BehaviorPolicy,
    #[serde(default)]
    pub displacement: Option<DisplacementProfile>,
    pub kind: String,
    #[serde(default)]
    pub subtypes: Vec<String>,
    pub attributes: ByAttribute<u16>,
    pub anatomy: Option<AnatomySpec>,
    pub melee: Natural,
    #[serde(default)]
    pub modifiers: Vec<Modifier>,
}
impl<'de> Deserialize<'de> for Species {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            #[serde(default)]
            behavior: BehaviorPolicy,
            #[serde(default)]
            displacement: Option<DisplacementProfile>,
            kind: String,
            #[serde(default)]
            subtypes: Vec<String>,
            attributes: ByAttribute<u16>,
            anatomy: Option<AnatomySpec>,
            melee: Natural,
            #[serde(default)]
            modifiers: Vec<Modifier>,
        }
        let fields: Fields = named_map(d)?;
        Ok(Self {
            behavior: fields.behavior,
            displacement: fields.displacement,
            kind: fields.kind,
            subtypes: fields.subtypes,
            attributes: fields.attributes,
            anatomy: fields.anatomy,
            melee: fields.melee,
            modifiers: fields.modifiers,
        })
    }
}
// Template stages may omit unchanged attributes; base attributes may not.
fn partial_attributes<'de, D, T>(deserializer: D) -> Result<ByAttribute<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de> + Default,
{
    struct Visitor<T>(std::marker::PhantomData<T>);
    impl<'de, T: Deserialize<'de> + Default> serde::de::Visitor<'de> for Visitor<T> {
        type Value = ByAttribute<T>;
        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("named attribute adjustments")
        }
        fn visit_map<M: serde::de::MapAccess<'de>>(
            self,
            mut map: M,
        ) -> Result<Self::Value, M::Error> {
            let mut result = ByAttribute::<T>::default();
            let mut seen = BTreeSet::new();
            while let Some(key) = map.next_key::<String>()? {
                if !seen.insert(key.clone()) {
                    return Err(serde::de::Error::custom("duplicate attribute"));
                }
                let slot = match key.as_str() {
                    "strength" => &mut result.strength,
                    "speed" => &mut result.speed,
                    "intellect" => &mut result.intellect,
                    "willpower" => &mut result.willpower,
                    "awareness" => &mut result.awareness,
                    "presence" => &mut result.presence,
                    _ => {
                        return Err(serde::de::Error::custom(format!(
                            "unknown attribute {key:?}"
                        )));
                    }
                };
                *slot = map.next_value()?;
            }
            Ok(result)
        }
    }
    deserializer.deserialize_map(Visitor(std::marker::PhantomData))
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Template {
    pub priority: i16,
    pub kind: Option<String>,
    #[serde(default)]
    pub add_subtypes: Vec<String>,
    #[serde(default)]
    pub remove_subtypes: Vec<String>,
    #[serde(default, deserialize_with = "partial_attributes")]
    pub adjustments: ByAttribute<i16>,
    #[serde(default, deserialize_with = "partial_attributes")]
    pub overrides: ByAttribute<Option<u16>>,
    #[serde(default)]
    pub modifiers: Vec<Modifier>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Advancement {
    pub source: HitDieSource,
    #[serde(default)]
    pub purchases: Vec<Purchase>,
    pub attribute: Option<Attribute>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Purchase {
    pub pool: String,
    pub selection: PurchaseSelection,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PurchaseSelection {
    Expertise { expertise: String },
    SkillRank { skill: Skill },
    Talent { talent: Talent },
    TalentForm { talent: Talent, form: String },
}
pub(crate) fn compile_choices(
    purchases: &[Purchase],
    selected_attribute: Option<&Attribute>,
    rules: &s::rules::Ruleset,
) -> Result<s::creatures::HdChoices, Failure> {
    Ok(s::creatures::HdChoices {
        purchases: purchases
            .iter()
            .map(|purchase| {
                Ok(s::creatures::AdvancementPurchase {
                    pool: PointPoolId::new(&purchase.pool)
                        .map_err(|_| invalid("Invalid point pool ID"))?,
                    selection: match &purchase.selection {
                        PurchaseSelection::Expertise { expertise: value } => {
                            s::creatures::PurchaseSelection::Expertise(expertise(value, rules)?)
                        }
                        PurchaseSelection::SkillRank { skill: value } => {
                            s::creatures::PurchaseSelection::SkillRank(skill(value)?)
                        }
                        PurchaseSelection::Talent { talent: value } => {
                            s::creatures::PurchaseSelection::Talent(talent(value, rules)?)
                        }
                        PurchaseSelection::TalentForm {
                            talent: value,
                            form,
                        } => s::creatures::PurchaseSelection::TalentForm {
                            talent: talent(value, rules)?,
                            form: s::content_ids::FormId::new(form)
                                .map_err(|_| invalid("Invalid form ID"))?,
                        },
                    },
                })
            })
            .collect::<Result<_, Failure>>()?,
        attribute: selected_attribute
            .map(|value| attribute(value))
            .transpose()?,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Restoration {
    ChooseAgain,
    Authored,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuildSpec {
    #[serde(default)]
    pub future_growth: FutureGrowth,
    pub starting_skills: [Skill; 2],
    pub restoration: Restoration,
    pub nutrition: s::nutrition::NutritionRequirement,
    pub species: String,
    pub name: String,
    pub faction: String,
    pub attributes: Option<ByAttribute<u16>>,
    pub binding: ManaBinding,
    pub hit_dice: Vec<Advancement>,
    #[serde(default)]
    pub templates: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum GrowthSource {
    Racial {},
    Class { class: String },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum FutureGrowth {
    Disabled {},
    Selected { source: GrowthSource },
    Authored {},
}
impl Default for FutureGrowth {
    fn default() -> Self {
        Self::Disabled {}
    }
}
impl FutureGrowth {
    fn compile(&self) -> Result<s::earned_growth::FutureGrowth, Failure> {
        Ok(match self {
            Self::Disabled {} => s::earned_growth::FutureGrowth::Disabled {},
            Self::Authored {} => s::earned_growth::FutureGrowth::Authored {},
            Self::Selected { source } => s::earned_growth::FutureGrowth::Selected {
                source: match source {
                    GrowthSource::Racial {} => s::earned_growth::GrowthSource::Racial {},
                    GrowthSource::Class { class } => s::earned_growth::GrowthSource::Class {
                        class: ClassId::new(class)
                            .map_err(|_| invalid("Invalid future growth class"))?,
                    },
                },
            },
        })
    }
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Catalog {
    #[serde(default)]
    pub species: BTreeMap<String, Species>,
    #[serde(default)]
    pub templates: BTreeMap<String, Template>,
}
fn subtypes(values: &[String]) -> Result<BTreeSet<SubtypeId>, Failure> {
    let compiled = values
        .iter()
        .map(|id| SubtypeId::new(id).map_err(|_| invalid(format!("Invalid subtype {id:?}"))))
        .collect::<Result<BTreeSet<_>, _>>()?;
    if compiled.len() != values.len() {
        return Err(invalid("Duplicate creature subtype"));
    }
    Ok(compiled)
}
#[derive(Clone, Debug)]
pub struct CompiledCatalog {
    content: Arc<s::creatures::Content>,
}
#[derive(Clone, Copy, Debug)]
pub(crate) enum CatalogKind {
    Species,
    Template,
}

impl Catalog {
    pub fn is_empty(&self) -> bool {
        self.species.is_empty() && self.templates.is_empty()
    }
    pub fn compile(&self, rules: Arc<s::rules::Ruleset>) -> Result<CompiledCatalog, Failure> {
        self.compile_with_context(rules, |_, _, failure| failure)
    }

    pub(crate) fn compile_with_context(
        &self,
        rules: Arc<s::rules::Ruleset>,
        context: impl Fn(CatalogKind, &str, Failure) -> Failure,
    ) -> Result<CompiledCatalog, Failure> {
        if self.species.len() > s::limits::CATALOG_ENTRIES
            || self.templates.len() > s::limits::CATALOG_ENTRIES
        {
            return Err(invalid("Creature catalog exceeds definition limits"));
        }
        let mut catalog = s::creatures::ScenarioCatalog::default();
        for (id, author) in &self.species {
            let (compiled_id, compiled) = (|| -> Result<_, Failure> {
                let id = SpeciesId::new(id)
                    .map_err(|_| invalid(format!("Invalid species ID {id:?}")))?;
                let compiled = s::creatures::Species {
                    behavior: author.behavior.compile(),
                    displacement: author.displacement.map(DisplacementProfile::compile),
                    id: id.clone(),
                    kind: TypeId::new(&author.kind)
                        .map_err(|_| invalid("Invalid creature type ID"))?,
                    subtypes: subtypes(&author.subtypes)?,
                    default_attributes: s::attributes::Attributes::new(author.attributes)
                        .map_err(|_| invalid("Invalid initial attributes"))?,
                    anatomy: author
                        .anatomy
                        .as_ref()
                        .map(|anatomy| anatomy.compile(&rules))
                        .transpose()?
                        .unwrap_or_else(|| s::AnatomySpec::from_rules(&rules)),
                    melee: author.melee.compile(&rules)?,
                    modifiers: author
                        .modifiers
                        .iter()
                        .map(Modifier::compile)
                        .collect::<Result<_, _>>()?,
                };
                Ok((id, compiled))
            })()
            .map_err(|failure| context(CatalogKind::Species, id, failure))?;
            catalog.species.insert(compiled_id, compiled);
        }
        for (id, author) in &self.templates {
            let (compiled_id, compiled) = (|| -> Result<_, Failure> {
                let id = TemplateId::new(id)
                    .map_err(|_| invalid(format!("Invalid template ID {id:?}")))?;
                let compiled = s::creatures::Template {
                    id: id.clone(),
                    priority: author.priority,
                    kind: author
                        .kind
                        .as_ref()
                        .map(|id| TypeId::new(id).map_err(|_| invalid("Invalid creature type ID")))
                        .transpose()?,
                    add_subtypes: subtypes(&author.add_subtypes)?,
                    remove_subtypes: subtypes(&author.remove_subtypes)?,
                    adjustments: author.adjustments,
                    overrides: author.overrides,
                    modifiers: author
                        .modifiers
                        .iter()
                        .map(Modifier::compile)
                        .collect::<Result<_, _>>()?,
                };
                Ok((id, compiled))
            })()
            .map_err(|failure| context(CatalogKind::Template, id, failure))?;
            catalog.templates.insert(compiled_id, compiled);
        }
        // Diagnose a rejected definition by its typed owner, without interpreting
        // error strings. The extra isolated checks run only after validation fails.
        if let Err(error) = catalog.validate(&rules) {
            for (id, species) in &catalog.species {
                let candidate = s::creatures::ScenarioCatalog {
                    species: BTreeMap::from([(id.clone(), species.clone())]),
                    ..Default::default()
                };
                if let Err(error) = candidate.validate(&rules) {
                    return Err(context(
                        CatalogKind::Species,
                        id.as_str(),
                        invalid(format!("Invalid creature catalog: {error:?}")),
                    ));
                }
            }
            for (id, template) in &catalog.templates {
                let candidate = s::creatures::ScenarioCatalog {
                    templates: BTreeMap::from([(id.clone(), template.clone())]),
                    ..Default::default()
                };
                if let Err(error) = candidate.validate(&rules) {
                    return Err(context(
                        CatalogKind::Template,
                        id.as_str(),
                        invalid(format!("Invalid creature catalog: {error:?}")),
                    ));
                }
            }
            return Err(invalid(format!("Invalid creature catalog: {error:?}")));
        }
        let content = s::creatures::Content::new(rules, catalog)
            .map_err(|error| invalid(format!("Invalid creature catalog: {error:?}")))?;
        Ok(CompiledCatalog { content })
    }
}
#[derive(Clone, Debug)]
pub struct Recipe {
    prototype: s::creatures::CreatureBuild,
    identity: s::CreatureIdentity,
    nutrition: s::nutrition::NutritionRequirement,
}
impl CompiledCatalog {
    pub(crate) fn with_trap_ammunition(
        mut self,
        ammunition: BTreeMap<String, s::ItemSpec>,
    ) -> Result<Self, Failure> {
        self.content = s::creatures::Content::with_trap_ammunition(&self.content, ammunition)
            .map_err(|error| invalid(format!("Invalid trap ammunition catalog: {error:?}")))?;
        Ok(self)
    }
    pub fn content(&self) -> &Arc<s::creatures::Content> {
        &self.content
    }
    pub(crate) fn template(&self, id: &str) -> Option<&s::creatures::Template> {
        self.content
            .catalog()
            .templates
            .get(&TemplateId::new(id).ok()?)
    }
    pub fn prepare(&self, author: &BuildSpec) -> Result<Recipe, Failure> {
        if author.nutrition == s::nutrition::NutritionRequirement::Required
            && matches!(
                self.content.rules().document().nutrition,
                s::nutrition::NutritionPolicy::Disabled
            )
        {
            return Err(invalid("Required nutrition needs an enabled policy"));
        }
        if author.hit_dice.is_empty()
            || author.hit_dice.len() > s::limits::HIT_DICE
            || author.templates.len() > s::limits::TEMPLATES
        {
            return Err(invalid(
                "Authored creatures require 1 to 256 hit dice and at most 32 templates",
            ));
        }
        for (label, value, maximum) in [
            ("name", &author.name, s::CreatureIdentity::MAX_NAME_BYTES),
            (
                "faction",
                &author.faction,
                s::CreatureIdentity::MAX_FACTION_BYTES,
            ),
        ] {
            if value.is_empty() || value.len() > maximum || value.chars().any(char::is_control) {
                return Err(invalid(format!(
                    "Creature {label} must be a nonempty label of at most {maximum} bytes without control characters"
                )));
            }
        }
        let id = SpeciesId::new(&author.species).map_err(|_| invalid("Invalid species ID"))?;
        let species = self
            .content
            .catalog()
            .species
            .get(&id)
            .ok_or_else(|| invalid(format!("Unknown creature species {:?}", author.species)))?;
        let attributes = author
            .attributes
            .map(s::attributes::Attributes::new)
            .transpose()
            .map_err(|_| invalid("Invalid initial attributes"))?
            .unwrap_or(species.default_attributes);
        let templates = author
            .templates
            .iter()
            .map(|id| TemplateId::new(id).map_err(|_| invalid("Invalid template ID")))
            .collect::<Result<Vec<_>, _>>()?;
        let sources = author
            .hit_dice
            .iter()
            .map(|entry| hit_die_source(&entry.source, self.content.rules()))
            .collect::<Result<Vec<_>, _>>()?;
        let ledger = s::progression::HdLedger::seeded(sources, 0)
            .map_err(|_| invalid("Invalid creature hit-dice ledger"))?;
        let choices = author
            .hit_dice
            .iter()
            .map(|entry| {
                compile_choices(
                    &entry.purchases,
                    entry.attribute.as_ref(),
                    self.content.rules(),
                )
            })
            .collect::<Result<Vec<_>, Failure>>()?;
        let mut prototype = s::creatures::CreatureBuild::from_recorded(
            self.content.clone(),
            id,
            attributes,
            ledger,
            primitive(&author.binding, "mana binding")?,
            s::creatures::BuildAdvancement {
                starting_skills: [
                    skill(&author.starting_skills[0])?,
                    skill(&author.starting_skills[1])?,
                ],
                restoration: match author.restoration {
                    Restoration::ChooseAgain => s::creatures::RestorationPolicy::ChooseAgain,
                    Restoration::Authored => s::creatures::RestorationPolicy::Authored(
                        author
                            .hit_dice
                            .iter()
                            .zip(&choices)
                            .map(|(entry, choices)| {
                                Ok(s::creatures::PlannedAdvancement {
                                    source: hit_die_source(&entry.source, self.content.rules())?,
                                    choices: choices.clone(),
                                })
                            })
                            .collect::<Result<_, Failure>>()?,
                    ),
                },
                choices,
            },
            templates,
        )
        .map_err(|error| invalid(format!("Invalid authored creature build: {error:?}")))?;
        prototype
            .set_future_growth(author.future_growth.compile()?)
            .map_err(|error| invalid(format!("Invalid future growth policy: {error:?}")))?;
        let derived = prototype
            .derive()
            .map_err(|error| invalid(format!("Invalid authored creature build: {error:?}")))?;
        if !derived.dormant_talents.is_empty() {
            return Err(invalid(
                "Initial talent choices must be eligible in the authored build",
            ));
        }
        Ok(Recipe {
            prototype,
            nutrition: author.nutrition,
            identity: s::CreatureIdentity {
                name: author.name.clone(),
                faction: author.faction.clone(),
            },
        })
    }
}
impl Recipe {
    pub fn behavior(&self) -> s::behavior::BehaviorPolicy {
        self.prototype.species().behavior
    }
    pub fn nutrition(&self) -> s::nutrition::NutritionRequirement {
        self.nutrition
    }
    pub fn identity(&self) -> &s::CreatureIdentity {
        &self.identity
    }
    pub fn recorded(&self) -> s::creatures::record::BuildRecord {
        s::creatures::record::BuildRecord::capture(&self.prototype)
    }
    pub fn instantiate(&self, seed: u64) -> Result<s::creatures::CreatureBuild, Failure> {
        self.prototype
            .instantiate_seeded(seed)
            .map_err(|error| invalid(format!("Invalid creature spawn: {error:?}")))
    }
}

/// Stable, domain-separated spawn stream. Actor allocation order, region
/// loading and combat draws cannot change an already assigned actor's HD seeds.
pub fn actor_health_seed(game_seed: u64, actor: u64) -> u64 {
    let mut hash = Sha256::new();
    hash.update(b"tor-creature-health-v1\0");
    hash.update(game_seed.to_le_bytes());
    hash.update(actor.to_le_bytes());
    let bytes = hash.finalize();
    u64::from_le_bytes(bytes[..8].try_into().expect("eight digest bytes"))
}

#[cfg(test)]
mod strict_stat_tests {
    use super::*;

    #[test]
    fn creature_stat_forms_preserve_authoring_shapes_and_compile() {
        for stat in [
            Stat::Health,
            Stat::HealthRecoveryPeriod,
            Stat::HealthRecoveryPeriodPercent,
            Stat::HealthRecoveryResetPercent,
            Stat::AttackDamageRollBonus,
            Stat::NutritionRateThirtieths,
            Stat::Edge,
            Stat::CheckEdge,
            Stat::IncomingEdge { defense: None },
            Stat::IncomingEdge {
                defense: Some("physical".into()),
            },
            Stat::Attribute {
                attribute: "strength".into(),
            },
            Stat::Defense {
                defense: "cognitive".into(),
            },
            Stat::ResourceMax {
                resource: "stamina".into(),
            },
            Stat::Ability {
                ability: "fear".into(),
                stat: "check_edge".into(),
            },
            Stat::AbilityTag {
                tag: "weapon_attack".into(),
                stat: "check_bonus".into(),
            },
        ] {
            let json = serde_json::to_value(&stat).unwrap();
            assert_eq!(serde_json::from_value::<Stat>(json.clone()).unwrap(), stat);
            let source = toml::to_string(&stat).unwrap();
            assert_eq!(toml::from_str::<Stat>(&source).unwrap(), stat);
            assert_eq!(
                serde_json::to_value(serde_json::from_value::<Stat>(json.clone()).unwrap())
                    .unwrap(),
                json
            );
            Modifier {
                scope: Scope::Always,
                op: Op::Add { stat, value: 1 },
            }
            .compile()
            .unwrap();
        }
    }

    #[test]
    fn creature_stat_authoring_rejects_unknown_fields_in_json_and_toml() {
        let mut accepted = Vec::new();
        for kind in [
            "health",
            "edge",
            "check_edge",
            "health_recovery_period",
            "health_recovery_period_percent",
            "health_recovery_reset_percent",
            "attack_damage_roll_bonus",
            "nutrition_rate_thirtieths",
        ] {
            let json = serde_json::json!({"kind": kind, "private_owner": 999});
            if serde_json::from_value::<Stat>(json).is_ok() {
                accepted.push((kind, "json"));
            }
            let toml = format!("kind = '{kind}'\nprivate_owner = 999\n");
            if toml::from_str::<Stat>(&toml).is_ok() {
                accepted.push((kind, "toml"));
            }
        }
        assert!(
            accepted.is_empty(),
            "unknown fields accepted for {accepted:?}"
        );
    }
}

#[cfg(test)]
mod ordered_strike_authoring_tests {
    use super::*;
    use serde_json::{json, Value};
    fn source() -> Value {
        json!({"species": {"sequence_test": {"kind": "humanoid", "behavior": {"disposition":"mean","idle":"explore","memory_ticks":1000},
            "attributes": {"strength": 1, "speed": 1, "intellect": 1, "willpower": 1, "awareness": 1, "presence": 1},
            "melee": {"skill": "heavy_weaponry", "bonus": 0, "wind_up": 10, "recovery": 20,
                "strikes": [{"offset": 0, "unlock": null, "check": null, "on_success": [], "on_failure": [], "descriptors": [], "riders": []}]}
        }}, "templates": {}})
    }
    fn rules() -> Arc<s::rules::Ruleset> {
        s::rules::Ruleset::validate(toml::from_str(s::rules::DEFAULT_RULESET_TOML).unwrap())
            .unwrap()
    }
    #[test]
    fn sequence_rider_authoring_maps_optional_source_gate_without_weakening_bounds() {
        for minimum in [None, Some(1), Some(2), Some(16)] {
            let mut value = source();
            let row = &mut value["species"]["sequence_test"]["melee"]["strikes"][0];
            row["riders"] = json!([{"trigger":"success","effects":[],"descriptors":[]}]);
            if let Some(minimum) = minimum {
                row["riders"][0]["condition"] =
                    json!({"kind":"sequence_successful_hits","minimum":minimum});
            }
            let authored: Catalog = serde_json::from_value(value).unwrap();
            let compiled = authored.compile(rules()).unwrap();
            let id = SpeciesId::new("sequence_test").unwrap();
            let rider = &compiled.content.catalog().species[&id].melee.strikes()[0].riders[0];
            assert_eq!(
                rider.condition,
                minimum.map(|minimum| {
                    s::rules::RiderCondition::SequenceSuccessfulHits { minimum }
                })
            );
            let roundtrip: Catalog = toml::from_str(&toml::to_string(&authored).unwrap()).unwrap();
            assert_eq!(
                roundtrip.compile(rules()).unwrap().content.catalog(),
                compiled.content.catalog()
            );
        }
        for minimum in [0, 17, u16::MAX] {
            let mut value = source();
            value["species"]["sequence_test"]["melee"]["strikes"][0]["riders"] = json!([{
                "trigger":"success","condition":{"kind":"sequence_successful_hits","minimum":minimum},
                "effects":[],"descriptors":[]}]);
            assert!(serde_json::from_value::<Catalog>(value)
                .unwrap()
                .compile(rules())
                .is_err());
        }
        let mut value = source();
        let row = &mut value["species"]["sequence_test"]["melee"]["strikes"][0];
        row["check"] = json!({"actor":"user","skill":{"kind":"natural_attack"},
            "against":{"kind":"defense","defense":"physical"}});
        row["riders"] = json!([{"trigger":"failure",
            "condition":{"kind":"sequence_successful_hits","minimum":2},"effects":[],"descriptors":[]}]);
        assert!(serde_json::from_value::<Catalog>(value)
            .unwrap()
            .compile(rules())
            .is_err());
    }

    #[test]
    fn ordered_strikes_natural_rider_trigger_is_explicit_and_compiles_without_inversion() {
        for trigger in ["success", "failure"] {
            let mut value = source();
            let strike = &mut value["species"]["sequence_test"]["melee"]["strikes"][0];
            strike["check"] = json!({"actor":"target","skill":{"kind":"named","skill":"athletics"},
                "against":{"kind":"difficulty","formula":{"base":10,"skill":null,"attribute":null}}});
            strike["riders"] =
                json!([{"trigger":trigger,"check":null,"effects":[],"descriptors":[]}]);
            let catalog: Catalog = serde_json::from_value(value.clone())
                .expect("explicit natural trigger must decode");
            catalog.compile(rules()).unwrap();
            assert_eq!(
                serde_json::to_value(&catalog).unwrap()["species"]["sequence_test"]["melee"]
                    ["strikes"][0]["riders"][0]["trigger"],
                trigger
            );
            let mut missing = value.clone();
            missing["species"]["sequence_test"]["melee"]["strikes"][0]["riders"][0]
                .as_object_mut()
                .unwrap()
                .remove("trigger");
            assert!(serde_json::from_value::<Catalog>(missing).is_err());
            value["species"]["sequence_test"]["melee"]["strikes"][0]["check"] = Value::Null;
            assert_eq!(
                serde_json::from_value::<Catalog>(value)
                    .unwrap()
                    .compile(rules())
                    .is_ok(),
                trigger == "success"
            );
        }
    }
    #[test]
    fn ordered_strike_content_authoring_accepts_checkless_natural_source_and_retains_identity() {
        let value = source();
        let catalog: Catalog =
            serde_json::from_value(value.clone()).expect("natural strike syntax must decode");
        catalog
            .compile(rules())
            .expect("a no-damage natural source is valid");
        let decoded = serde_json::to_value(&catalog).unwrap();
        assert_eq!(
            decoded["species"]["sequence_test"]["melee"],
            value["species"]["sequence_test"]["melee"]
        );
        let text = toml::to_string(&catalog).unwrap();
        assert_eq!(
            serde_json::to_value(toml::from_str::<Catalog>(&text).unwrap()).unwrap(),
            serde_json::to_value(catalog).unwrap()
        );
    }
    #[test]
    fn ordered_strike_content_authoring_rejects_unknown_unlocks_and_old_natural_shape() {
        let value = source();
        let valid: Catalog =
            serde_json::from_value(value.clone()).expect("new source syntax must decode");
        valid.compile(rules()).unwrap();
        let mut unknown = value.clone();
        let mut second = unknown["species"]["sequence_test"]["melee"]["strikes"][0].clone();
        second["offset"] = json!(10);
        second["unlock"] = json!("absent_talent");
        unknown["species"]["sequence_test"]["melee"]["strikes"]
            .as_array_mut()
            .unwrap()
            .push(second);
        assert!(serde_json::from_value::<Catalog>(unknown)
            .unwrap()
            .compile(rules())
            .is_err());
        let mut old = value;
        old["species"]["sequence_test"]["melee"]["damage"] =
            json!({"components": [], "primary": null});
        assert!(serde_json::from_value::<Catalog>(old).is_err());
    }
    #[test]
    fn ordered_strike_content_authored_ordinals_and_checked_riders_survive_compilation() {
        let mut value = source();
        let first = &mut value["species"]["sequence_test"]["melee"]["strikes"][0];
        first["check"] = json!({"actor": "user", "skill": {"kind": "natural_attack"}, "against": {"kind": "defense", "defense": "physical"}});
        first["riders"] = json!([{"trigger": "success", "check": {"skill": {"kind": "named", "skill": "spellcasting"}, "defense": "spiritual"}, "effects": [{"kind": "apply_condition", "condition": "fear", "duration": 30}], "descriptors": ["fear"]}]);
        let mut second = first.clone();
        second["offset"] = json!(10);
        second["unlock"] = json!("rattling_blow");
        let mut third = first.clone();
        third["offset"] = json!(10);
        value["species"]["sequence_test"]["melee"]["strikes"]
            .as_array_mut()
            .unwrap()
            .extend([second, third]);
        let author: Catalog = serde_json::from_value(value).unwrap();
        let compiled = author.compile(rules()).unwrap();
        let id = SpeciesId::new("sequence_test").unwrap();
        let natural = &compiled.content.catalog().species[&id].melee;
        assert_eq!(
            natural
                .strikes()
                .iter()
                .map(|strike| strike.offset)
                .collect::<Vec<_>>(),
            vec![0, 10, 10]
        );
        assert_eq!(
            natural.strikes()[1].unlock.as_ref().unwrap().as_str(),
            "rattling_blow"
        );
        assert!(natural.strikes()[2].unlock.is_none());
        assert_eq!(
            natural.strikes()[0].riders[0]
                .check
                .as_ref()
                .unwrap()
                .defense,
            s::rules::DefenseKind::Spiritual
        );
        let roundtrip: Catalog = toml::from_str(&toml::to_string(&author).unwrap()).unwrap();
        assert_eq!(
            roundtrip.compile(rules()).unwrap().content.catalog(),
            compiled.content.catalog()
        );
    }
    #[test]
    fn ordered_strike_content_natural_check_selector_rejects_unknown_fields() {
        for kind in ["selected_weapon", "natural_attack"] {
            let mut value = source();
            value["species"]["sequence_test"]["melee"]["strikes"][0]["riders"] = json!([{"trigger": "success", "check": {"skill": {"kind": kind, "actor": "target"}, "defense": "physical"}, "effects": [], "descriptors": []}]);
            assert!(serde_json::from_value::<Catalog>(value).is_err(), "{kind}");
        }
    }
}

/// Optional explicit physical eligibility. Absence is never inferred from mass,
/// footprint, hit dice, creature kind or its content identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CreatureSize {
    Tiny,
    Small,
    Medium,
    Large,
    Huge,
    Colossal,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DisplacementProfile {
    pub size: CreatureSize,
    pub anchored: bool,
}
impl<'de> Deserialize<'de> for DisplacementProfile {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            size: CreatureSize,
            anchored: bool,
        }
        struct Named;
        impl<'de> serde::de::Visitor<'de> for Named {
            type Value = Fields;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a named displacement profile")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(self, map: A) -> Result<Fields, A::Error> {
                Fields::deserialize(serde::de::value::MapAccessDeserializer::new(map))
            }
        }
        let fields = deserializer.deserialize_map(Named)?;
        Ok(Self {
            size: fields.size,
            anchored: fields.anchored,
        })
    }
}
impl DisplacementProfile {
    fn compile(self) -> s::creatures::DisplacementProfile {
        s::creatures::DisplacementProfile {
            size: match self.size {
                CreatureSize::Tiny => s::creatures::CreatureSize::Tiny,
                CreatureSize::Small => s::creatures::CreatureSize::Small,
                CreatureSize::Medium => s::creatures::CreatureSize::Medium,
                CreatureSize::Large => s::creatures::CreatureSize::Large,
                CreatureSize::Huge => s::creatures::CreatureSize::Huge,
                CreatureSize::Colossal => s::creatures::CreatureSize::Colossal,
            },
            anchored: self.anchored,
        }
    }
}

#[cfg(test)]
mod action_duration_authoring_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn action_duration_authoring_requires_exact_scalar_names() {
        for rate in ["normal", "haste", "slow"] {
            let value = json!(rate);
            let decoded: ActionDuration = serde_json::from_value(value.clone()).unwrap();
            assert_eq!(serde_json::to_value(decoded).unwrap(), value);
            let modifier = json!({
                "scope":"always", "op":{"kind":"action_duration","value":rate}
            });
            let decoded: Modifier = serde_json::from_value(modifier.clone()).unwrap();
            assert!(decoded.compile().is_ok());
            assert_eq!(serde_json::to_value(decoded).unwrap(), modifier);
            let authored: Modifier = toml::from_str(&format!(
                "scope = 'always'\nop = {{ kind = 'action_duration', value = '{rate}' }}"
            ))
            .unwrap();
            assert!(authored.compile().is_ok());
            for invalid in [
                json!({(rate):null}),
                json!([rate]),
                json!(null),
                json!(true),
                json!(1),
                json!("fast"),
            ] {
                assert!(serde_json::from_value::<ActionDuration>(invalid.clone()).is_err());
                let mut malformed = modifier.clone();
                malformed["op"]["value"] = invalid;
                assert!(serde_json::from_value::<Modifier>(malformed).is_err());
            }
        }
    }
}

#[cfg(test)]
mod locomotion_authoring_tests {
    use super::*;
    #[test]
    fn omitted_confused_chance_is_authoring_only_suppression() {
        let authored: Modifier = toml::from_str(
            "scope = 'always'\nop = { kind = 'locomotion', value = { kind = 'confused' } }",
        )
        .unwrap();
        assert_eq!(
            authored.compile().unwrap().op,
            s::rules::ModifierOp::Locomotion {
                value: s::locomotion::LocomotionValue::Confused { chance: None }
            }
        );
        let bad: Modifier = toml::from_str("scope = 'always'\nop = { kind = 'locomotion', value = { kind = 'confused', chance = { numerator = 5, denominator = 4 } } }").unwrap();
        assert!(bad.compile().is_err());
    }
    #[test]
    fn omitted_erratic_chance_is_authoring_only_suppression() {
        let authored: Modifier = toml::from_str(
            "scope = 'always'\nop = { kind = 'locomotion', value = { kind = 'erratic' } }",
        )
        .unwrap();
        assert_eq!(
            authored.compile().unwrap().op,
            s::rules::ModifierOp::Locomotion {
                value: s::locomotion::LocomotionValue::Erratic { chance: None }
            }
        );
        assert!(toml::from_str::<Modifier>("scope = 'always'\nop = { kind = 'locomotion', value = { kind = 'erratic', enabled = true, chance = { numerator = 1, denominator = 2 } } }").is_err());
        let bad: Modifier = toml::from_str("scope = 'always'\nop = { kind = 'locomotion', value = { kind = 'erratic', chance = { numerator = 2, denominator = 1 } } }").unwrap();
        assert!(bad.compile().is_err());
    }
}

#[cfg(test)]
mod expertise_purchase_tests {
    use super::*;
    #[test]
    fn authored_expertise_purchase_resolves_exact_known_catalog_id() {
        let mut document: s::rules::RulesetDocument =
            toml::from_str(s::rules::DEFAULT_RULESET_TOML).unwrap();
        document.expertises.push(s::expertise::ExpertiseDef {
            id: ExpertiseId::new("fixture.knife").unwrap(),
            name: "Knife".into(),
            category: s::expertise::ExpertiseCategory::Equipment,
        });
        let rules = s::rules::Ruleset::validate(document).unwrap();
        let row = |id: &str| Purchase {
            pool: "advancement".into(),
            selection: PurchaseSelection::Expertise {
                expertise: id.into(),
            },
        };
        let choices = compile_choices(&[row("fixture.knife")], None, &rules).unwrap();
        assert!(
            matches!(&choices.purchases[0].selection, s::creatures::PurchaseSelection::Expertise(id) if id.as_str() == "fixture.knife")
        );
        for id in ["fixture.missing", "INVALID"] {
            assert!(compile_choices(&[row(id)], None, &rules).is_err());
        }
        assert!(serde_json::from_str::<PurchaseSelection>(
            r#"{"kind":"expertise","expertise":"fixture.knife","free":true}"#
        )
        .is_err());
    }
}

#[cfg(test)]
mod wearable_modifier_tests {
    use super::*;
    #[test]
    fn shared_wearable_modifiers_round_trip_and_compile_without_ring_identity() {
        let ops = [
            Op::SignedReduction {
                selector: Selector::Category {
                    category: DamageType::Psychic,
                },
                amount: -1,
            },
            Op::AttributeWithoutDefense {
                attribute: "speed".into(),
                defense: "physical".into(),
                value: 2,
            },
            Op::SustainAttribute {
                attribute: "strength".into(),
            },
            Op::ProtectArmor {},
            Op::HealthRecoveryRate { multiplier: 2 },
        ];
        for op in ops {
            let modifier = Modifier {
                scope: Scope::Always,
                op,
            };
            let value = serde_json::to_value(&modifier).unwrap();
            let restored: Modifier = serde_json::from_value(value.clone()).unwrap();
            assert_eq!(restored, modifier);
            assert!(restored.compile().is_ok());
            let mut extra = value;
            extra["op"]["secret_ring"] = serde_json::json!(true);
            assert!(serde_json::from_value::<Modifier>(extra).is_err());
        }
    }
}

#[cfg(test)]
mod awareness_authoring_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn authored_awareness_capabilities_compile_and_reject_invalid_policies() {
        for good in [
            json!({"kind":"continual_stealth","edge":1}),
            json!({"kind":"aggravation","interval":500,"chance_denominator":3,"stealth_disadvantage":1}),
        ] {
            let modifier = json!({"scope":"always","op":{"kind":"perception","value":good}});
            let authored: Modifier = serde_json::from_value(modifier.clone()).unwrap();
            assert_eq!(serde_json::to_value(&authored).unwrap(), modifier);
            assert!(authored.compile().is_ok());
            for field in good.as_object().unwrap().keys() {
                let mut missing = modifier.clone();
                missing["op"]["value"]
                    .as_object_mut()
                    .unwrap()
                    .remove(field);
                assert!(serde_json::from_value::<Modifier>(missing).is_err());
            }
            let mut extra = modifier;
            extra["op"]["value"]["unknown"] = json!(true);
            assert!(serde_json::from_value::<Modifier>(extra).is_err());
        }
        for bad in [
            json!({"kind":"aggravation","interval":0,"chance_denominator":3,"stealth_disadvantage":1}),
            json!({"kind":"aggravation","interval":500,"chance_denominator":0,"stealth_disadvantage":1}),
        ] {
            let authored: Modifier = serde_json::from_value(
                json!({"scope":"always","op":{"kind":"perception","value":bad}}),
            )
            .unwrap();
            assert!(authored.compile().is_err());
        }
    }
}
