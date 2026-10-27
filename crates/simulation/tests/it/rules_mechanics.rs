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
    fixture_with_modifiers(speed, base, scale, adjustment, defense, vec![])
}

fn fixture_with_modifiers(
    speed: u16,
    base: u64,
    scale: Option<Attribute>,
    adjustment: i64,
    defense: i32,
    modifiers: Vec<Modifier>,
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
    species.modifiers.extend(modifiers);
    species.default_attributes = Attributes::new(ByAttribute {
        speed,
        ..Default::default()
    })
    .unwrap();
    species.modifiers.extend([
        Modifier {
            cancellable: false,
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
            cancellable: false,
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
        [
            tor_simulation::attributes::Skill::Deception,
            tor_simulation::attributes::Skill::Persuasion,
        ],
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
fn action_duration_sources_scale_actions_without_changing_speed_or_other_operands() {
    for (rates, expected_ability, expected_wait) in [
        (vec!["haste"], 32, 50),
        (vec!["haste", "haste"], 32, 50),
        (vec!["slow"], 126, 200),
        (vec!["slow", "slow"], 126, 200),
        (vec!["haste", "slow"], 63, 100),
        (vec!["slow", "haste"], 63, 100),
    ] {
        let modifiers = rates
            .iter()
            .map(|rate| {
                serde_json::from_value::<Modifier>(serde_json::json!({
                    "scope": "always", "op": {"kind": "action_duration", "value": rate}
                }))
                .expect("shared typed action-duration modifier")
            })
            .collect();
        let (mut game, actor) =
            fixture_with_modifiers(3, 100, Some(Attribute::Speed), 0, 10, modifiers);
        let (normal, normal_actor) = fixture(3, 100, Some(Attribute::Speed), 0, 10);
        let actual_creature = game.creature(actor).unwrap();
        let baseline_creature = normal.creature(normal_actor).unwrap();
        let actual = actual_creature.derived();
        let baseline = baseline_creature.derived();
        assert_eq!(actual.attributes, baseline.attributes);
        assert_eq!(actual.defenses, baseline.defenses);
        assert_eq!(actual.skills, baseline.skills);
        assert_eq!(actual.resources, baseline.resources);
        assert_eq!(
            actual.health_recovery_period,
            baseline.health_recovery_period
        );
        assert_eq!(actual.ability_values, baseline.ability_values);
        assert_eq!(
            game.ability_timing(actor, &AbilityId::new("basic_melee").unwrap())
                .unwrap(),
            tor_simulation::creatures::AbilityTiming {
                preparation: expected_ability,
                recovery: expected_ability,
            },
        );
        game.act(actor, tor_simulation::Action::Wait).unwrap();
        assert_eq!(game.tick(), expected_wait, "{rates:?}");
    }
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
fn canonical_ranged_and_falter_checks_consume_their_declared_defenses_and_speed() {
    use tor_simulation::rules::{Against, CheckActor, DefenseKind, Effect, SkillSelector};
    let rules = super::creature_fixture::ruleset();
    let bolt = rules
        .ability(&AbilityId::new("spark_lance_fire").unwrap())
        .unwrap();
    let tor_simulation::rules::StrikeSource::Authored {
        strikes: bolt_strikes,
    } = &bolt.strikes
    else {
        panic!("authored bolt")
    };
    assert_eq!(
        bolt_strikes[0].check.as_ref().unwrap().against,
        Against::Defense {
            defense: DefenseKind::Cognitive
        }
    );
    let fear = rules.ability(&AbilityId::new("falter").unwrap()).unwrap();
    let tor_simulation::rules::StrikeSource::Authored {
        strikes: fear_strikes,
    } = &fear.strikes
    else {
        panic!("authored fear")
    };
    let check = fear_strikes[0].check.as_ref().unwrap();
    assert_eq!(check.actor, CheckActor::User);
    assert_eq!(
        check.skill,
        SkillSelector::Named {
            skill: tor_simulation::attributes::Skill::Spellcasting
        }
    );
    assert_eq!(
        check.against,
        Against::Defense {
            defense: DefenseKind::Spiritual
        }
    );
    assert!(fear_strikes[0].on_failure.is_empty());
    assert!(
        matches!(fear_strikes[0].on_success.as_slice(), [Effect::ApplyCondition { condition, duration: 300 }] if condition.as_str() == "fear")
    );
    assert!(rules
        .document()
        .abilities
        .iter()
        .filter(|ability| ability.id.as_str() != "bind_wounds")
        .all(|ability| ability.timing.scaled_by == Some(Attribute::Speed)));
    assert_eq!(
        rules
            .ability(&AbilityId::new("bind_wounds").unwrap())
            .unwrap()
            .timing
            .scaled_by,
        None,
        "universal care uses its authored fixed duration"
    );
}

#[test]
fn ability_scoped_fixture_grants_only_falter_advantage_and_one_hundred_ticks() {
    use tor_simulation::{
        abilities::PlannedEffect,
        attributes::Skill,
        content_ids::{ClassId, TalentId},
    };
    let mut document = super::creature_fixture::ruleset().document().clone();
    let mut probe = super::creature_fixture::talent(
        "fixture_falter_edge_duration",
        vec![
            super::creature_fixture::add(
                Stat::Ability {
                    ability: AbilityId::new("falter").unwrap(),
                    stat: tor_simulation::rules::AbilityStat::CheckEdge,
                },
                1,
            ),
            super::creature_fixture::add(
                Stat::Ability {
                    ability: AbilityId::new("falter").unwrap(),
                    stat: tor_simulation::rules::AbilityStat::Duration,
                },
                100,
            ),
        ],
    );
    probe.prerequisites = vec![TalentId::new("binding_initiate").unwrap()];
    document.talents.push(probe);
    let content = Content::new(
        Ruleset::validate(document).unwrap(),
        super::creature_fixture::content().catalog().clone(),
    )
    .unwrap();
    let mut build = CreatureBuild::new(
        content.clone(),
        super::creature_fixture::species().id,
        HdLedger::seeded(vec![HdSource::Class(ClassId::new("mage").unwrap()); 4], 42).unwrap(),
        ManaBinding::Intellect,
        [
            tor_simulation::attributes::Skill::Deception,
            tor_simulation::attributes::Skill::Persuasion,
        ],
    )
    .unwrap();
    build =
        super::creature_fixture::with_starting_skills(build, [Skill::Spellcasting, Skill::Lore]);
    build
        .purchase(
            tor_simulation::HdIndex(0),
            tor_simulation::content_ids::PointPoolId::new("advancement").unwrap(),
            tor_simulation::creatures::PurchaseSelection::TalentForm {
                talent: TalentId::new("binding_initiate").unwrap(),
                form: tor_simulation::content_ids::FormId::new("falter").unwrap(),
            },
        )
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
    let ability = AbilityId::new("falter").unwrap();
    let before = game.ability_plan(ActorId(1), &ability, ActorId(2)).unwrap();
    build
        .select_talent(
            tor_simulation::HdIndex(3),
            TalentId::new("fixture_falter_edge_duration").unwrap(),
        )
        .unwrap();
    game.rebuild_creature(ActorId(1), build).unwrap();
    let after = game.ability_plan(ActorId(1), &ability, ActorId(2)).unwrap();
    assert_eq!(
        after.as_ability().unwrap().strikes[0]
            .check
            .as_ref()
            .unwrap()
            .bonus,
        before.as_ability().unwrap().strikes[0]
            .check
            .as_ref()
            .unwrap()
            .bonus
    );
    assert_eq!(
        after.as_ability().unwrap().strikes[0]
            .check
            .as_ref()
            .unwrap()
            .threshold,
        before.as_ability().unwrap().strikes[0]
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
    assert_eq!(
        before.as_ability().unwrap().strikes[0]
            .check
            .as_ref()
            .unwrap()
            .edge,
        0
    );
    assert_eq!(
        after.as_ability().unwrap().strikes[0]
            .check
            .as_ref()
            .unwrap()
            .edge,
        1
    );
    let melee = game
        .ability_plan(
            ActorId(1),
            &AbilityId::new("basic_melee").unwrap(),
            ActorId(2),
        )
        .unwrap();
    assert_eq!(
        melee.as_ability().unwrap().strikes[0]
            .check
            .as_ref()
            .unwrap()
            .edge,
        0
    );
}
