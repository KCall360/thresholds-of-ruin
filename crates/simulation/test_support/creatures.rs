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

// This file is compiled into both unit and standalone integration tests. The
// latter links the ordinary library, so it must not rely on feature unification
// from another workspace member to expose production-gated fixture helpers.
pub(crate) fn ruleset() -> std::sync::Arc<tor_simulation::rules::Ruleset> {
    let document = toml::from_str(tor_simulation::rules::DEFAULT_RULESET_TOML)
        .expect("bundled rules fixture must parse");
    tor_simulation::rules::Ruleset::validate(document).expect("bundled rules fixture must validate")
}

/// A deliberately opt-in modifier probe, never part of the shipped catalog.
pub(crate) fn talent(id: &str, modifiers: Vec<Modifier>) -> tor_simulation::rules::TalentDef {
    assert!(id.starts_with("fixture_"));
    tor_simulation::rules::TalentDef {
        id: tor_simulation::content_ids::TalentId::new(id).unwrap(),
        name: id.replace('_', " "),
        prerequisites: vec![],
        requirement: tor_simulation::rules::Requirement::TotalHd { minimum: 1 },
        modifiers,
        form_choices: vec![],
    }
}

// Shared with integration tests; this probe is unused in the unit-test host.
#[allow(dead_code)]
pub(crate) fn add(stat: Stat, value: i64) -> Modifier {
    Modifier {
        scope: Scope::Always,
        op: ModifierOp::Add { stat, value },
    }
}

/// Keep the actor's owners, seeds and choices while installing only selected probes.
#[allow(dead_code)] // Used by integration tests, not every host of this shared file.
pub(crate) fn with_talents(
    build: CreatureBuild,
    talents: impl IntoIterator<Item = tor_simulation::rules::TalentDef>,
) -> CreatureBuild {
    let mut document = build.ruleset().document().clone();
    document.talents.extend(talents);
    let content = Content::new(
        tor_simulation::rules::Ruleset::validate(document).unwrap(),
        build.content().catalog().clone(),
    )
    .unwrap();
    let mut changed = CreatureBuild::from_recorded(
        content,
        build.species_id().clone(),
        build.initial_attributes(),
        build.ledger().clone(),
        build.binding(),
        build.advancement().clone(),
        build.template_ids().to_vec(),
    )
    .unwrap();
    changed
        .set_future_growth(build.future_growth().clone())
        .unwrap();
    changed
}

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

pub(crate) fn natural_melee(
    bonus: i32,
    wind_up: u64,
    kind: DamageType,
    amount: u32,
) -> tor_simulation::attacks::NaturalAttack {
    tor_simulation::attacks::NaturalAttack::from_melee(melee(bonus, wind_up, kind, amount)).unwrap()
}

/// An unmodified humanoid with the baseline rolled natural attack.
pub(crate) fn unmodified_species(id: &str) -> Species {
    let component =
        DamageComponent::rolled(DamageType::Impact, None, DicePool::new(1, 4, 0).unwrap());
    let primary = component.key();
    Species {
        displacement: None,
        id: SpeciesId::new(id).unwrap(),
        kind: TypeId::new("humanoid").unwrap(),
        subtypes: BTreeSet::new(),
        default_attributes: Attributes::default(),
        anatomy: AnatomySpec::from_rules(&ruleset()),
        melee: tor_simulation::attacks::NaturalAttack::from_melee(
            MeleeAttack::new(
                Skill::HeavyWeaponry,
                0,
                60,
                40,
                DamageSpec::new(vec![component], Some(primary)).unwrap(),
            )
            .unwrap(),
        )
        .unwrap(),
        modifiers: vec![],
    }
}

pub(crate) fn species() -> Species {
    let mut species = unmodified_species("unit_subject");
    species.melee = natural_melee(2, 60, DamageType::Impact, 4);
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
        [
            tor_simulation::attributes::Skill::Deception,
            tor_simulation::attributes::Skill::Persuasion,
        ],
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
        check: None,
        participation: tor_simulation::abilities::AttemptedParticipation::target(),
        target_rider_participated: false,
        effects: vec![tor_simulation::abilities::ResolvedEffect::Damage { amount }],
    })
}

/// Explicit starting-rank fixture; production creation never chooses these skills.
pub(crate) fn with_starting_skills(
    build: CreatureBuild,
    starting_skills: [Skill; 2],
) -> CreatureBuild {
    let mut changed = CreatureBuild::from_recorded(
        build.content().clone(),
        build.species_id().clone(),
        build.initial_attributes(),
        build.ledger().clone(),
        build.binding(),
        tor_simulation::creatures::BuildAdvancement {
            starting_skills,
            ..build.advancement().clone()
        },
        build.template_ids().to_vec(),
    )
    .unwrap();
    changed
        .set_future_growth(build.future_growth().clone())
        .unwrap();
    changed
}

/// Selected-weapon stamina probe for preparation, damage and cancellation tests.
pub(crate) fn paid_melee() -> tor_simulation::rules::AbilityDef {
    toml::from_str(r#"
id = "fixture_paid_melee"
name = "Fixture paid melee"
tags = ["weapon_attack"]
basic_attack = false
cost = { resource = "stamina", start = 1, resolution = 1 }
timing = { source = { kind = "selected_weapon" }, scaled_by = "speed" }
reach = { kind = "melee" }
strikes = { kind = "authored", strikes = [{ offset = 0, check = { actor = "user", skill = { kind = "selected_weapon" }, against = { kind = "defense", defense = "physical" } }, on_success = [{ kind = "damage", damage = { kind = "selected_weapon", impact_bonus = 3 } }], on_failure = [], descriptors = [], riders = [] }] }
ai = { priority = 20, hostile = true, minimum_resource_percent = 0, range_preference = "any" }
"#).unwrap()
}

/// Descriptor-free rolled Mana probe, independent of playable elemental forms.
pub(crate) fn mana_ray() -> tor_simulation::rules::AbilityDef {
    toml::from_str(r#"
id = "fixture_mana_ray"
name = "Fixture Mana ray"
tags = []
basic_attack = false
cost = { resource = "mana", start = 1, resolution = 1 }
timing = { source = { kind = "fixed", prepare = 100, recover = 100 }, scaled_by = "speed" }
reach = { kind = "sight", range = 6 }
strikes = { kind = "authored", strikes = [{ offset = 0, check = { actor = "user", skill = { kind = "named", skill = "spellcasting" }, against = { kind = "defense", defense = "cognitive" } }, on_success = [{ kind = "damage", damage = { kind = "components", components = [{ category = "energy", amount = { kind = "rolled", dice = { count = 1, sides = 6, bonus = 1 } } }] } }], on_failure = [], descriptors = [], riders = [] }] }
ai = { priority = 10, hostile = true, minimum_resource_percent = 0, range_preference = "outside_melee" }
"#).unwrap()
}

/// Focus-funded condition probe keeps this resource and Intimidation covered.
pub(crate) fn focus_fear() -> tor_simulation::rules::AbilityDef {
    toml::from_str(r#"
id = "fixture_focus_fear"
name = "Fixture Focus fear"
tags = []
basic_attack = false
cost = { resource = "focus", start = 1, resolution = 1 }
timing = { source = { kind = "fixed", prepare = 100, recover = 100 }, scaled_by = "speed" }
reach = { kind = "sight", range = 6 }
strikes = { kind = "authored", strikes = [{ offset = 0, check = { actor = "user", skill = { kind = "named", skill = "intimidation" }, against = { kind = "defense", defense = "spiritual" } }, on_success = [{ kind = "apply_condition", condition = "fear", duration = 300 }], on_failure = [], descriptors = ["fear", "mind_affecting"], riders = [] }] }
ai = { priority = 30, hostile = true, minimum_resource_percent = 100, range_preference = "any" }
"#).unwrap()
}
