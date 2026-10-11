//! Source-owned one-HD subjects for focused simulation tests.
use std::collections::BTreeSet;
use tor_simulation::{
    attacks::MeleeAttack,
    attributes::{Attributes, ManaBinding, Skill},
    combat::DamageType,
    content_ids::{SpeciesId, TypeId},
    creatures::{Content, CreatureBuild, ScenarioCatalog, Species, Template},
    damage::{DamageComponent, DamageSpec},
    dice::DicePool,
    progression::{HdLedger, HdSource, HitDie},
    rules::{Modifier, ModifierOp, Scope, Stat},
    ActorId, AnatomySpec, CreatureIdentity, Game,
};

pub(crate) use tor_simulation::test_support::test_ruleset as ruleset;

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

/// An unmodified humanoid with the baseline rolled natural attack.
pub(crate) fn unmodified_species(id: &str) -> Species {
    let component =
        DamageComponent::rolled(DamageType::Impact, None, DicePool::new(1, 4, 0).unwrap());
    let primary = component.key();
    Species {
        id: SpeciesId::new(id).unwrap(),
        kind: TypeId::new("humanoid").unwrap(),
        subtypes: BTreeSet::new(),
        default_attributes: Attributes::default(),
        anatomy: AnatomySpec::from_rules(&ruleset()),
        melee: MeleeAttack::new(
            Skill::HeavyWeaponry,
            0,
            60,
            40,
            DamageSpec::new(vec![component], Some(primary)).unwrap(),
        )
        .unwrap(),
        modifiers: vec![],
    }
}

pub(crate) fn species() -> Species {
    let mut species = unmodified_species("unit_subject");
    species.melee = melee(2, 60, DamageType::Impact, 4);
    species.modifiers.push(Modifier {
        scope: Scope::Always,
        op: ModifierOp::Add {
            stat: Stat::Health,
            value: 22,
        },
    });
    species
}

pub(crate) fn content_with(species: Species, templates: Vec<Template>) -> std::sync::Arc<Content> {
    Content::new(
        ruleset(),
        ScenarioCatalog {
            species: std::collections::BTreeMap::from([(species.id.clone(), species)]),
            templates: templates.into_iter().map(|t| (t.id.clone(), t)).collect(),
        },
    )
    .unwrap()
}

pub(crate) fn content() -> std::sync::Arc<Content> {
    content_with(species(), vec![])
}

pub(crate) fn build_with(
    species: Species,
    templates: Vec<Template>,
    sources: &[HdSource],
) -> CreatureBuild {
    let id = species.id.clone();
    CreatureBuild::new(
        content_with(species, templates),
        id,
        HdLedger::new(
            sources
                .iter()
                .cloned()
                .map(|source| HitDie::new(source, 42))
                .collect(),
        )
        .unwrap(),
        ManaBinding::Intellect,
    )
    .unwrap()
}

pub(crate) fn build(species: Species) -> CreatureBuild {
    build_with(species, vec![], &[HdSource::Racial])
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

/// Apply resolved injury through the same public transaction as production effects.
pub(crate) fn injure(
    game: &mut Game,
    actor: ActorId,
    amount: u32,
) -> Result<(), tor_simulation::GameError> {
    game.apply(tor_simulation::abilities::Outcome {
        actor,
        target: actor,
        ability: None,
        intention: None,
        result: tor_simulation::abilities::OutcomeStatus::Applied,
        effects: vec![tor_simulation::abilities::ResolvedEffect::Damage { amount }],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tor_simulation::damage::DamageAmount;

    #[test]
    fn unmodified_subject_keeps_baseline_rolled_melee_without_fixture_bonuses() {
        let subject = unmodified_species("baseline_subject");
        assert_eq!(subject.id.as_str(), "baseline_subject");
        assert_eq!(subject.kind.as_str(), "humanoid");
        assert_eq!(subject.default_attributes, Attributes::default());
        assert!(subject.subtypes.is_empty());
        assert!(subject.modifiers.is_empty());
        assert_eq!(subject.melee.skill(), Skill::HeavyWeaponry);
        assert_eq!(subject.melee.bonus(), 0);
        assert_eq!(
            (subject.melee.wind_up(), subject.melee.recovery()),
            (60, 40)
        );
        let [component] = subject.melee.damage().components() else {
            panic!("baseline natural melee must have one damage component");
        };
        assert_eq!(component.key().category, DamageType::Impact);
        assert_eq!(component.key().descriptor, None);
        assert_eq!(subject.melee.damage().primary(), Some(component.key()));
        let DamageAmount::Rolled(pool) = component.amount() else {
            panic!("baseline natural melee must roll its damage");
        };
        assert_eq!((pool.count(), pool.sides(), pool.bonus()), (1, 4, 0));
        content_with(subject, vec![]);
    }
}
