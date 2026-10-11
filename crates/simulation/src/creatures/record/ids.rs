use super::*;

pub(super) trait CatalogId: Sized {
    fn id(self) -> u8;
    fn decode(id: u8) -> Result<Self, BuildError>;
}
macro_rules! catalog {
    ($kind:ty; $($variant:ident = $id:literal),+ $(,)?) => {
        impl CatalogId for $kind {
            fn id(self) -> u8 { match self { $(Self::$variant => $id),+ } }
            fn decode(id: u8) -> Result<Self, BuildError> { match id { $($id => Ok(Self::$variant)),+, _ => Err(BuildError::InvalidDefinition) } }
        }
    };
}
catalog!(Attribute; Strength=0, Speed=1, Intellect=2, Willpower=3, Awareness=4, Presence=5);
catalog!(ManaBinding; Intellect=0, Willpower=1, Awareness=2, Presence=3);
catalog!(Skill; Athletics=0, HeavyWeaponry=1, Agility=2, LightWeaponry=3, Stealth=4, Thievery=5, Crafting=6, Deduction=7, Lore=8, Medicine=9, Discipline=10, Intimidation=11, Insight=12, Perception=13, Survival=14, Deception=15, Leadership=16, Persuasion=17, Spellcasting=18);
catalog!(CreatureType; Aberration=0, Animal=1, Construct=2, Dragon=3, Elemental=4, Fey=5, Giant=6, Humanoid=7, MagicalBeast=8, MonstrousHumanoid=9, Ooze=10, Outsider=11, Plant=12, Undead=13, Vermin=14);
catalog!(Subtype; Air=0, Angel=1, Aquatic=2, Archon=3, Augmented=4, Chaotic=5, Cold=6, Earth=7, Evil=8, Extraplanar=9, Fire=10, Goblinoid=11, Good=12, Incorporeal=13, Lawful=14, Native=15, Reptilian=16, Shapechanger=17, Swarm=18, Water=19);
catalog!(Class; Warrior=0, Mage=1);
catalog!(Resource; Stamina=0, Focus=1, Mana=2);
catalog!(DamageType; Energy=0, Impact=1, Keen=2, Spirit=3, Vital=4);
catalog!(Descriptor; Fire=0, Cold=1, Fear=2, MindAffecting=3);
catalog!(Ability; BasicMelee=0, PowerStrike=1, MagicBolt=2, Fear=3);
catalog!(Talent; Hardiness=0, Toughness=1, Unyielding=2, Indomitable=3, Endurance=4, DeepEndurance=5, Tireless=6, Guard=7, GreaterGuard=8, IronGuard=9, HeavyBlows=10, MightyBlows=11, CrushingBlows=12, PerfectedBlows=13, ImpactWard=14, KeenWard=15, EnergyWard=16, Resolve=17, PowerStrike=18, MagicBolt=19, Fear=20, ArcaneReserve=21, PotentBolt=22, EmpoweredBolt=23, GreaterBolt=24, MasterBolt=25, FearMastery=26);
catalog!(EquipmentSlot; Weapon=0, BodyArmor=1, Shield=2, HeadArmor=3, HandsArmor=4, FeetArmor=5, Cloak=6, Ring=7, Amulet=8);

#[cfg(test)]
mod tests {
    use super::*;

    fn check<T: CatalogId>(count: u8) {
        for id in 0..=u8::MAX {
            match T::decode(id) {
                Ok(value) => {
                    assert!(id < count);
                    assert_eq!(value.id(), id);
                }
                Err(_) => assert!(id >= count),
            }
        }
    }

    #[test]
    fn every_catalog_id_round_trips_and_unknown_ids_reject() {
        check::<Attribute>(6);
        check::<ManaBinding>(4);
        check::<Skill>(19);
        check::<CreatureType>(15);
        check::<Subtype>(20);
        check::<Class>(2);
        check::<DamageType>(5);
        check::<Descriptor>(4);
        check::<Ability>(4);
        check::<Talent>(27);
        check::<EquipmentSlot>(9);
    }
}
