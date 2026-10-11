//! Source-owned one-HD subjects for focused simulation tests.
use std::collections::BTreeSet;
use tor_simulation::{
    attacks::MeleeAttack,
    attributes::{Attributes, ByAttribute, ManaBinding, Skill},
    combat::DamageType,
    content_ids::{SpeciesId, TypeId},
    creatures::{Content, CreatureBuild, ScenarioCatalog, Species, Template},
    damage::{DamageComponent, DamageSpec},
    progression::{HdLedger, HdSource, HitDie},
    rules::{Modifier, ModifierOp, Ruleset, Scope, Stat, DEFAULT_RULESET_TOML},
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
        id: SpeciesId::new("unit_subject").unwrap(),
        kind: TypeId::new("humanoid").unwrap(),
        subtypes: BTreeSet::new(),
        default_attributes: Attributes::new(ByAttribute::default()).unwrap(),
        anatomy: AnatomySpec::humanoid(),
        melee: melee(2, 60, DamageType::Impact, 4),
        modifiers: vec![Modifier {
            scope: Scope::Always,
            op: ModifierOp::Add {
                stat: Stat::Health,
                value: 22,
            },
        }],
    }
}

pub(crate) fn ruleset() -> std::sync::Arc<Ruleset> {
    Ruleset::validate(toml::from_str(DEFAULT_RULESET_TOML).unwrap()).unwrap()
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
