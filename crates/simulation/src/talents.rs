//! One-time talent cards. Eligibility uses permanent build facts and active
//! prerequisite choices, never the benefits granted by the candidate itself.

use crate::attributes::{Attributes, Skill, SkillRanks};
use crate::combat::DamageType;
use crate::grants::{Ability, Grant, Selector};
use crate::progression::{Class, CreatureType, HdLedger};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
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

impl Talent {
    /// Topological catalog order. Keep the catalog invariant test with changes.
    pub const ALL: [Self; 27] = [
        Self::Hardiness,
        Self::Toughness,
        Self::Unyielding,
        Self::Indomitable,
        Self::Endurance,
        Self::DeepEndurance,
        Self::Tireless,
        Self::Guard,
        Self::GreaterGuard,
        Self::IronGuard,
        Self::HeavyBlows,
        Self::MightyBlows,
        Self::CrushingBlows,
        Self::PerfectedBlows,
        Self::ImpactWard,
        Self::KeenWard,
        Self::EnergyWard,
        Self::Resolve,
        Self::PowerStrike,
        Self::MagicBolt,
        Self::Fear,
        Self::ArcaneReserve,
        Self::PotentBolt,
        Self::EmpoweredBolt,
        Self::GreaterBolt,
        Self::MasterBolt,
        Self::FearMastery,
    ];

    pub fn predecessor(self) -> Option<Self> {
        match self {
            Self::Toughness => Some(Self::Hardiness),
            Self::Unyielding => Some(Self::Toughness),
            Self::Indomitable => Some(Self::Unyielding),
            Self::DeepEndurance => Some(Self::Endurance),
            Self::Tireless => Some(Self::DeepEndurance),
            Self::GreaterGuard => Some(Self::Guard),
            Self::IronGuard => Some(Self::GreaterGuard),
            Self::MightyBlows => Some(Self::HeavyBlows),
            Self::CrushingBlows => Some(Self::MightyBlows),
            Self::PerfectedBlows => Some(Self::CrushingBlows),
            Self::PotentBolt => Some(Self::MagicBolt),
            Self::EmpoweredBolt => Some(Self::PotentBolt),
            Self::GreaterBolt => Some(Self::EmpoweredBolt),
            Self::MasterBolt => Some(Self::GreaterBolt),
            Self::FearMastery => Some(Self::Fear),
            _ => None,
        }
    }

    pub fn minimum_hd(self) -> u16 {
        match self {
            Self::Toughness | Self::DeepEndurance | Self::GreaterGuard | Self::MightyBlows => 4,
            Self::Unyielding | Self::Tireless | Self::IronGuard | Self::CrushingBlows => 8,
            Self::Indomitable | Self::PerfectedBlows => 12,
            _ => 1,
        }
    }

    pub fn minimum_mage_level(self) -> u16 {
        match self {
            Self::MagicBolt | Self::Fear => 1,
            Self::PotentBolt => 2,
            Self::EmpoweredBolt | Self::FearMastery => 4,
            Self::GreaterBolt => 8,
            Self::MasterBolt => 12,
            _ => 0,
        }
    }

    pub fn eligible(self, facts: &TalentFacts<'_>, active: &BTreeSet<Self>) -> bool {
        if facts.ledger.total_hd() < self.minimum_hd()
            || facts.ledger.class_level(Class::Mage) < self.minimum_mage_level()
            || self
                .predecessor()
                .is_some_and(|prerequisite| !active.contains(&prerequisite))
        {
            return false;
        }
        match self {
            Self::PowerStrike => {
                facts.skills.get(facts.melee_skill) > 0
                    && (facts.ledger.class_level(Class::Warrior) > 0
                        || (matches!(facts.kind, CreatureType::Animal | CreatureType::Humanoid)
                            && facts.ledger.racial_hd() >= 2))
            }
            Self::MagicBolt => facts.skills.get(Skill::Spellcasting) > 0,
            Self::Fear => !facts.mindless && facts.skills.get(Skill::Intimidation) > 0,
            Self::ArcaneReserve => facts.magical,
            _ => true,
        }
    }

    pub fn grants(self) -> &'static [Grant] {
        match self {
            Self::Hardiness => &[Grant::Health(2)],
            Self::Toughness => &[Grant::Health(4)],
            Self::Unyielding => &[Grant::Health(6)],
            Self::Indomitable => &[Grant::Health(8)],
            Self::Endurance | Self::DeepEndurance | Self::Tireless => &[Grant::Stamina(2)],
            Self::Guard | Self::GreaterGuard | Self::IronGuard => &[Grant::PhysicalDefense(1)],
            Self::HeavyBlows => &[Grant::MeleeFlat(1)],
            Self::MightyBlows | Self::CrushingBlows | Self::PerfectedBlows => {
                &[Grant::MeleeDice(1)]
            }
            Self::ImpactWard => &[Grant::Reduction(Selector::Category(DamageType::Impact), 2)],
            Self::KeenWard => &[Grant::Reduction(Selector::Category(DamageType::Keen), 2)],
            Self::EnergyWard => &[Grant::Reduction(Selector::Category(DamageType::Energy), 2)],
            Self::Resolve => &[Grant::Focus(2)],
            Self::PowerStrike => &[Grant::Ability(Ability::PowerStrike)],
            Self::MagicBolt => &[Grant::Ability(Ability::MagicBolt)],
            Self::Fear => &[Grant::Ability(Ability::Fear)],
            Self::ArcaneReserve => &[Grant::Mana(2)],
            Self::PotentBolt => &[Grant::BoltFlat(1)],
            Self::EmpoweredBolt | Self::GreaterBolt | Self::MasterBolt => &[Grant::BoltDice(1)],
            Self::FearMastery => &[Grant::FearDifficulty(1), Grant::FearDuration(100)],
        }
    }
}

pub struct TalentFacts<'a> {
    pub ledger: &'a HdLedger,
    pub kind: CreatureType,
    /// Permanent composition attributes, excluding equipment/conditions and
    /// talent grants. Reserved for attribute prerequisites in future cards.
    pub attributes: Attributes,
    pub skills: SkillRanks,
    pub melee_skill: Skill,
    pub magical: bool,
    pub mindless: bool,
}

pub fn active_talents(facts: &TalentFacts<'_>, selected: &BTreeSet<Talent>) -> BTreeSet<Talent> {
    let mut active = BTreeSet::new();
    for talent in Talent::ALL {
        if selected.contains(&talent) && talent.eligible(facts, &active) {
            active.insert(talent);
        }
    }
    active
}
