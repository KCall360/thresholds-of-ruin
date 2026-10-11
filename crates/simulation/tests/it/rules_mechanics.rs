//! Numeric rules use the authoritative derived cache and generic plan timing.
use std::{collections::BTreeMap, num::NonZeroU64};
use tor_simulation::{
    attributes::{Attribute, Attributes, ByAttribute, ManaBinding},
    content_ids::AbilityId,
    creatures::{Content, CreatureBuild, ScenarioCatalog},
    progression::{HdLedger, HdSource, HitDie},
    rules::{Modifier, ModifierOp, Ruleset, Scope, Stat, TimingSource},
    ActorId, CreatureIdentity, Game,
};
use tor_world::{Location, Position, RegionId};

fn fixture(
    speed: u16,
    base: u64,
    scale: Option<Attribute>,
    adjustment: i64,
    defense: i32,
) -> (Game, ActorId) {
    let mut document = super::creature_fixture::ruleset().document().clone();
    document.constants.base_defense = defense;
    let ability = AbilityId::new("basic_melee").unwrap();
    let definition = document
        .abilities
        .iter_mut()
        .find(|def| def.id == ability)
        .unwrap();
    definition.timing.source = TimingSource::Fixed {
        prepare: base,
        recover: base,
    };
    definition.timing.scaled_by = scale;
    let rules = Ruleset::validate(document).unwrap();
    let mut species = super::creature_fixture::species();
    species.default_attributes = Attributes::new(ByAttribute {
        speed,
        ..Default::default()
    })
    .unwrap();
    species.modifiers.extend([
        Modifier {
            scope: Scope::Always,
            op: ModifierOp::Add {
                stat: Stat::Ability {
                    ability: ability.clone(),
                    stat: tor_simulation::rules::AbilityStat::Preparation,
                },
                value: adjustment,
            },
        },
        Modifier {
            scope: Scope::Always,
            op: ModifierOp::Add {
                stat: Stat::Ability {
                    ability,
                    stat: tor_simulation::rules::AbilityStat::Recovery,
                },
                value: adjustment,
            },
        },
    ]);
    let species_id = species.id.clone();
    let content = Content::new(
        rules,
        ScenarioCatalog {
            species: BTreeMap::from([(species_id.clone(), species)]),
            templates: BTreeMap::new(),
        },
    )
    .unwrap();
    let build = CreatureBuild::new(
        content.clone(),
        species_id,
        HdLedger::new(vec![HitDie::new(HdSource::Racial, 42)]).unwrap(),
        ManaBinding::Intellect,
    )
    .unwrap();
    let mut game = Game::two_room_in_stone(42, content);
    let actor = game
        .spawn_actor(
            Location {
                region: RegionId(1),
                position: Position { x: 1, y: 1, z: 0 },
            },
            NonZeroU64::new(100).unwrap(),
        )
        .unwrap();
    game.configure_creature(
        actor,
        CreatureIdentity {
            name: "subject".into(),
            faction: "neutral".into(),
        },
        build,
    )
    .unwrap();
    (game, actor)
}

#[test]
fn data_designates_timing_scale_with_ceil_minimum_and_authoring_bound() {
    for (speed, base, scale, adjustment, expected) in [
        (0, 100, Some(Attribute::Speed), 0, 100),
        (3, 100, Some(Attribute::Speed), 0, 63),
        (3, 1, Some(Attribute::Speed), 0, 1),
        (3, 1, Some(Attribute::Speed), -1, 1),
        (3, 100, None, 0, 100),
        (0, 1_000_000, Some(Attribute::Speed), 0, 1_000_000),
        (3, 1_000_000, Some(Attribute::Speed), 0, 625_000),
    ] {
        let (game, actor) = fixture(speed, base, scale, adjustment, 10);
        assert_eq!(
            game.ability_timing(actor, &AbilityId::new("basic_melee").unwrap())
                .unwrap(),
            tor_simulation::creatures::AbilityTiming {
                preparation: expected,
                recovery: expected
            }
        );
    }
}

#[test]
fn every_defense_uses_ruleset_base_and_the_derived_attribute_pair() {
    for base in [0, 10, 17] {
        let (game, actor) = fixture(3, 100, None, 0, base);
        let creature = game.creature(actor).unwrap();
        let defenses = creature.derived().defenses;
        assert_eq!(defenses.physical, base + 3);
        assert_eq!(defenses.cognitive, base);
        assert_eq!(defenses.spiritual, base);
    }
}

#[test]
fn canonical_ranged_and_fear_checks_consume_their_declared_defenses_and_speed() {
    use tor_simulation::rules::{Against, CheckActor, DefenseKind, Effect, SkillSelector};
    let rules = super::creature_fixture::ruleset();
    let bolt = rules
        .ability(&AbilityId::new("magic_bolt").unwrap())
        .unwrap();
    assert_eq!(
        bolt.check.as_ref().unwrap().against,
        Against::Defense {
            defense: DefenseKind::Cognitive
        }
    );
    let fear = rules.ability(&AbilityId::new("fear").unwrap()).unwrap();
    let check = fear.check.as_ref().unwrap();
    assert_eq!(check.actor, CheckActor::User);
    assert_eq!(
        check.skill,
        SkillSelector::Named {
            skill: tor_simulation::attributes::Skill::Intimidation
        }
    );
    assert_eq!(
        check.against,
        Against::Defense {
            defense: DefenseKind::Spiritual
        }
    );
    assert!(fear.on_failure.is_empty());
    assert!(
        matches!(fear.on_success.as_slice(), [Effect::ApplyCondition { condition, duration: 300 }] if condition.as_str() == "fear")
    );
    assert!(rules
        .document()
        .abilities
        .iter()
        .all(|ability| ability.timing.scaled_by == Some(Attribute::Speed)));
}

#[test]
fn fear_mastery_adds_one_to_the_real_check_and_one_hundred_to_duration() {
    use tor_simulation::{
        abilities::{CheckThreshold, PlannedEffect},
        attributes::Skill,
        content_ids::{ClassId, TalentId},
    };
    let content = super::creature_fixture::content();
    let mut build = CreatureBuild::new(
        content.clone(),
        super::creature_fixture::species().id,
        HdLedger::seeded(vec![HdSource::Class(ClassId::new("mage").unwrap()); 4], 42).unwrap(),
        ManaBinding::Intellect,
    )
    .unwrap();
    build.train(0, Skill::Intimidation).unwrap();
    build
        .select_talent(0, TalentId::new("fear").unwrap())
        .unwrap();
    let mut game = Game::two_room_in_stone(42, content);
    for x in [1, 2] {
        let actor = game
            .spawn_actor(
                Location {
                    region: RegionId(1),
                    position: Position { x, y: 1, z: 0 },
                },
                NonZeroU64::new(100).unwrap(),
            )
            .unwrap();
        game.configure_creature(
            actor,
            CreatureIdentity {
                name: "subject".into(),
                faction: "neutral".into(),
            },
            build.clone(),
        )
        .unwrap();
    }
    let ability = AbilityId::new("fear").unwrap();
    let before = game.ability_plan(ActorId(1), &ability, ActorId(2)).unwrap();
    build
        .select_talent(3, TalentId::new("fear_mastery").unwrap())
        .unwrap();
    game.rebuild_creature(ActorId(1), build).unwrap();
    let after = game.ability_plan(ActorId(1), &ability, ActorId(2)).unwrap();
    assert_eq!(
        after.as_ability().unwrap().check.as_ref().unwrap().bonus,
        before.as_ability().unwrap().check.as_ref().unwrap().bonus + 1
    );
    assert_eq!(
        after
            .as_ability()
            .unwrap()
            .check
            .as_ref()
            .unwrap()
            .threshold,
        before
            .as_ability()
            .unwrap()
            .check
            .as_ref()
            .unwrap()
            .threshold
    );
    assert!(matches!(
        before.on_success(),
        [PlannedEffect::ApplyCondition { duration: 300, .. }]
    ));
    assert!(matches!(
        after.on_success(),
        [PlannedEffect::ApplyCondition { duration: 400, .. }]
    ));
    // Enumerate every no-edge d20 result against the planned operands, rather
    // than approximating the five-percentage-point benefit with random trials.
    let CheckThreshold::Fixed { value: threshold } = before
        .as_ability()
        .unwrap()
        .check
        .as_ref()
        .unwrap()
        .threshold
    else {
        panic!("fixed defense")
    };
    let successes = |bonus| {
        (1..=20)
            .filter(|roll| i64::from(*roll) + bonus >= threshold)
            .count()
    };
    assert_eq!(
        successes(after.as_ability().unwrap().check.as_ref().unwrap().bonus),
        successes(before.as_ability().unwrap().check.as_ref().unwrap().bonus) + 1
    );
}
