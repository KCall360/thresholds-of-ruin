//! Exact personal creature information. Enemy views carry only qualitative
//! combat observations. These DTOs do not contain simulation or save records.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnStats {
    pub kind: CreatureType,
    pub subtypes: Vec<CreatureSubtype>,
    pub hit_dice: Vec<HitDieSource>,
    pub attributes: AttributeView,
    pub skills: Vec<SkillView>,
    pub defenses: DefenseView,
    pub binding: ManaBinding,
    pub resources: Vec<ResourceView>,
    pub active_talents: Vec<Talent>,
    pub dormant_talents: Vec<Talent>,
    /// Granted techniques, not a promise of target validity or affordability.
    pub abilities: Vec<Technique>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttributeView {
    pub strength: u16,
    pub speed: u16,
    pub intellect: u16,
    pub willpower: u16,
    pub awareness: u16,
    pub presence: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DefenseView {
    pub physical: i32,
    pub cognitive: i32,
    pub spiritual: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkillView {
    pub skill: Skill,
    pub rank: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceView {
    pub resource: Resource,
    pub balance: u32,
    pub maximum: u32,
    pub available: u32,
    pub reserved: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CreatureType {
    Aberration,
    Animal,
    Construct,
    Dragon,
    Elemental,
    Fey,
    Giant,
    Humanoid,
    MagicalBeast,
    MonstrousHumanoid,
    Ooze,
    Outsider,
    Plant,
    Undead,
    Vermin,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CreatureSubtype {
    Air,
    Angel,
    Aquatic,
    Archon,
    Augmented,
    Chaotic,
    Cold,
    Earth,
    Evil,
    Extraplanar,
    Fire,
    Goblinoid,
    Good,
    Incorporeal,
    Lawful,
    Native,
    Reptilian,
    Shapechanger,
    Swarm,
    Water,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HitDieSource {
    Racial,
    Warrior,
    Mage,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ManaBinding {
    Intellect,
    Willpower,
    Awareness,
    Presence,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Resource {
    Stamina,
    Focus,
    Mana,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Skill {
    Athletics,
    HeavyWeaponry,
    Agility,
    LightWeaponry,
    Stealth,
    Thievery,
    Crafting,
    Deduction,
    Lore,
    Medicine,
    Discipline,
    Intimidation,
    Insight,
    Perception,
    Survival,
    Deception,
    Leadership,
    Persuasion,
    Spellcasting,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Talent {
    Hardiness,
    Toughness,
    Unyielding,
    Indomitable,
    Endurance,
    DeepEndurance,
    Tireless,
    Guard,
    GreaterGuard,
    IronGuard,
    HeavyBlows,
    MightyBlows,
    CrushingBlows,
    PerfectedBlows,
    ImpactWard,
    KeenWard,
    EnergyWard,
    Resolve,
    PowerStrike,
    MagicBolt,
    Fear,
    ArcaneReserve,
    PotentBolt,
    EmpoweredBolt,
    GreaterBolt,
    MasterBolt,
    FearMastery,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Technique {
    BasicMelee,
    PowerStrike,
    MagicBolt,
    Fear,
}
