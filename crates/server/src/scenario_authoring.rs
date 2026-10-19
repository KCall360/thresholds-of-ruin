//! Author-owned declaration schemas. Conversions into simulation definitions are
//! explicit so runtime representation changes cannot silently alter package files.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemClass {
    #[default]
    Misc,
    Weapon,
    Armor,
    Potion,
    Food,
    Corpse,
    Tool,
    Amulet,
    Ring,
    Scroll,
    Spellbook,
    Wand,
    Coin,
    Gem,
}

impl From<ItemClass> for tor_simulation::ItemClass {
    fn from(value: ItemClass) -> Self {
        match value {
            ItemClass::Misc => Self::Misc,
            ItemClass::Weapon => Self::Weapon,
            ItemClass::Armor => Self::Armor,
            ItemClass::Potion => Self::Potion,
            ItemClass::Food => Self::Food,
            ItemClass::Corpse => Self::Corpse,
            ItemClass::Tool => Self::Tool,
            ItemClass::Amulet => Self::Amulet,
            ItemClass::Ring => Self::Ring,
            ItemClass::Scroll => Self::Scroll,
            ItemClass::Spellbook => Self::Spellbook,
            ItemClass::Wand => Self::Wand,
            ItemClass::Coin => Self::Coin,
            ItemClass::Gem => Self::Gem,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CurrencyKind {
    Gold,
}
impl From<CurrencyKind> for tor_simulation::CurrencyKind {
    fn from(value: CurrencyKind) -> Self {
        match value {
            CurrencyKind::Gold => Self::Gold,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DamageType {
    Energy,
    Impact,
    Keen,
    Spirit,
    Vital,
    Psychic,
}

impl From<DamageType> for tor_simulation::combat::DamageType {
    fn from(value: DamageType) -> Self {
        match value {
            DamageType::Energy => Self::Energy,
            DamageType::Impact => Self::Impact,
            DamageType::Keen => Self::Keen,
            DamageType::Spirit => Self::Spirit,
            DamageType::Vital => Self::Vital,
            DamageType::Psychic => Self::Psychic,
        }
    }
}

/// Explicit source-owned equipment birth policy; never modifies an archetype.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum EquipmentGeneration {
    Authored {
        #[serde(default)]
        hit_enchantment: i32,
        #[serde(default)]
        damage_enchantment: i32,
    },
    Weapon {
        curse_percent: u8,
        enchant_percent: u8,
        minimum: u32,
        maximum: u32,
    },
    Armor {
        curse_percent: u8,
        enchant_percent: u8,
        minimum: u32,
        maximum: u32,
    },
}
impl EquipmentGeneration {
    pub(crate) fn compile(self) -> Result<tor_simulation::EquipmentGeneration, crate::Failure> {
        let value = match self {
            Self::Authored {
                hit_enchantment,
                damage_enchantment,
            } => tor_simulation::EquipmentGeneration::Authored {
                hit_enchantment,
                damage_enchantment,
            },
            Self::Weapon {
                curse_percent,
                enchant_percent,
                minimum,
                maximum,
            } => tor_simulation::EquipmentGeneration::Weapon {
                curse_percent,
                enchant_percent,
                minimum,
                maximum,
            },
            Self::Armor {
                curse_percent,
                enchant_percent,
                minimum,
                maximum,
            } => tor_simulation::EquipmentGeneration::Armor {
                curse_percent,
                enchant_percent,
                minimum,
                maximum,
            },
        };
        super::require(value.valid(), "Invalid equipment generation policy")?;
        Ok(value)
    }
}

pub type EquipmentSlot = String;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnatomySpec {
    pub slots: Vec<EquipmentSlot>,
}
impl AnatomySpec {
    pub(crate) fn compile(
        &self,
        rules: &tor_simulation::rules::Ruleset,
    ) -> Result<tor_simulation::AnatomySpec, crate::Failure> {
        let slots = self
            .slots
            .iter()
            .map(|slot| {
                let id = tor_simulation::EquipmentSlot::new(slot)
                    .map_err(|_| super::fail(format!("Invalid equipment slot ID {slot:?}")))?;
                if rules.equipment_slot(&id).is_none() {
                    return Err(super::fail(format!("Unknown equipment slot {slot:?}")));
                }
                Ok(id)
            })
            .collect::<Result<Vec<_>, crate::Failure>>()?;
        let anatomy = tor_simulation::AnatomySpec { slots };
        if !anatomy.valid(rules) {
            return Err(super::fail("Invalid anatomy"));
        }
        Ok(anatomy)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArmorMaterial {
    Metal,
    Nonmetal,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArmorSpec {
    pub physical_reduction: u32,
    pub material: ArmorMaterial,
    #[serde(default)]
    pub rustproof: bool,
}
impl ArmorSpec {
    fn compile(self) -> Result<tor_simulation::ArmorSpec, crate::Failure> {
        let armor = tor_simulation::ArmorSpec {
            physical_reduction: self.physical_reduction,
            material: match self.material {
                ArmorMaterial::Metal => tor_simulation::ArmorMaterial::Metal,
                ArmorMaterial::Nonmetal => tor_simulation::ArmorMaterial::Nonmetal,
            },
            rustproof: self.rustproof,
        };
        if !armor.valid() {
            return Err(super::fail("Invalid armor reduction"));
        }
        Ok(armor)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EquipmentSpec {
    pub slot: EquipmentSlot,
    pub armor: Option<ArmorSpec>,
    pub attack: Option<crate::creature_authoring::Melee>,
    #[serde(default)]
    pub modifiers: Vec<crate::creature_authoring::Modifier>,
}
impl EquipmentSpec {
    pub(crate) fn compile(
        &self,
        rules: &tor_simulation::rules::Ruleset,
    ) -> Result<tor_simulation::EquipmentSpec, crate::Failure> {
        let modifiers = self
            .modifiers
            .iter()
            .map(crate::creature_authoring::Modifier::compile)
            .collect::<Result<Vec<_>, _>>()?;
        rules
            .validate_modifiers(&modifiers)
            .map_err(|error| super::fail(format!("Invalid equipment modifier: {error:?}")))?;
        let slot = tor_simulation::EquipmentSlot::new(&self.slot)
            .map_err(|_| super::fail(format!("Invalid equipment slot ID {:?}", self.slot)))?;
        if rules.equipment_slot(&slot).is_none() {
            return Err(super::fail(format!(
                "Unknown equipment slot {:?}",
                self.slot
            )));
        }
        Ok(tor_simulation::EquipmentSpec {
            armor: self.armor.map(ArmorSpec::compile).transpose()?,
            slot,
            attack: self
                .attack
                .as_ref()
                .map(|attack| attack.compile(rules))
                .transpose()?,
            modifiers,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiceExpr {
    pub count: u16,
    pub sides: u16,
    pub bonus: i32,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AmountExpr {
    Fixed { amount: u32 },
    Rolled { dice: DiceExpr },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DamageComponentExpr {
    pub category: DamageType,
    pub descriptor: Option<String>,
    pub amount: AmountExpr,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum DamageExpr {
    SelectedWeapon {
        impact_bonus: u32,
    },
    Components {
        #[serde(
            deserialize_with = "crate::creature_authoring::bounded_strike_vec::<_, _, { tor_simulation::limits::DAMAGE_COMPONENTS }>"
        )]
        components: Vec<DamageComponentExpr>,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TheftSelection {
    Currency {
        currency: CurrencyKind,
        amount: super::PopulationQuantityExpr,
    },
    MagicalUnit,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelocationPolicy {
    SameFloor,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RelocationGate {
    pub flag: String,
    pub policy: RelocationPolicy,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EquipmentAlteration {
    EnchantArmor,
    EnchantWeapon,
    ProtectArmor,
    RemoveCurse,
}
impl From<EquipmentAlteration> for tor_simulation::EquipmentAlteration {
    fn from(value: EquipmentAlteration) -> Self {
        match value {
            EquipmentAlteration::EnchantArmor => Self::EnchantArmor,
            EquipmentAlteration::EnchantWeapon => Self::EnchantWeapon,
            EquipmentAlteration::ProtectArmor => Self::ProtectArmor,
            EquipmentAlteration::RemoveCurse => Self::RemoveCurse,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum EquippedSelection {
    Slot {
        slot: String,
        class: ItemClass,
        last: bool,
    },
    All {},
}

/// Healing overflow is explicitly authored and never inferred from potion identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HealingOverflow {
    None,
    ImproveEarliestHitDie,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Effect {
    Theft {
        selection: TheftSelection,
        relocation: Option<RelocationGate>,
    },
    AlterEquipment {
        selection: EquippedSelection,
        alteration: EquipmentAlteration,
    },
    CorrodeArmor {
        slot: String,
        amount: u32,
    },
    ApplyLinkedHold {
        policy: HoldPolicy,
    },
    HoldDamage {
        category: DamageType,
        descriptor: Option<String>,
    },
    DrainLatestHitDie,
    RestoreHitDie,
    StepSelfOneCell,
    PushTargetOneCell {
        direction: PushDirection,
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
    LoseAttribute {
        attribute: String,
        amount: AmountExpr,
    },
    RestoreAttribute {
        attribute: String,
        amount: AmountExpr,
    },
    LoseMaximumHealth {
        amount: AmountExpr,
    },
    RestoreMaximumHealth {
        amount: AmountExpr,
    },
    Nourish {
        amount: AmountExpr,
    },
    ApplyCondition {
        condition: String,
        duration: u64,
    },
    ApplyConditionForTurns {
        condition: String,
        duration: AmountExpr,
    },
    RemoveCondition {
        condition: String,
    },
    Restore {
        resource: String,
        amount: AmountExpr,
    },
    SearchAdjacentGuises,
    ObservePublicPlan,
    ApplyQualifiedConditionToSource {
        condition: String,
        duration: u64,
        ability_tag: String,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HoldPolicy {
    pub sustaining: String,
    pub preparation: u64,
    pub recovery: u64,
    pub protection: u64,
}
impl<'de> Deserialize<'de> for HoldPolicy {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            sustaining: String,
            preparation: u64,
            recovery: u64,
            protection: u64,
        }
        let f: Fields = crate::creature_authoring::named_map(d)?;
        if tor_simulation::content_ids::FlagId::new(&f.sustaining).is_err()
            || ![f.preparation, f.recovery, f.protection]
                .into_iter()
                .all(|value| (1..=tor_simulation::limits::NUMERIC as u64).contains(&value))
        {
            return Err(serde::de::Error::custom("Invalid hold policy"));
        }
        Ok(Self {
            sustaining: f.sustaining,
            preparation: f.preparation,
            recovery: f.recovery,
            protection: f.protection,
        })
    }
}
/// Discovery is an explicit content policy, never inferred from the item name.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemDiscovery {
    #[default]
    SelfEffect,
    OnUse,
    Never,
}
impl From<ItemDiscovery> for tor_simulation::ItemDiscovery {
    fn from(value: ItemDiscovery) -> Self {
        match value {
            ItemDiscovery::SelfEffect => Self::SelfEffect,
            ItemDiscovery::OnUse => Self::OnUse,
            ItemDiscovery::Never => Self::Never,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ConsumableSpec {
    pub effects: Vec<Effect>,
    pub discovery: ItemDiscovery,
}
impl<'de> Deserialize<'de> for ConsumableSpec {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            #[serde(
                deserialize_with = "crate::creature_authoring::bounded_strike_vec::<_, _, { tor_simulation::limits::CONSUMABLE_EFFECTS }>"
            )]
            effects: Vec<Effect>,
            #[serde(default)]
            discovery: ItemDiscovery,
        }
        let fields: Fields = crate::creature_authoring::named_map(deserializer)?;
        Ok(Self {
            effects: fields.effects,
            discovery: fields.discovery,
        })
    }
}
fn compile_item_effects(
    effects: &[Effect],
    rules: &tor_simulation::rules::Ruleset,
) -> Result<Vec<tor_simulation::rules::Effect>, crate::Failure> {
    let value = serde_json::to_value(effects).map_err(|error| super::fail(error.to_string()))?;
    let effects: Vec<tor_simulation::rules::Effect> = serde_json::from_value(value)
        .map_err(|error| super::fail(format!("Invalid item effect: {error}")))?;
    rules
        .validate_effects(&effects)
        .map_err(|error| super::fail(format!("Invalid item effect: {error:?}")))?;
    Ok(effects)
}
impl ConsumableSpec {
    pub(crate) fn compile(
        &self,
        rules: &tor_simulation::rules::Ruleset,
    ) -> Result<tor_simulation::ConsumableSpec, crate::Failure> {
        Ok(tor_simulation::ConsumableSpec {
            effects: compile_item_effects(&self.effects, rules)?,
            discovery: self.discovery.into(),
        })
    }
}

/// Reading either applies bounded immediate effects or requests one class of
/// identity selection. Physical class does not grant this capability.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReadableSpec {
    pub effects: Vec<Effect>,
    pub identify: Option<ItemClass>,
    pub discovery: ItemDiscovery,
}
impl<'de> Deserialize<'de> for ReadableSpec {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            #[serde(
                default,
                deserialize_with = "crate::creature_authoring::bounded_strike_vec::<_, _, { tor_simulation::limits::CONSUMABLE_EFFECTS }>"
            )]
            effects: Vec<Effect>,
            identify: Option<ItemClass>,
            #[serde(default)]
            discovery: ItemDiscovery,
        }
        let fields: Fields = crate::creature_authoring::named_map(deserializer)?;
        Ok(Self {
            effects: fields.effects,
            identify: fields.identify,
            discovery: fields.discovery,
        })
    }
}
impl ReadableSpec {
    pub(crate) fn compile(
        &self,
        rules: &tor_simulation::rules::Ruleset,
    ) -> Result<tor_simulation::ReadableSpec, crate::Failure> {
        let spec = tor_simulation::ReadableSpec {
            effects: compile_item_effects(&self.effects, rules)?,
            identify: self.identify.map(Into::into),
            discovery: self.discovery.into(),
        };
        super::require(spec.valid(), "Invalid readable definition")?;
        Ok(spec)
    }
}

/// Charge ranges are distinct from physical stack quantities and admit empty
/// devices for authored fixtures. Ordinary Rogue devices use three to seven.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ChargeRange {
    pub minimum: u16,
    pub maximum: u16,
}
impl<'de> Deserialize<'de> for ChargeRange {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            minimum: u16,
            maximum: u16,
        }
        let fields: Fields = crate::creature_authoring::named_map(d)?;
        if fields.minimum > fields.maximum {
            return Err(serde::de::Error::custom("Invalid charge range"));
        }
        Ok(Self {
            minimum: fields.minimum,
            maximum: fields.maximum,
        })
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DeviceSpec {
    pub ability: String,
    pub initial_charges: ChargeRange,
    pub discovery: ItemDiscovery,
}
impl<'de> Deserialize<'de> for DeviceSpec {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            ability: String,
            initial_charges: ChargeRange,
            #[serde(default)]
            discovery: ItemDiscovery,
        }
        let fields: Fields = crate::creature_authoring::named_map(d)?;
        Ok(Self {
            ability: fields.ability,
            initial_charges: fields.initial_charges,
            discovery: fields.discovery,
        })
    }
}
impl DeviceSpec {
    pub(crate) fn compile(
        &self,
        rules: &tor_simulation::rules::Ruleset,
    ) -> Result<tor_simulation::DeviceSpec, crate::Failure> {
        let ability = tor_simulation::content_ids::AbilityId::new(&self.ability)
            .map_err(|_| super::fail(format!("Invalid device ability {:?}", self.ability)))?;
        let spec = tor_simulation::DeviceSpec {
            ability,
            initial_charges: tor_simulation::ChargeRange {
                minimum: self.initial_charges.minimum,
                maximum: self.initial_charges.maximum,
            },
            discovery: self.discovery.into(),
        };
        spec.validate_rules(rules)
            .map_err(|error| super::fail(format!("Invalid device definition: {error:?}")))?;
        Ok(spec)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BodySpec {
    pub cells: Vec<[i32; 3]>,
    pub eye: [i32; 3],
    pub mass: u32,
}

impl From<BodySpec> for tor_simulation::BodySpec {
    fn from(value: BodySpec) -> Self {
        Self {
            cells: value.cells,
            eye: value.eye,
            mass: value.mass,
        }
    }
}

/// Authored task overrides. Omission inherits the species' explicit defaults.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AiProfile {
    pub idle: Option<crate::creature_authoring::IdleBehavior>,
    pub doors: Option<bool>,
    pub care: Option<bool>,
    pub equipment: Option<bool>,
    pub loot: Option<bool>,
    pub gold: Option<bool>,
    pub flee_percent: Option<u32>,
}
impl<'de> Deserialize<'de> for AiProfile {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize, Default)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            #[serde(default)]
            idle: Option<crate::creature_authoring::IdleBehavior>,
            #[serde(default)]
            doors: Option<bool>,
            #[serde(default)]
            care: Option<bool>,
            #[serde(default)]
            equipment: Option<bool>,
            #[serde(default)]
            loot: Option<bool>,
            #[serde(default)]
            gold: Option<bool>,
            #[serde(default)]
            flee_percent: Option<u32>,
        }
        let fields: Fields = crate::creature_authoring::named_map(d)?;
        Ok(Self {
            idle: fields.idle,
            doors: fields.doors,
            care: fields.care,
            equipment: fields.equipment,
            loot: fields.loot,
            gold: fields.gold,
            flee_percent: fields.flee_percent,
        })
    }
}
impl AiProfile {
    pub fn general() -> Self {
        Self {
            idle: None,
            doors: Some(true),
            care: Some(true),
            equipment: Some(true),
            loot: Some(true),
            gold: None,
            flee_percent: Some(25),
        }
    }
    pub fn valid(&self) -> bool {
        self.flee_percent.is_none_or(|percent| percent <= 100)
    }
    pub fn resolve(
        &self,
        species: tor_simulation::behavior::BehaviorPolicy,
    ) -> tor_simulation::ai::AiProfile {
        let idle = self
            .idle
            .map(|idle| match idle {
                crate::creature_authoring::IdleBehavior::Wait => {
                    tor_simulation::behavior::IdleBehavior::Wait
                }
                crate::creature_authoring::IdleBehavior::Explore => {
                    tor_simulation::behavior::IdleBehavior::Explore
                }
            })
            .unwrap_or(species.idle);
        tor_simulation::ai::AiProfile {
            idle,
            doors: self.doors.unwrap_or(species.doors),
            care: self.care.unwrap_or(species.care),
            equipment: self.equipment.unwrap_or(species.equipment),
            loot: self.loot.unwrap_or(species.loot),
            gold: self.gold.unwrap_or(species.gold),
            flee_percent: self.flee_percent.unwrap_or(species.flee_percent),
        }
    }
}

impl<'de> Deserialize<'de> for Effect {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            Theft {
                selection: TheftSelection,
                relocation: Option<RelocationGate>,
            },
            AlterEquipment {
                selection: EquippedSelection,
                alteration: EquipmentAlteration,
            },
            CorrodeArmor {
                slot: String,
                amount: u32,
            },
            ApplyLinkedHold {
                policy: HoldPolicy,
            },
            HoldDamage {
                category: DamageType,
                // TOML has no null value. Authoring may omit an unmarked
                // descriptor; the independent stored schema requires null.
                descriptor: Option<String>,
            },
            DrainLatestHitDie {},
            RestoreHitDie {},
            StepSelfOneCell {},
            PushTargetOneCell {
                direction: PushDirection,
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
            LoseAttribute {
                attribute: String,
                amount: AmountExpr,
            },
            RestoreAttribute {
                attribute: String,
                amount: AmountExpr,
            },
            LoseMaximumHealth {
                amount: AmountExpr,
            },
            RestoreMaximumHealth {
                amount: AmountExpr,
            },
            Nourish {
                amount: AmountExpr,
            },
            ApplyCondition {
                condition: String,
                duration: u64,
            },
            ApplyConditionForTurns {
                condition: String,
                duration: AmountExpr,
            },
            RemoveCondition {
                condition: String,
            },
            Restore {
                resource: String,
                amount: AmountExpr,
            },
            SearchAdjacentGuises {},
            ObservePublicPlan {},
            ApplyQualifiedConditionToSource {
                condition: String,
                duration: u64,
                ability_tag: String,
            },
        }
        struct Named;
        impl<'de> serde::de::Visitor<'de> for Named {
            type Value = Fields;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a named effect object")
            }
            fn visit_map<M: serde::de::MapAccess<'de>>(self, map: M) -> Result<Fields, M::Error> {
                Fields::deserialize(serde::de::value::MapAccessDeserializer::new(map))
            }
        }
        Ok(match deserializer.deserialize_map(Named)? {
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
            Fields::LoseAttribute { attribute, amount } => {
                Self::LoseAttribute { attribute, amount }
            }
            Fields::RestoreAttribute { attribute, amount } => {
                Self::RestoreAttribute { attribute, amount }
            }
            Fields::LoseMaximumHealth { amount } => Self::LoseMaximumHealth { amount },
            Fields::RestoreMaximumHealth { amount } => Self::RestoreMaximumHealth { amount },
            Fields::Nourish { amount } => Self::Nourish { amount },
            Fields::ApplyCondition {
                condition,
                duration,
            } => Self::ApplyCondition {
                condition,
                duration,
            },
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

#[cfg(test)]
mod linked_hold_authoring_tests {
    use super::*;
    #[test]
    fn linked_hold_effects_require_named_bounded_policy() {
        let linked = serde_json::json!({"kind":"apply_linked_hold","policy":{
            "sustaining":"clutch","preparation":100,"recovery":100,"protection":400}});
        let damage =
            serde_json::json!({"kind":"hold_damage","category":"impact","descriptor":null});
        for value in [linked.clone(), damage.clone()] {
            let decoded: Effect = serde_json::from_value(value.clone()).unwrap();
            assert_eq!(serde_json::to_value(decoded).unwrap(), value);
        }
        for field in ["sustaining", "preparation", "recovery", "protection"] {
            let mut missing = linked.clone();
            missing["policy"].as_object_mut().unwrap().remove(field);
            assert!(serde_json::from_value::<Effect>(missing).is_err());
        }
        for invalid in [
            serde_json::json!(0),
            serde_json::json!(1_000_001),
            serde_json::json!("100"),
        ] {
            let mut value = linked.clone();
            value["policy"]["preparation"] = invalid;
            assert!(serde_json::from_value::<Effect>(value).is_err());
        }
        let mut positional = linked.clone();
        positional["policy"] = serde_json::json!(["clutch", 100, 100, 400]);
        assert!(serde_json::from_value::<Effect>(positional).is_err());
        let mut missing = damage.clone();
        missing.as_object_mut().unwrap().remove("descriptor");
        let unmarked: Effect = serde_json::from_value(missing).unwrap();
        assert_eq!(serde_json::to_value(unmarked).unwrap(), damage);
        let mut forged = linked;
        forged["holder"] = 1.into();
        assert!(serde_json::from_value::<Effect>(forged).is_err());
    }

    #[test]
    fn hold_damage_toml_accepts_omitted_descriptor_and_round_trips_named_descriptors() {
        for descriptor in [None, Some("fire")] {
            let mut source = "kind = 'hold_damage'\ncategory = 'impact'\n".to_owned();
            if let Some(descriptor) = descriptor {
                source.push_str(&format!("descriptor = '{descriptor}'\n"));
            }
            let authored: Effect = toml::from_str(&source).unwrap();
            let canonical = serde_json::json!({
                "kind":"hold_damage", "category":"impact", "descriptor":descriptor
            });
            assert_eq!(serde_json::to_value(&authored).unwrap(), canonical);
            let restored: Effect = toml::from_str(&toml::to_string(&authored).unwrap()).unwrap();
            assert_eq!(serde_json::to_value(restored).unwrap(), canonical);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules() -> std::sync::Arc<tor_simulation::rules::Ruleset> {
        tor_simulation::rules::Ruleset::validate(
            toml::from_str(tor_simulation::rules::DEFAULT_RULESET_TOML).unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn equipment_slot_references_compile_from_data_and_reject_unknown_ids() {
        let mut document = rules().document().clone();
        document
            .equipment_slots
            .push(tor_simulation::rules::EquipmentSlotDef {
                id: tor_simulation::EquipmentSlot::new("tentacle").unwrap(),
                name: "Tentacle".into(),
                equipped_phrase: "held by a tentacle".into(),
                compatible_classes: vec![tor_simulation::ItemClass::Tool],
                requires_attack: false,
                equip_turns: 2,
                unequip_turns: 4,
            });
        let rules = tor_simulation::rules::Ruleset::validate(document).unwrap();
        let anatomy = AnatomySpec {
            slots: vec!["tentacle".into(), "tentacle".into()],
        };
        let compiled = anatomy.compile(&rules).unwrap();
        assert_eq!(compiled.slots[0].as_str(), "tentacle");
        let equipment = EquipmentSpec {
            armor: None,
            slot: "tentacle".into(),
            attack: None,
            modifiers: vec![],
        };
        assert!(equipment
            .compile(&rules)
            .unwrap()
            .validate_rules(tor_simulation::ItemClass::Tool, &rules)
            .is_ok());
        for slot in ["missing_slot", "invalid slot"] {
            assert!(AnatomySpec {
                slots: vec![slot.into()]
            }
            .compile(&rules)
            .is_err());
            assert!(EquipmentSpec {
                slot: slot.into(),
                ..equipment.clone()
            }
            .compile(&rules)
            .is_err());
        }
    }

    #[test]
    fn generic_item_effects_compile_and_reject_unknown_content_references() {
        let valid = ConsumableSpec {
            discovery: ItemDiscovery::SelfEffect,
            effects: vec![Effect::Heal {
                amount: AmountExpr::Fixed { amount: 5 },
            }],
        };
        assert!(valid.compile(&rules()).is_ok());
        let missing = ConsumableSpec {
            discovery: ItemDiscovery::SelfEffect,
            effects: vec![Effect::ApplyCondition {
                condition: "missing_condition".into(),
                duration: 100,
            }],
        };
        assert!(missing.compile(&rules()).is_err());
        let missing_descriptor = ConsumableSpec {
            discovery: ItemDiscovery::SelfEffect,
            effects: vec![Effect::Damage {
                damage: DamageExpr::Components {
                    components: vec![DamageComponentExpr {
                        category: DamageType::Energy,
                        descriptor: Some("missing_descriptor".into()),
                        amount: AmountExpr::Fixed { amount: 5 },
                    }],
                },
            }],
        };
        assert!(missing_descriptor.compile(&rules()).is_err());
    }

    #[test]
    fn item_condition_and_hit_die_healing_effects_preserve_owned_source_shapes() {
        for value in [
            serde_json::json!({"kind":"apply_condition_for_turns","condition":"braced","duration":{"kind":"rolled","dice":{"count":1,"sides":5,"bonus":3}}}),
            serde_json::json!({"kind":"remove_condition","condition":"braced"}),
            serde_json::json!({"kind":"heal_per_hit_die","sides":4,"bonus":0,"overflow":"none"}),
        ] {
            let effect: Effect = serde_json::from_value(value.clone()).unwrap();
            assert_eq!(serde_json::to_value(&effect).unwrap(), value);
            let compiled = ConsumableSpec {
                effects: vec![effect],
                discovery: ItemDiscovery::OnUse,
            }
            .compile(&rules())
            .unwrap();
            assert_eq!(serde_json::to_value(&compiled.effects[0]).unwrap(), value);
            let mut extra = value.clone();
            extra["private_counter"] = 1.into();
            assert!(serde_json::from_value::<Effect>(extra).is_err());
            let mut missing = value;
            let key = if missing["kind"] == "heal_per_hit_die" {
                "sides"
            } else {
                "condition"
            };
            missing.as_object_mut().unwrap().remove(key);
            assert!(serde_json::from_value::<Effect>(missing).is_err());
        }
        for value in [
            serde_json::json!({"kind":"apply_condition_for_turns","condition":"missing","duration":{"kind":"fixed","amount":4}}),
            serde_json::json!({"kind":"remove_condition","condition":"missing"}),
            serde_json::json!({"kind":"heal_per_hit_die","sides":0,"bonus":0,"overflow":"none"}),
        ] {
            let effect: Effect = serde_json::from_value(value).unwrap();
            assert!(ConsumableSpec {
                effects: vec![effect],
                discovery: ItemDiscovery::OnUse
            }
            .compile(&rules())
            .is_err());
        }
    }

    #[test]
    fn hit_die_healing_requires_an_explicit_supported_overflow_policy() {
        for (name, policy) in [
            ("none", HealingOverflow::None),
            (
                "improve_earliest_hit_die",
                HealingOverflow::ImproveEarliestHitDie,
            ),
        ] {
            let value = serde_json::json!({
                "kind":"heal_per_hit_die", "sides":4, "bonus":0, "overflow":name
            });
            let effect: Effect = serde_json::from_value(value.clone()).unwrap();
            assert_eq!(
                effect,
                Effect::HealPerHitDie {
                    sides: 4,
                    bonus: 0,
                    overflow: policy
                }
            );
            let compiled = ConsumableSpec {
                effects: vec![effect],
                discovery: ItemDiscovery::OnUse,
            }
            .compile(&rules())
            .unwrap();
            assert_eq!(serde_json::to_value(&compiled.effects[0]).unwrap(), value);
        }
        for value in [
            serde_json::json!({"kind":"heal_per_hit_die","sides":4,"bonus":0}),
            serde_json::json!({"kind":"heal_per_hit_die","sides":4,"bonus":0,"overflow":null}),
            serde_json::json!({"kind":"heal_per_hit_die","sides":4,"bonus":0,"overflow":"improve_latest_hit_die"}),
            serde_json::json!({"kind":"heal_per_hit_die","sides":4,"bonus":0,"overflow":{"kind":"none"}}),
            serde_json::json!(["heal_per_hit_die", 4, 0, "none"]),
        ] {
            assert!(serde_json::from_value::<Effect>(value).is_err());
        }
    }

    #[test]
    fn readable_authoring_compiles_exactly_one_effect_or_identify_capability() {
        let effects: ReadableSpec = toml::from_str(
            r#"effects = [{ kind = "heal", amount = { kind = "fixed", amount = 5 } }]
               discovery = "on_use""#,
        )
        .unwrap();
        assert!(effects.compile(&rules()).is_ok());
        let identify: ReadableSpec = toml::from_str(
            r#"identify = "potion"
               discovery = "on_use""#,
        )
        .unwrap();
        let compiled = identify.compile(&rules()).unwrap();
        assert_eq!(compiled.identify, Some(tor_simulation::ItemClass::Potion));
        assert_eq!(compiled.discovery, tor_simulation::ItemDiscovery::OnUse);
        let mut invalid = identify.clone();
        invalid.effects = effects.effects;
        assert!(invalid.compile(&rules()).is_err());
        invalid.effects.clear();
        invalid.identify = None;
        assert!(invalid.compile(&rules()).is_err());
        invalid.effects.push(Effect::ApplyCondition {
            condition: "missing".into(),
            duration: 100,
        });
        assert!(invalid.compile(&rules()).is_err());
        for value in [
            serde_json::json!([[], "potion", "on_use"]),
            serde_json::json!({"identify":"potion", "unknown":true}),
            serde_json::json!({"identify":"secret_class"}),
            serde_json::json!({"identify":"potion", "discovery":"infer_name"}),
        ] {
            assert!(serde_json::from_value::<ReadableSpec>(value).is_err());
        }
    }

    #[test]
    fn device_sources_own_named_bounded_charge_ranges_and_reject_non_ray_abilities() {
        for (minimum, maximum) in [(0, 0), (3, 7), (0, u16::MAX)] {
            let range: ChargeRange = serde_json::from_value(serde_json::json!({
                "minimum":minimum, "maximum":maximum
            }))
            .unwrap();
            assert_eq!((range.minimum, range.maximum), (minimum, maximum));
        }
        for value in [
            serde_json::json!([3, 7]),
            serde_json::json!({"minimum":7,"maximum":3}),
            serde_json::json!({"minimum":3}),
            serde_json::json!({"minimum":0,"maximum":65536}),
            serde_json::json!({"minimum":3,"maximum":7,"seed":2}),
        ] {
            assert!(serde_json::from_value::<ChargeRange>(value).is_err());
        }
        for ability in ["missing", "basic_melee"] {
            let device = DeviceSpec {
                ability: ability.into(),
                initial_charges: ChargeRange {
                    minimum: 3,
                    maximum: 7,
                },
                discovery: ItemDiscovery::OnUse,
            };
            assert!(device.compile(&rules()).is_err());
        }
        for value in [
            serde_json::json!(["basic_melee", {"minimum":3,"maximum":7}, "on_use"]),
            serde_json::json!({"ability":"basic_melee","discovery":"on_use"}),
            serde_json::json!({"ability":"basic_melee","initial_charges":{"minimum":3,"maximum":7},"charges":4}),
        ] {
            assert!(serde_json::from_value::<DeviceSpec>(value).is_err());
        }
    }

    #[test]
    fn device_source_compilation_requires_cost_free_directional_ray_authority() {
        use tor_simulation::rules::{AbilityDef, Ruleset};
        let ability: AbilityDef = toml::from_str(r#"
            id = "fixture.device_ray"
            name = "Fixture Ray"
            interaction = "hostile_to_target"
            tags = ["attack"]
            basic_attack = false
            invocation = "directional_ray"
            timing = { source = { kind = "fixed", prepare = 100, recover = 100 } }
            reach = { kind = "sight", range = 6 }
            strikes = { kind = "authored", strikes = [{ offset = 0, check = { actor = "target", skill = { kind = "named", skill = "agility" }, against = { kind = "opposed", skill = { kind = "named", skill = "agility" } } }, on_success = [], on_failure = [{ kind = "damage", damage = { kind = "components", components = [{ category = "energy", amount = { kind = "fixed", amount = 5 } }] } }], descriptors = [], riders = [] }] }
            ai = { priority = 0, hostile = true, minimum_resource_percent = 0, range_preference = "any" }
        "#).unwrap();
        let source = DeviceSpec {
            ability: "fixture.device_ray".into(),
            initial_charges: ChargeRange {
                minimum: 3,
                maximum: 7,
            },
            discovery: ItemDiscovery::OnUse,
        };
        let mut document = rules().document().clone();
        document.abilities.push(ability.clone());
        let valid = Ruleset::validate(document).unwrap();
        let compiled = source.compile(&valid).unwrap();
        assert_eq!(compiled.ability.as_str(), "fixture.device_ray");
        assert_eq!(compiled.initial_charges.minimum, 3);
        assert_eq!(compiled.initial_charges.maximum, 7);
        for (field, operand) in [
            (
                "cost",
                serde_json::json!({"resource":"mana","start":1,"resolution":0}),
            ),
            (
                "supply_costs",
                serde_json::json!([{"supply":"bandage","quantity":1,"access":"carried_only","spend_at":"first_resolution"}]),
            ),
            ("cooldown", serde_json::json!(100)),
        ] {
            let mut changed = serde_json::to_value(&ability).unwrap();
            changed[field] = operand;
            let mut document = rules().document().clone();
            document
                .abilities
                .push(serde_json::from_value(changed).unwrap());
            let rules = Ruleset::validate(document).unwrap();
            assert!(
                source.compile(&rules).is_err(),
                "device must reject {field}"
            );
        }
        let mut hidden = source;
        hidden.discovery = ItemDiscovery::SelfEffect;
        assert!(hidden.compile(&valid).is_err());
    }

    #[test]
    fn authored_consumable_discovery_omission_preserves_self_effect_policy() {
        let spec: ConsumableSpec = toml::from_str(
            r#"effects = [{ kind = "heal", amount = { kind = "fixed", amount = 5 } }]"#,
        )
        .unwrap();
        assert_eq!(spec.discovery, ItemDiscovery::SelfEffect);
        assert_eq!(
            spec.compile(&rules()).unwrap().discovery,
            tor_simulation::ItemDiscovery::SelfEffect
        );
        assert_eq!(
            serde_json::to_value(spec).unwrap()["discovery"],
            "self_effect"
        );
    }

    #[test]
    fn restorable_effect_sources_compile_exact_operands_and_reject_invalid_shapes() {
        for kind in [
            "lose_attribute",
            "restore_attribute",
            "lose_maximum_health",
            "restore_maximum_health",
        ] {
            for amount in [
                serde_json::json!({"kind":"fixed","amount":2}),
                // Requests retain the effect bound; only actual attribute loss is capped at 1000.
                serde_json::json!({"kind":"fixed","amount":1_000_000}),
                serde_json::json!({"kind":"rolled","dice":{"count":1,"sides":3,"bonus":0}}),
            ] {
                let mut value = serde_json::json!({"kind":kind,"amount":amount});
                if kind.ends_with("attribute") {
                    value["attribute"] = "strength".into();
                }
                let authored: Effect =
                    serde_json::from_value(value.clone()).expect("author effect");
                let compiled = ConsumableSpec {
                    discovery: ItemDiscovery::SelfEffect,
                    effects: vec![authored],
                }
                .compile(&rules())
                .unwrap();
                assert_eq!(serde_json::to_value(&compiled.effects[0]).unwrap(), value);
                let mut missing = value.clone();
                missing.as_object_mut().unwrap().remove("amount");
                assert!(serde_json::from_value::<Effect>(missing).is_err());
                let mut unknown = value.clone();
                unknown["private_source"] = 7.into();
                assert!(serde_json::from_value::<Effect>(unknown).is_err());
                value["amount"] = serde_json::json!({"kind":"fixed","amount":1_000_001});
                let invalid: Effect = serde_json::from_value(value).unwrap();
                assert!(ConsumableSpec {
                    discovery: ItemDiscovery::SelfEffect,
                    effects: vec![invalid]
                }
                .compile(&rules())
                .is_err());
            }
        }
        let invalid: Effect = serde_json::from_value(serde_json::json!({
            "kind":"lose_attribute","attribute":"health","amount":{"kind":"fixed","amount":1}
        }))
        .unwrap();
        assert!(ConsumableSpec {
            discovery: ItemDiscovery::SelfEffect,
            effects: vec![invalid]
        }
        .compile(&rules())
        .is_err());
    }

    #[test]
    fn old_item_effect_and_equipment_shapes_are_rejected() {
        assert!(serde_json::from_value::<ConsumableSpec>(serde_json::json!({
            "effects": [{"type":"heal", "amount":5}]
        }))
        .is_err());
        assert!(serde_json::from_value::<EquipmentSpec>(serde_json::json!({
            "slot":"body_armor", "attack":null, "defense":1, "reductions":{}
        }))
        .is_err());
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PushDirection {
    AwayFromSource,
}

#[cfg(test)]
mod equipment_generation_tests {
    use super::EquipmentGeneration;

    #[test]
    fn authored_birth_defaults_are_source_only_and_generated_bounds_are_checked() {
        let policy: EquipmentGeneration =
            serde_json::from_value(serde_json::json!({"kind":"authored"})).unwrap();
        assert_eq!(
            policy.compile().unwrap(),
            tor_simulation::EquipmentGeneration::Authored {
                hit_enchantment: 0,
                damage_enchantment: 0
            }
        );
        for value in [
            serde_json::json!({"kind":"weapon","curse_percent":99,"enchant_percent":2,"minimum":1,"maximum":3}),
            serde_json::json!({"kind":"armor","curse_percent":20,"enchant_percent":8,"minimum":0,"maximum":3}),
            serde_json::json!({"kind":"armor","curse_percent":20,"enchant_percent":8,"minimum":4,"maximum":3}),
            serde_json::json!({"kind":"armor","curse_percent":20,"enchant_percent":8,"minimum":1,"maximum":1000001}),
            serde_json::json!({"kind":"authored","hit_enchantment":1000001}),
        ] {
            let policy: EquipmentGeneration = serde_json::from_value(value).unwrap();
            assert!(policy.compile().is_err());
        }
        assert!(serde_json::from_value::<EquipmentGeneration>(
            serde_json::json!({"kind":"authored","curse_percent":1})
        )
        .is_err());
    }
}
