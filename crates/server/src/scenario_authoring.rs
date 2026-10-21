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
    Cursed,
    SignedPotency {
        negative: u32,
        positive: u32,
    },
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
            Self::Cursed => tor_simulation::EquipmentGeneration::Cursed,
            Self::SignedPotency { negative, positive } => {
                tor_simulation::EquipmentGeneration::SignedPotency { negative, positive }
            }
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
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct AnatomySpec {
    pub slots: Vec<EquipmentSlot>,
    pub capacities: std::collections::BTreeMap<String, u16>,
    pub primary_attack_socket: PrimaryAttackSocket,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PrimaryAttackSocket {
    None {},
    Socket { slot: u16 },
}
impl<'de> Deserialize<'de> for AnatomySpec {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            slots: Vec<EquipmentSlot>,
            #[serde(deserialize_with = "anatomy_capacities")]
            capacities: std::collections::BTreeMap<String, u16>,
            primary_attack_socket: PrimaryAttackSocket,
        }
        let f: Fields = crate::creature_authoring::named_map(d)?;
        if f.slots.len() > tor_simulation::limits::EQUIPMENT_SOCKETS
            || matches!(f.primary_attack_socket, PrimaryAttackSocket::Socket { slot } if usize::from(slot) >= f.slots.len())
        {
            return Err(serde::de::Error::custom("invalid anatomy sockets"));
        }
        Ok(Self {
            slots: f.slots,
            capacities: f.capacities,
            primary_attack_socket: f.primary_attack_socket,
        })
    }
}
fn anatomy_capacities<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<std::collections::BTreeMap<String, u16>, D::Error> {
    struct Capacities;
    impl<'de> serde::de::Visitor<'de> for Capacities {
        type Value = std::collections::BTreeMap<String, u16>;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("bounded unique anatomy capacities")
        }
        fn visit_map<A: serde::de::MapAccess<'de>>(
            self,
            mut map: A,
        ) -> Result<Self::Value, A::Error> {
            let mut values = std::collections::BTreeMap::new();
            while let Some((id, units)) = map.next_entry::<String, u16>()? {
                if values.len() >= tor_simulation::limits::EQUIPMENT_SOCKETS
                    || usize::from(units) > tor_simulation::limits::EQUIPMENT_SOCKETS
                    || values.insert(id, units).is_some()
                {
                    return Err(serde::de::Error::custom("invalid anatomy capacities"));
                }
            }
            Ok(values)
        }
    }
    d.deserialize_map(Capacities)
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
        let capacities = self
            .capacities
            .iter()
            .map(|(id, units)| {
                tor_simulation::content_ids::EquipmentCapacityId::new(id)
                    .map(|id| (id, *units))
                    .map_err(|_| super::fail(format!("Invalid equipment capacity ID {id:?}")))
            })
            .collect::<Result<std::collections::BTreeMap<_, _>, _>>()?;
        let capacities = tor_simulation::equipment_capacity::EquipmentCapacities::new(capacities)
            .ok_or_else(|| super::fail("Invalid anatomy capacities"))?;
        let primary_attack_socket = match self.primary_attack_socket {
            PrimaryAttackSocket::None {} => {
                tor_simulation::equipment_capacity::PrimaryAttackSocket::None
            }
            PrimaryAttackSocket::Socket { slot } => {
                tor_simulation::equipment_capacity::PrimaryAttackSocket::Socket {
                    slot: tor_simulation::EquipmentSlotId(slot),
                }
            }
        };
        let anatomy = tor_simulation::AnatomySpec {
            slots,
            capacities,
            primary_attack_socket,
        };
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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WeaponRangeBands {
    pub short_feet: u32,
    pub long_feet: u32,
}
impl From<WeaponRangeBands> for tor_simulation::weapon_profiles::RangeBands {
    fn from(value: WeaponRangeBands) -> Self {
        Self {
            short_feet: value.short_feet,
            long_feet: value.long_feet,
        }
    }
}
/// Source-owned launcher identity and physical range, independent of runtime serde.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LauncherProfile {
    pub family: String,
    pub range: WeaponRangeBands,
}
impl LauncherProfile {
    fn compile(&self) -> Result<tor_simulation::weapon_profiles::LauncherProfile, crate::Failure> {
        Ok(tor_simulation::weapon_profiles::LauncherProfile {
            family: tor_simulation::content_ids::LauncherFamilyId::new(&self.family)
                .map_err(|_| super::fail("Invalid launcher family ID"))?,
            range: self.range.into(),
        })
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HitRecovery {
    ConsumedOnHit,
    Recoverable,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectileSpec {
    pub family: String,
    pub damage: crate::creature_authoring::Damage,
    pub recovery: HitRecovery,
}
impl ProjectileSpec {
    pub(crate) fn compile(
        &self,
        rules: &tor_simulation::rules::Ruleset,
    ) -> Result<tor_simulation::projectile_profiles::ProjectileSpec, crate::Failure> {
        let result = tor_simulation::projectile_profiles::ProjectileSpec {
            family: tor_simulation::content_ids::LauncherFamilyId::new(&self.family)
                .map_err(|_| super::fail("Invalid projectile family ID"))?,
            damage: self.damage.compile(rules)?,
            recovery: match self.recovery {
                HitRecovery::ConsumedOnHit => {
                    tor_simulation::projectile_profiles::HitRecovery::ConsumedOnHit
                }
                HitRecovery::Recoverable => {
                    tor_simulation::projectile_profiles::HitRecovery::Recoverable
                }
            },
        };
        result
            .validate_shape()
            .map_err(|_| super::fail("Invalid projectile definition"))?;
        Ok(result)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum WeaponTrait {
    TwoHanded {},
    Offhand {},
    Quickdraw {},
    Thrown { range: WeaponRangeBands },
}
impl From<WeaponTrait> for tor_simulation::weapon_profiles::WeaponTrait {
    fn from(value: WeaponTrait) -> Self {
        match value {
            WeaponTrait::TwoHanded {} => Self::TwoHanded,
            WeaponTrait::Offhand {} => Self::Offhand,
            WeaponTrait::Quickdraw {} => Self::Quickdraw,
            WeaponTrait::Thrown { range } => Self::Thrown {
                range: range.into(),
            },
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WeaponTraitKind {
    TwoHanded,
    Offhand,
    Quickdraw,
    Thrown,
}
impl From<WeaponTraitKind> for tor_simulation::weapon_profiles::WeaponTraitKind {
    fn from(value: WeaponTraitKind) -> Self {
        match value {
            WeaponTraitKind::TwoHanded => Self::TwoHanded,
            WeaponTraitKind::Offhand => Self::Offhand,
            WeaponTraitKind::Quickdraw => Self::Quickdraw,
            WeaponTraitKind::Thrown => Self::Thrown,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WeaponTraitChanges {
    pub add: Vec<WeaponTrait>,
    pub remove: Vec<WeaponTraitKind>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WeaponProfileSpec {
    pub expertise: String,
    pub launcher: Option<LauncherProfile>,
    pub base_traits: Vec<WeaponTrait>,
    pub expert_traits: WeaponTraitChanges,
}
impl WeaponProfileSpec {
    fn compile(
        &self,
    ) -> Result<tor_simulation::weapon_profiles::WeaponProfileSpec, crate::Failure> {
        Ok(tor_simulation::weapon_profiles::WeaponProfileSpec {
            expertise: tor_simulation::content_ids::ExpertiseId::new(&self.expertise)
                .map_err(|_| super::fail("Invalid weapon expertise ID"))?,
            launcher: self
                .launcher
                .as_ref()
                .map(LauncherProfile::compile)
                .transpose()?,
            base_traits: self.base_traits.iter().copied().map(Into::into).collect(),
            expert_traits: tor_simulation::weapon_profiles::WeaponTraitChanges {
                add: self
                    .expert_traits
                    .add
                    .iter()
                    .copied()
                    .map(Into::into)
                    .collect(),
                remove: self
                    .expert_traits
                    .remove
                    .iter()
                    .copied()
                    .map(Into::into)
                    .collect(),
            },
        })
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EquipmentSpec {
    pub periodic_relocation: Option<PeriodicRelocation>,
    pub weapon_profile: Option<WeaponProfileSpec>,
    pub slot: EquipmentSlot,
    pub armor: Option<ArmorSpec>,
    pub attack: Option<crate::creature_authoring::Melee>,
    #[serde(default)]
    pub modifiers: Vec<crate::creature_authoring::Modifier>,
    #[serde(default)]
    pub potency_modifiers: Vec<crate::creature_authoring::Modifier>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PeriodicRelocation {
    pub interval: u64,
    pub chance_denominator: u32,
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
        let potency_modifiers = self
            .potency_modifiers
            .iter()
            .map(crate::creature_authoring::Modifier::compile)
            .collect::<Result<Vec<_>, _>>()?;
        rules
            .validate_modifiers(&potency_modifiers)
            .map_err(|error| {
                super::fail(format!("Invalid equipment potency modifier: {error:?}"))
            })?;
        let slot = tor_simulation::EquipmentSlot::new(&self.slot)
            .map_err(|_| super::fail(format!("Invalid equipment slot ID {:?}", self.slot)))?;
        if rules.equipment_slot(&slot).is_none() {
            return Err(super::fail(format!(
                "Unknown equipment slot {:?}",
                self.slot
            )));
        }
        Ok(tor_simulation::EquipmentSpec {
            periodic_relocation: self
                .periodic_relocation
                .map(|policy| {
                    let compiled = tor_simulation::PeriodicRelocation {
                        interval: policy.interval,
                        chance_denominator: policy.chance_denominator,
                    };
                    super::require(compiled.valid(), "Invalid periodic relocation policy")?;
                    Ok::<_, crate::engine::Failure>(compiled)
                })
                .transpose()?,
            weapon_profile: self
                .weapon_profile
                .as_ref()
                .map(WeaponProfileSpec::compile)
                .transpose()?,
            armor: self.armor.map(ArmorSpec::compile).transpose()?,
            slot,
            attack: self
                .attack
                .as_ref()
                .map(|attack| attack.compile(rules))
                .transpose()?,
            modifiers,
            potency_modifiers,
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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
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
    TeleportRandom,
    TeleportToRayOrigin,
    ArrestDescent,
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
    ApplyPersistentCondition {
        condition: String,
        cancels: Option<String>,
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
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemDiscovery {
    #[default]
    SelfEffect,
    OnUse,
    Never,
}
impl<'de> Deserialize<'de> for ItemDiscovery {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match String::deserialize(deserializer)?.as_str() {
            "self_effect" => Ok(Self::SelfEffect),
            "on_use" => Ok(Self::OnUse),
            "never" => Ok(Self::Never),
            _ => Err(serde::de::Error::custom("invalid item discovery policy")),
        }
    }
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
    pub descriptors: Vec<String>,
    pub effects: Vec<Effect>,
    pub discovery: ItemDiscovery,
}
impl<'de> Deserialize<'de> for ConsumableSpec {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            #[serde(
                default,
                deserialize_with = "crate::creature_authoring::bounded_strike_vec::<_, _, { tor_simulation::limits::DESCRIPTORS_PER_EFFECT }>"
            )]
            descriptors: Vec<String>,
            #[serde(
                deserialize_with = "crate::creature_authoring::bounded_strike_vec::<_, _, { tor_simulation::limits::CONSUMABLE_EFFECTS }>"
            )]
            effects: Vec<Effect>,
            #[serde(default)]
            discovery: ItemDiscovery,
        }
        let fields: Fields = crate::creature_authoring::named_map(deserializer)?;
        Ok(Self {
            descriptors: fields.descriptors,
            effects: fields.effects,
            discovery: fields.discovery,
        })
    }
}
fn compile_payload_descriptors(
    values: &[String],
    rules: &tor_simulation::rules::Ruleset,
) -> Result<Vec<tor_simulation::content_ids::DescriptorId>, crate::Failure> {
    let values = values
        .iter()
        .map(|name| {
            tor_simulation::content_ids::DescriptorId::new(name)
                .map_err(|_| super::fail(format!("Invalid payload descriptor {name:?}")))
        })
        .collect::<Result<Vec<_>, _>>()?;
    rules
        .validate_payload_descriptors(&values)
        .map_err(|error| super::fail(format!("Invalid payload descriptors: {error:?}")))?;
    Ok(values)
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
            descriptors: compile_payload_descriptors(&self.descriptors, rules)?,
            effects: compile_item_effects(&self.effects, rules)?,
            discovery: self.discovery.into(),
        })
    }
}

/// Reading either applies bounded immediate effects or requests a class set for
/// identity selection. Physical class does not grant this capability.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReadableSpec {
    pub descriptors: Vec<String>,
    pub effects: Vec<Effect>,
    pub identify: Option<Vec<ItemClass>>,
    pub discovery: ItemDiscovery,
}
impl<'de> Deserialize<'de> for ReadableSpec {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            #[serde(
                default,
                deserialize_with = "crate::creature_authoring::bounded_strike_vec::<_, _, { tor_simulation::limits::DESCRIPTORS_PER_EFFECT }>"
            )]
            descriptors: Vec<String>,
            #[serde(
                default,
                deserialize_with = "crate::creature_authoring::bounded_strike_vec::<_, _, { tor_simulation::limits::CONSUMABLE_EFFECTS }>"
            )]
            effects: Vec<Effect>,
            #[serde(default, deserialize_with = "read_identification_classes")]
            identify: Option<Vec<ItemClass>>,
            #[serde(default)]
            discovery: ItemDiscovery,
        }
        let fields: Fields = crate::creature_authoring::named_map(deserializer)?;
        Ok(Self {
            descriptors: fields.descriptors,
            effects: fields.effects,
            identify: fields.identify,
            discovery: fields.discovery,
        })
    }
}
fn read_identification_classes<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Vec<ItemClass>>, D::Error> {
    struct Classes(Vec<ItemClass>);
    impl<'de> Deserialize<'de> for Classes {
        fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
            #[derive(Deserialize)]
            struct Token(#[serde(deserialize_with = "read_class")] ItemClass);
            fn read_class<'de, D: serde::Deserializer<'de>>(d: D) -> Result<ItemClass, D::Error> {
                let token = String::deserialize(d)?;
                ItemClass::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(token))
            }
            let tokens: Vec<Token> = crate::creature_authoring::bounded_strike_vec::<_, _, 14>(d)?;
            let classes: Vec<_> = tokens.into_iter().map(|token| token.0).collect();
            if classes.is_empty() || !classes.windows(2).all(|pair| pair[0] < pair[1]) {
                return Err(serde::de::Error::custom("invalid identification class set"));
            }
            Ok(Self(classes))
        }
    }
    Option::<Classes>::deserialize(deserializer).map(|classes| classes.map(|classes| classes.0))
}

impl ReadableSpec {
    pub(crate) fn compile(
        &self,
        rules: &tor_simulation::rules::Ruleset,
    ) -> Result<tor_simulation::ReadableSpec, crate::Failure> {
        let spec = tor_simulation::ReadableSpec {
            descriptors: compile_payload_descriptors(&self.descriptors, rules)?,
            effects: compile_item_effects(&self.effects, rules)?,
            identify: self
                .identify
                .as_ref()
                .map(|classes| classes.iter().copied().map(Into::into).collect()),
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
            TeleportRandom {},
            TeleportToRayOrigin {},
            ArrestDescent {},
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
            ApplyPersistentCondition {
                condition: String,
                // Authored absence means no opposite; canonical and stored
                // records still write and require the explicit nullable field.
                #[serde(default)]
                cancels: Option<String>,
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
            Fields::TeleportRandom {} => Self::TeleportRandom,
            Fields::TeleportToRayOrigin {} => Self::TeleportToRayOrigin,
            Fields::ArrestDescent {} => Self::ArrestDescent,
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
    fn anatomy_requires_explicit_resources_and_primary_socket() {
        let value = serde_json::json!({"slots": ["weapon"], "capacities": {"grip": 2}, "primary_attack_socket": {"kind": "socket", "slot": 0}});
        let anatomy: AnatomySpec = serde_json::from_value(value.clone()).unwrap();
        assert!(anatomy.compile(&rules()).is_ok());
        for field in ["slots", "capacities", "primary_attack_socket"] {
            let mut malformed = value.clone();
            malformed.as_object_mut().unwrap().remove(field);
            assert!(
                serde_json::from_value::<AnatomySpec>(malformed).is_err(),
                "{field}"
            );
        }
        for malformed in [
            serde_json::json!([[], {}, {"kind": "none"}]),
            serde_json::json!({"slots": [], "capacities": {}, "primary_attack_socket": {"kind": "socket", "slot": 0}}),
            serde_json::json!({"slots": [], "capacities": {"grip": 65}, "primary_attack_socket": {"kind": "none"}}),
            serde_json::json!({"slots": [], "capacities": {}, "primary_attack_socket": {"kind": "none", "slot": 0}}),
        ] {
            assert!(serde_json::from_value::<AnatomySpec>(malformed).is_err());
        }
        assert!(serde_json::from_str::<AnatomySpec>(r#"{"slots":[],"capacities":{"grip":1,"grip":2},"primary_attack_socket":{"kind":"none"}}"#).is_err());
        let none: AnatomySpec = toml::from_str(
            "slots = []\ncapacities = {}\nprimary_attack_socket = { kind = 'none' }",
        )
        .unwrap();
        assert!(none.capacities.is_empty());
        assert!(none.compile(&rules()).is_ok());
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
                capacity: tor_simulation::equipment_capacity::EquipmentCapacityUse::None,
                equip_turns: 2,
                unequip_turns: 4,
            });
        let rules = tor_simulation::rules::Ruleset::validate(document).unwrap();
        let anatomy = AnatomySpec {
            slots: vec!["tentacle".into(), "tentacle".into()],
            capacities: Default::default(),
            primary_attack_socket: PrimaryAttackSocket::None {},
        };
        let compiled = anatomy.compile(&rules).unwrap();
        assert_eq!(compiled.slots[0].as_str(), "tentacle");
        let equipment = EquipmentSpec {
            periodic_relocation: None,
            weapon_profile: None,
            armor: None,
            slot: "tentacle".into(),
            attack: None,
            modifiers: vec![],
            potency_modifiers: vec![],
        };
        assert!(equipment
            .compile(&rules)
            .unwrap()
            .validate_rules(tor_simulation::ItemClass::Tool, &rules)
            .is_ok());
        for slot in ["missing_slot", "invalid slot"] {
            assert!(AnatomySpec {
                slots: vec![slot.into()],
                capacities: Default::default(),
                primary_attack_socket: PrimaryAttackSocket::None {},
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
    fn item_payload_descriptors_share_bounded_single_resistance_validation() {
        use tor_simulation::rules::{
            DescriptorResistance, NamedDescriptor, Ruleset, RulesetDocument,
        };
        let mut document: RulesetDocument =
            toml::from_str(tor_simulation::rules::DEFAULT_RULESET_TOML).unwrap();
        for name in ["fixture_poison", "fixture_other"] {
            document.descriptors.push(NamedDescriptor {
                id: tor_simulation::content_ids::DescriptorId::new(name).unwrap(),
                name: name.into(),
                resistance: Some(DescriptorResistance {
                    skill: tor_simulation::attributes::Skill::Athletics,
                    difficulty: 12,
                }),
            });
        }
        let rules = Ruleset::validate(document).unwrap();
        for values in [
            vec![],
            vec!["fixture_poison"],
            vec!["fire", "fixture_poison"],
        ] {
            let descriptors = values.into_iter().map(str::to_owned).collect::<Vec<_>>();
            assert!(compile_payload_descriptors(&descriptors, &rules).is_ok());
            let source: ConsumableSpec = serde_json::from_value(serde_json::json!({
                "effects":[{"kind":"heal","amount":{"kind":"fixed","amount":1}}],
                "descriptors":descriptors,
            }))
            .unwrap();
            assert_eq!(
                source.compile(&rules).unwrap().descriptors.len(),
                descriptors.len()
            );
        }
        for values in [
            vec!["missing"],
            vec!["fire", "fire"],
            vec!["fixture_poison", "fixture_other"],
        ] {
            let descriptors = values.into_iter().map(str::to_owned).collect::<Vec<_>>();
            assert!(compile_payload_descriptors(&descriptors, &rules).is_err());
        }
        let omitted: ConsumableSpec =
            serde_json::from_value(serde_json::json!({"effects":[]})).unwrap();
        assert!(
            omitted.descriptors.is_empty(),
            "source omission differs from strict current saves"
        );
    }

    #[test]
    fn generic_item_effects_compile_and_reject_unknown_content_references() {
        let valid = ConsumableSpec {
            descriptors: vec![],
            discovery: ItemDiscovery::SelfEffect,
            effects: vec![Effect::Heal {
                amount: AmountExpr::Fixed { amount: 5 },
            }],
        };
        assert!(valid.compile(&rules()).is_ok());
        let missing = ConsumableSpec {
            descriptors: vec![],
            discovery: ItemDiscovery::SelfEffect,
            effects: vec![Effect::ApplyCondition {
                condition: "missing_condition".into(),
                duration: 100,
            }],
        };
        assert!(missing.compile(&rules()).is_err());
        let missing_descriptor = ConsumableSpec {
            descriptors: vec![],
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
                descriptors: vec![],
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
                descriptors: vec![],
                effects: vec![effect],
                discovery: ItemDiscovery::OnUse
            }
            .compile(&rules())
            .is_err());
        }
    }

    #[test]
    fn authored_healing_overflow_requires_exact_scalar_names() {
        for name in [
            "none",
            "improve_earliest_hit_die",
            "improve_earliest_hit_dice_by_excess",
        ] {
            let canonical = serde_json::json!({
                "kind":"heal_per_hit_die", "sides":8, "bonus":0, "overflow":name
            });
            let decoded: Effect = serde_json::from_value(canonical.clone()).unwrap();
            assert_eq!(serde_json::to_value(decoded).unwrap(), canonical);
            let source =
                format!("kind = 'heal_per_hit_die'\nsides = 8\nbonus = 0\noverflow = '{name}'");
            let decoded: Effect = toml::from_str(&source).unwrap();
            assert_eq!(serde_json::to_value(decoded).unwrap(), canonical);
            for invalid in [
                serde_json::json!({(name):null}),
                serde_json::json!([name]),
                serde_json::json!(null),
                serde_json::json!(true),
                serde_json::json!(1),
                serde_json::json!("unknown"),
            ] {
                let mut bad = canonical.clone();
                bad["overflow"] = invalid;
                assert!(
                    serde_json::from_value::<Effect>(bad.clone()).is_err(),
                    "noncanonical healing overflow: {bad}"
                );
            }
        }
    }

    #[test]
    fn authored_item_discovery_requires_exact_scalar_names() {
        for name in ["self_effect", "on_use", "never"] {
            let canonical = serde_json::json!({
                "ability":"fixture.device_ray",
                "initial_charges":{"minimum":3,"maximum":7},
                "discovery":name
            });
            let decoded: DeviceSpec = serde_json::from_value(canonical.clone()).unwrap();
            assert_eq!(serde_json::to_value(decoded).unwrap(), canonical);
            let source = format!(
                "ability = 'fixture.device_ray'\ninitial_charges = {{ minimum = 3, maximum = 7 }}\ndiscovery = '{name}'"
            );
            let decoded: DeviceSpec = toml::from_str(&source).unwrap();
            assert_eq!(serde_json::to_value(decoded).unwrap(), canonical);
            for invalid in [
                serde_json::json!({(name):null}),
                serde_json::json!([name]),
                serde_json::json!(null),
                serde_json::json!(true),
                serde_json::json!(1),
                serde_json::json!("unknown"),
            ] {
                let mut bad = canonical.clone();
                bad["discovery"] = invalid;
                assert!(
                    serde_json::from_value::<DeviceSpec>(bad.clone()).is_err(),
                    "noncanonical item discovery: {bad}"
                );
            }
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
            (
                "improve_earliest_hit_dice_by_excess",
                HealingOverflow::ImproveEarliestHitDiceByExcess,
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
                descriptors: vec![],
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
            r#"identify = ["potion"]
               discovery = "on_use""#,
        )
        .unwrap();
        let compiled = identify.compile(&rules()).unwrap();
        assert_eq!(
            compiled.identify,
            Some(vec![tor_simulation::ItemClass::Potion])
        );
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
            serde_json::json!({"identify":["potion"], "unknown":true}),
            serde_json::json!({"identify":"secret_class"}),
            serde_json::json!({"identify":"wand"}),
            serde_json::json!({"identify":[]}),
            serde_json::json!({"identify":["wand", "ring"]}),
            serde_json::json!({"identify":["wand", "wand"]}),
            serde_json::json!({"identify":[{"wand":null}]}),
            serde_json::json!({"identify":["potion"], "discovery":"infer_name"}),
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
                    descriptors: vec![],
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
                    descriptors: vec![],
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
            descriptors: vec![],
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

#[cfg(test)]
mod weapon_profile_authoring_tests {
    use super::*;
    use serde_json::json;

    fn equipment() -> EquipmentSpec {
        serde_json::from_value(json!({
            "slot":"weapon", "attack":{"skill":"light_weaponry","bonus":0,"wind_up":1,"recovery":1,
                "damage":{"components":[{"category":"keen","descriptor":null,"amount":{"type":"fixed","value":1}}],"primary":{"category":"keen","descriptor":null}}},
            "weapon_profile":{"expertise":"fixture.knife", "base_traits":[{"kind":"offhand"}],
                "expert_traits":{"add":[{"kind":"quickdraw"}],"remove":[]}}
        })).unwrap()
    }

    #[test]
    fn authored_weapon_profile_round_trips_toml_with_omitted_nullable_ranges() {
        let value = equipment();
        assert!(value.weapon_profile.as_ref().unwrap().launcher.is_none());
        let encoded = toml::to_string(&value).unwrap();
        assert_eq!(toml::from_str::<EquipmentSpec>(&encoded).unwrap(), value);
        let legacy: EquipmentSpec = toml::from_str("slot = 'weapon'").unwrap();
        assert!(legacy.weapon_profile.is_none());
    }

    #[test]
    fn compiled_weapon_profile_resolves_equipment_expertise_in_regular_validation() {
        use tor_simulation::{
            content_ids::ExpertiseId,
            expertise::{ExpertiseCategory, ExpertiseDef},
            rules::{Ruleset, RulesetDocument},
        };
        let mut document: RulesetDocument =
            toml::from_str(tor_simulation::rules::DEFAULT_RULESET_TOML).unwrap();
        let value = equipment();
        let unknown = Ruleset::validate(document.clone()).unwrap();
        assert!(value
            .compile(&unknown)
            .unwrap()
            .validate_rules(tor_simulation::ItemClass::Weapon, &unknown)
            .is_err());
        document.expertises.push(ExpertiseDef {
            id: ExpertiseId::new("fixture.knife").unwrap(),
            name: "Knife".into(),
            category: ExpertiseCategory::Equipment,
        });
        let valid = Ruleset::validate(document.clone()).unwrap();
        assert!(value
            .compile(&valid)
            .unwrap()
            .validate_rules(tor_simulation::ItemClass::Weapon, &valid)
            .is_ok());
        document.expertises.last_mut().unwrap().category = ExpertiseCategory::Utility;
        let wrong = Ruleset::validate(document).unwrap();
        assert!(value
            .compile(&wrong)
            .unwrap()
            .validate_rules(tor_simulation::ItemClass::Weapon, &wrong)
            .is_err());
        for traits in [
            json!([{"kind":"finesse"}]),
            json!([{"kind":"offhand","extra":true}]),
            json!([{"kind":"thrown"}]),
        ] {
            let mut malformed = serde_json::to_value(&value).unwrap();
            malformed["weapon_profile"]["base_traits"] = traits;
            assert!(serde_json::from_value::<EquipmentSpec>(malformed).is_err());
        }
        for range in [
            WeaponRangeBands {
                short_feet: 0,
                long_feet: 10,
            },
            WeaponRangeBands {
                short_feet: 6,
                long_feet: 10,
            },
            WeaponRangeBands {
                short_feet: 20,
                long_feet: 10,
            },
        ] {
            let mut malformed = value.clone();
            malformed.weapon_profile.as_mut().unwrap().launcher = Some(LauncherProfile {
                family: "fixture.arrow".into(),
                range,
            });
            assert!(malformed
                .compile(&valid)
                .unwrap()
                .validate_rules(tor_simulation::ItemClass::Weapon, &valid)
                .is_err());
        }
    }
}

#[cfg(test)]
mod projectile_authoring_tests {
    use super::*;
    use serde_json::json;

    fn projectile() -> ProjectileSpec {
        serde_json::from_value(json!({"family":"fixture.arrow", "recovery":"recoverable",
            "damage":{"components":[{"category":"keen","descriptor":null,
                "amount":{"type":"rolled","count":1,"sides":6,"bonus":0}}],
                "primary":{"category":"keen","descriptor":null,"sides":6}}}))
        .unwrap()
    }

    #[test]
    fn projectile_source_round_trips_and_compiles_shared_damage_without_equipment() {
        let source = projectile();
        let encoded = toml::to_string(&source).unwrap();
        assert_eq!(toml::from_str::<ProjectileSpec>(&encoded).unwrap(), source);
        let rules = tor_simulation::rules::Ruleset::validate(
            toml::from_str(tor_simulation::rules::DEFAULT_RULESET_TOML).unwrap(),
        )
        .unwrap();
        let compiled = source.compile(&rules).unwrap();
        let launcher = LauncherProfile {
            family: "fixture.arrow".into(),
            range: WeaponRangeBands {
                short_feet: 20,
                long_feet: 60,
            },
        }
        .compile()
        .unwrap();
        assert!(compiled.compatible_with(&launcher));
        for change in 0..4 {
            let mut bad = source.clone();
            match change {
                0 => bad.family = "bad family".into(),
                1 => bad.damage.primary = None,
                2 => bad.damage.primary.as_mut().unwrap().sides = Some(8),
                _ => bad.damage.components[0].descriptor = Some("unknown".into()),
            }
            assert!(bad.compile(&rules).is_err());
        }
        let mut bad = serde_json::to_value(&source).unwrap();
        bad["attack"] = json!({});
        assert!(serde_json::from_value::<ProjectileSpec>(bad).is_err());
    }
}

#[cfg(test)]
mod persistent_condition_authoring_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn persistent_effect_authoring_accepts_toml_absence_and_named_opposite_without_runtime_defaults(
    ) {
        for (suffix, cancels) in [("", None), ("\ncancels = 'fixture_b'", Some("fixture_b"))] {
            let text =
                format!("kind = 'apply_persistent_condition'\ncondition = 'fixture_a'{suffix}");
            let authored: Effect = toml::from_str(&text).unwrap();
            let canonical = json!({"kind":"apply_persistent_condition","condition":"fixture_a","cancels":cancels});
            assert_eq!(serde_json::to_value(&authored).unwrap(), canonical);
            assert_eq!(
                serde_json::from_value::<Effect>(canonical.clone()).unwrap(),
                authored
            );
            assert!(serde_json::from_value::<tor_simulation::rules::Effect>(canonical).is_ok());
            assert!(toml::from_str::<Effect>(&format!("{text}\nextra = true")).is_err());
        }
        let absent = json!({"kind":"apply_persistent_condition","condition":"fixture_a"});
        assert!(serde_json::from_value::<Effect>(absent.clone()).is_ok());
        assert!(serde_json::from_value::<tor_simulation::rules::Effect>(absent).is_err());
        for invalid in [
            json!({"fixture_b":null}),
            json!(["fixture_b"]),
            json!(true),
            json!(1),
        ] {
            let bad = json!({"kind":"apply_persistent_condition","condition":"fixture_a","cancels":invalid});
            assert!(serde_json::from_value::<Effect>(bad).is_err());
        }
        for operand in [
            "{ fixture_b = true }",
            "['fixture_b']",
            "true",
            "1",
            "nan",
            "1979-05-27",
        ] {
            let text = format!(
                "kind = 'apply_persistent_condition'\ncondition = 'fixture_a'\ncancels = {operand}"
            );
            assert!(toml::from_str::<Effect>(&text).is_err(), "{operand}");
        }
        assert!(serde_json::from_value::<Effect>(json!([
            "apply_persistent_condition",
            "fixture_a",
            null
        ]))
        .is_err());
    }

    #[test]
    fn persistent_effect_authoring_compiles_with_optional_named_opposition() {
        let mut document: tor_simulation::rules::RulesetDocument =
            toml::from_str(tor_simulation::rules::DEFAULT_RULESET_TOML).unwrap();
        for name in ["fixture_a", "fixture_b"] {
            let mut definition = document
                .conditions
                .iter()
                .find(|definition| definition.id.as_str() == "fear")
                .unwrap()
                .clone();
            definition.id = tor_simulation::content_ids::ConditionId::new(name).unwrap();
            definition.family = tor_simulation::content_ids::ConditionFamilyId::new(name).unwrap();
            definition.timing = tor_simulation::rules::ConditionTiming::UntilRemoved;
            definition.stacking = tor_simulation::rules::Stacking::Keep;
            document.conditions.push(definition);
        }
        let rules = tor_simulation::rules::Ruleset::validate(document).unwrap();
        for mut value in [
            json!({"kind":"apply_persistent_condition","condition":"fixture_a"}),
            json!({"kind":"apply_persistent_condition","condition":"fixture_a","cancels":"fixture_b"}),
        ] {
            let effect: Effect = serde_json::from_value(value.clone()).unwrap();
            let compiled = ConsumableSpec {
                descriptors: vec![],
                discovery: ItemDiscovery::OnUse,
                effects: vec![effect],
            }
            .compile(&rules)
            .unwrap();
            if value.get("cancels").is_none() {
                value["cancels"] = json!(null);
            }
            assert_eq!(serde_json::to_value(&compiled.effects[0]).unwrap(), value);
            value["duration"] = json!(100);
            assert!(serde_json::from_value::<Effect>(value).is_err());
        }
        for condition in ["fear", "missing"] {
            let spec = ConsumableSpec {
                descriptors: vec![],
                discovery: ItemDiscovery::OnUse,
                effects: vec![Effect::ApplyPersistentCondition {
                    condition: condition.into(),
                    cancels: None,
                }],
            };
            assert!(spec.compile(&rules).is_err());
        }
    }
}
