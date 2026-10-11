//! Source-owned one-HD subjects for focused simulation tests.
use std::collections::BTreeSet;
use tor_simulation::{
    attacks::MeleeAttack,
    attributes::{Attributes, ManaBinding, Skill},
    combat::DamageType,
    creatures::{CreatureBuild, Species},
    damage::{DamageComponent, DamageSpec},
    grants::Grant,
    progression::{CreatureType, HdLedger, HdSource},
    ActorId, AnatomySpec, CreatureIdentity, Game,
};

pub(crate) fn melee(bonus: i32, wind_up: u64, kind: DamageType, amount: u32) -> MeleeAttack {
    let component = DamageComponent::fixed(kind, None, amount);
    let primary = component.key();
    MeleeAttack::new(
        Skill::HeavyWeaponry,
        bonus,
        wind_up,
        40,
        DamageSpec::new(vec![component], Some(primary)).unwrap(),
    )
    .unwrap()
}

pub(crate) fn species() -> Species {
    Species {
        id: "unit_subject".into(),
        kind: CreatureType::Humanoid,
        subtypes: BTreeSet::new(),
        default_attributes: Attributes::new([0; 6]).unwrap(),
        anatomy: AnatomySpec::humanoid(),
        melee: melee(2, 60, DamageType::Impact, 4),
        grants: vec![Grant::Health(22)],
    }
}

pub(crate) fn build(species: Species) -> CreatureBuild {
    CreatureBuild::new(
        species,
        HdLedger::seeded(vec![HdSource::Racial], 42).unwrap(),
        ManaBinding::Intellect,
    )
    .unwrap()
}

pub(crate) fn configure(game: &mut Game, actor: ActorId, faction: &str, species: Species) {
    game.configure_creature(
        actor,
        CreatureIdentity {
            name: "figure".into(),
            faction: faction.into(),
        },
        build(species),
    )
    .unwrap();
}
