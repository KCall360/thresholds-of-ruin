use super::*;
use crate::content_ids::{AbilityId, BodyFormId, ClassId};
use crate::creatures::{
    BodyPackage, Content, CreatureBuild, CreatureState, HdChoices, PlannedAdvancement,
};
use crate::progression::{HdLedger, HdSource};
use crate::rules::{Against, CheckActor, CheckDef, Effect, Reach, SkillSelector, StrikeSource};
use crate::{ActorId, Game, ItemId};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

fn form(value: &str) -> BodyFormId {
    BodyFormId::new(value).unwrap()
}
fn fixture() -> (Game, ItemId, Arc<BodyPackage>) {
    fixture_with_discovery(crate::ItemDiscovery::OnUse)
}
fn fixture_with_discovery(discovery: crate::ItemDiscovery) -> (Game, ItemId, Arc<BodyPackage>) {
    let source = super::ray_tests::fixture_with(|_, ray, _| {
        ray.invocation = InvocationKind::DirectionalContactRay;
        ray.reach = Reach::UntilObstruction;
        let StrikeSource::Authored { strikes } = &mut ray.strikes else {
            unreachable!()
        };
        strikes[0].descriptors.clear();
        strikes[0].riders.clear();
        strikes[0].check = Some(CheckDef {
            actor: CheckActor::Target,
            skill: SkillSelector::Named {
                skill: crate::attributes::Skill::Agility,
            },
            against: Against::Opposed {
                skill: SkillSelector::Named {
                    skill: crate::attributes::Skill::Lore,
                },
            },
        });
        strikes[0].on_success.clear();
        strikes[0].on_failure = vec![Effect::BalefulPolymorph {
            forms: vec![form("base"), form("one"), form("two")],
        }];
    });
    let species = source
        .creature(ActorId(1))
        .unwrap()
        .build()
        .species_id()
        .clone();
    let packages: BTreeMap<_, _> = ["base", "one", "two"]
        .into_iter()
        .map(|id| {
            let package = Arc::new(BodyPackage {
                id: form(id),
                species: species.clone(),
                body: if id == "two" {
                    crate::BodySpec {
                        cells: vec![[0, 0, 0], [0, 1, 0], [0, 2, 0]],
                        ..crate::BodySpec::default()
                    }
                } else {
                    crate::BodySpec::default()
                },
                advancement: vec![PlannedAdvancement {
                    source: HdSource::Racial,
                    choices: HdChoices::default(),
                }],
            });
            (package.id.clone(), package)
        })
        .collect();
    let base = packages[&form("base")].clone();
    let content = Content::with_body_packages(source.content(), packages).unwrap();
    let content = Content::with_polymorph_forms(
        &content,
        [form("base"), form("one"), form("two")]
            .into_iter()
            .collect(),
    )
    .unwrap();
    let build = CreatureBuild::new(
        content.clone(),
        species,
        HdLedger::seeded(
            vec![
                HdSource::Racial,
                HdSource::Class(ClassId::new("warrior").unwrap()),
            ],
            42,
        )
        .unwrap(),
        crate::attributes::ManaBinding::Intellect,
        [
            crate::attributes::Skill::Deception,
            crate::attributes::Skill::Persuasion,
        ],
    )
    .unwrap()
    .with_body_package(base.clone(), 42)
    .unwrap();
    let mut game = Game::two_room_in_stone(43, content);
    let at = tor_world::Location {
        region: tor_world::RegionId(1),
        position: tor_world::Position { x: 1, y: 1, z: 0 },
    };
    let actor = game
        .spawn_actor(at, std::num::NonZeroU64::new(100).unwrap())
        .unwrap();
    game.configure_creature(
        actor,
        crate::CreatureIdentity {
            name: "subject".into(),
            faction: "fixture".into(),
        },
        build,
    )
    .unwrap();
    let mut item = crate::ItemSpec::ordinary("Polymorph test wand".into());
    item.identity = "fixture.polymorph_wand".into();
    item.class = crate::ItemClass::Tool;
    item.concealed = true;
    item.device = Some(crate::DeviceSpec {
        ability: AbilityId::new("fixture_ray").unwrap(),
        initial_charges: crate::ChargeRange {
            minimum: 2,
            maximum: 2,
        },
        discovery,
    });
    let item = game
        .place_item_stack(game.next_item_id, at, Some(actor), 1, item)
        .unwrap();
    (game, item, base)
}

#[test]
fn polymorph_rosters_and_private_result_records_are_strict() {
    let valid = serde_json::json!({"kind":"baleful_polymorph","forms":["one","two"]});
    let effect = serde_json::from_value::<Effect>(valid.clone()).unwrap();
    assert!(effect.valid_shape());
    let plan = serde_json::from_value::<PlannedEffect>(valid).unwrap();
    assert!(super::plan::effects_match_definition(&[effect], &[plan]));
    for malformed in [
        serde_json::json!({"kind":"baleful_polymorph","forms":[]}),
        serde_json::json!({"kind":"baleful_polymorph","forms":["one","one"]}),
        serde_json::json!({"kind":"baleful_polymorph"}),
        serde_json::json!({"kind":"baleful_polymorph","forms":["one"],"unknown":true}),
    ] {
        assert!(serde_json::from_value::<Effect>(malformed.clone()).is_err());
        assert!(serde_json::from_value::<PlannedEffect>(malformed).is_err());
    }
    assert!(serde_json::from_value::<ResolvedEffect>(
        serde_json::json!({"kind":"baleful_polymorph","changed":false})
    )
    .is_err());
    assert!(serde_json::from_value::<ResolvedEffect>(
        serde_json::json!({"kind":"baleful_polymorph","form":null,"changed":true})
    )
    .is_err());
    let private = ResolvedEffect::BalefulPolymorph {
        form: Some(form("one")),
        changed: false,
    };
    assert_eq!(
        serde_json::from_value::<ResolvedEffect>(serde_json::to_value(&private).unwrap()).unwrap(),
        private
    );
}

#[test]
fn one_uniform_form_selection_excludes_current_and_has_no_geometry_reroll() {
    let (game, _, _) = fixture();
    let creature = game.creature(ActorId(1)).unwrap();
    let roster = [form("base"), form("one"), form("two")];
    assert!(!game.body_fits(
        ActorId(1),
        game.actors[&ActorId(1)].location,
        0,
        &game.content().body_package(&form("two")).unwrap().body
    ));
    // Fixed independently computed SplitMix64 witnesses, not runtime seed search:
    // seed1 -> 0x910a2dec89025cc1 (odd), seed2 -> 0x975835de1c9756ce (even).
    // The two-entry inclusive range needs one word and no rejection draw.
    for (seed, expected, expected_state) in [
        (1_u64, "two", 0x9e3779b97f4a7c16_u64),
        (2_u64, "one", 0x9e3779b97f4a7c17_u64),
    ] {
        let mut actual_rng = seed;
        let chosen = super::resolve::select_baleful_form(&creature, &roster, &mut actual_rng)
            .unwrap()
            .unwrap();
        assert_eq!(chosen, form(expected));
        assert_eq!(
            actual_rng, expected_state,
            "exactly one bounded selection, never a fit reroll"
        );
    }
    let mut rng = 77;
    assert_eq!(
        super::resolve::select_baleful_form(&creature, &[form("base")], &mut rng).unwrap(),
        None
    );
    assert_eq!(rng, 77);
    let plain_build = CreatureBuild::new(
        game.content().clone(),
        creature.build().species_id().clone(),
        HdLedger::seeded(vec![HdSource::Racial], 42).unwrap(),
        crate::attributes::ManaBinding::Intellect,
        [
            crate::attributes::Skill::Deception,
            crate::attributes::Skill::Persuasion,
        ],
    )
    .unwrap();
    let plain = CreatureState::new(plain_build).unwrap();
    assert_eq!(
        super::resolve::select_baleful_form(&plain, &roster, &mut rng).unwrap(),
        None
    );
    assert_eq!(rng, 77);
    assert!(game
        .effect_plan(
            ActorId(1),
            &[Effect::BalefulPolymorph {
                forms: vec![form("missing")]
            }]
        )
        .is_err());
    let plain = CreatureState::new(crate::test_creatures::build(
        crate::test_creatures::species(),
    ))
    .unwrap();
    // Missing catalog references remain invalid, never silently change the distribution.
    assert!(super::resolve::select_baleful_form(&plain, &roster, &mut rng).is_err());
}

#[test]
fn explicit_self_polymorph_is_known_only_and_waives_only_its_own_save() {
    let (mut game, item, _) = fixture();
    let actor = ActorId(1);
    assert!(game
        .compile_device_plan(actor, item, crate::DeviceAim::SelfTarget {}, 100)
        .is_err());
    assert_eq!(
        game.observe(actor)
            .unwrap()
            .interactions
            .unwrap()
            .inventory
            .iter()
            .find(|row| row.item == item)
            .unwrap()
            .device_activation,
        Some(crate::DeviceActivation::Direction)
    );
    game.learn_identity(actor, "fixture.polymorph_wand")
        .unwrap();
    assert_eq!(
        game.observe(actor)
            .unwrap()
            .interactions
            .unwrap()
            .inventory
            .iter()
            .find(|row| row.item == item)
            .unwrap()
            .device_activation,
        Some(crate::DeviceActivation::DirectionOrSelf)
    );
    let plan = game
        .compile_device_plan(actor, item, crate::DeviceAim::SelfTarget {}, 100)
        .unwrap();
    assert!(plan.valid(game.content().rules()));
    assert!(
        matches!(plan.resolution(), Some(ResolutionPlan::Exposure { actor: a, target: t, .. }) if a == actor && t == actor)
    );
    let outcome = resolve(
        plan.resolution().unwrap(),
        &Snapshot::capture(&game.creature(actor).unwrap()),
        &mut 42,
        &mut (),
    )
    .unwrap();
    assert!(outcome.check.is_none() && outcome.resistance.is_none());
    assert!(matches!(
        outcome.effects.as_slice(),
        [ResolvedEffect::BalefulPolymorph {
            form: Some(_),
            changed: false
        }]
    ));
    let mut forged = plan;
    let Plan::Zap { aim, .. } = &mut forged else {
        unreachable!()
    };
    *aim = crate::DeviceAim::Direction {
        direction: crate::abilities::StepDirection::East,
    };
    assert!(
        !forged.valid(game.content().rules()),
        "a directional request cannot inherit the self waiver"
    );
    let directional = game
        .compile_device_plan(
            actor,
            item,
            crate::DeviceAim::Direction {
                direction: crate::abilities::StepDirection::East,
            },
            100,
        )
        .unwrap();
    assert!(matches!(
        directional,
        Plan::Zap {
            delivery: DeviceDeliveryPlan::Ray { .. },
            ..
        }
    ));
}

#[test]
fn polymorph_ray_keeps_exact_agility_vs_lore_save_and_never_checkless_targeting() {
    let (game, _, _) = fixture();
    let definition = game
        .content()
        .rules()
        .ability(&AbilityId::new("fixture_ray").unwrap())
        .unwrap();
    assert!(super::ray::definition_shape_valid(definition));
    assert!(super::self_polymorph_device_definition_valid(definition));
    let mut bounded = definition.clone();
    bounded.reach = crate::rules::Reach::Sight { range: 4 };
    assert!(super::ray::definition_shape_valid(&bounded));
    let StrikeSource::Authored { strikes } = &mut bounded.strikes else {
        unreachable!()
    };
    strikes[0].check.as_mut().unwrap().skill = SkillSelector::Named {
        skill: crate::attributes::Skill::Discipline,
    };
    assert!(
        !super::ray::definition_shape_valid(&bounded),
        "bounded Polymorph uses the same exact opposed skills"
    );
    assert!(
        !super::self_polymorph_device_definition_valid(&bounded),
        "the self waiver cannot license an unsupported save shape"
    );
    let mut wrong = definition.clone();
    let StrikeSource::Authored { strikes } = &mut wrong.strikes else {
        unreachable!()
    };
    strikes[0].check = None;
    assert!(!super::ray::definition_shape_valid(&wrong));
    let mut wrong = definition.clone();
    let StrikeSource::Authored { strikes } = &mut wrong.strikes else {
        unreachable!()
    };
    let Some(check) = &mut strikes[0].check else {
        unreachable!()
    };
    check.skill = SkillSelector::Named {
        skill: crate::attributes::Skill::Discipline,
    };
    assert!(!super::ray::definition_shape_valid(&wrong));
}

#[test]
fn bounded_polymorph_prepares_off_ray_shape_without_selecting_or_loading() {
    use tor_world::{Direction, Extent, Location, Passage, Position, Region, RegionId, World};
    let (mut game, item, _) = fixture();
    *game.world = World::new(
        (1..=2)
            .map(|id| Region {
                id: RegionId(id),
                name: format!("shape dependency {id}"),
                bounds: Extent::new(4, 3, 1).unwrap(),
            })
            .collect(),
        vec![],
    )
    .unwrap();
    let origin = Location {
        region: RegionId(1),
        position: Position { x: 1, y: 2, z: 0 },
    };
    let target_at = Location {
        position: Position {
            x: 2,
            ..origin.position
        },
        ..origin
    };
    let side = Location {
        region: RegionId(2),
        position: Position { x: 2, y: 0, z: 0 },
    };
    game.world
        .connect(
            Passage {
                from: target_at,
                direction: Direction::South,
                to: side,
            },
            0,
        )
        .unwrap();
    game.actors.get_mut(&ActorId(1)).unwrap().location = origin;
    let target = game
        .spawn_actor(target_at, std::num::NonZeroU64::new(100).unwrap())
        .unwrap();
    let build = game.creature(ActorId(1)).unwrap().build().clone();
    game.configure_creature(
        target,
        crate::CreatureIdentity {
            name: "target".into(),
            faction: "fixture".into(),
        },
        build,
    )
    .unwrap();
    let plan = game
        .compile_device_plan(
            ActorId(1),
            item,
            crate::DeviceAim::Direction {
                direction: crate::abilities::StepDirection::East,
            },
            100,
        )
        .unwrap();
    let mut ray = plan.as_ray().unwrap().clone();
    ray.traversal = RayTraversal::Bounded { clear_visits: 3 };
    let expected = BTreeSet::from([RegionId(1), RegionId(2)]);
    assert_eq!(game.bounded_body_effect_regions(&ray).unwrap(), expected);
    let slice = game.world.detach_region(RegionId(2)).unwrap();
    let before = game.clone();
    assert_eq!(game.bounded_body_effect_regions(&ray).unwrap(), expected);
    assert_eq!(
        game, before,
        "preparation must not roll, mutate, load, or activate"
    );
    game.world.attach_region(slice).unwrap();
    assert_eq!(game.bounded_body_effect_regions(&ray).unwrap(), expected);
    let shape = game.content().polymorph_body_package(&form("two")).unwrap();
    assert!(game.body_fits(target, target_at, 0, &shape.body));
}

#[test]
fn paid_polymorph_identifies_only_immediate_visible_changed_form() {
    let actor = ActorId(1);
    // Independent SplitMix64 witnesses: neutral launch/save each consume 3d6.
    // Seed 1: 9 vs 13 -> saved; seed 2: 12 vs 7, seventh word even -> one;
    // seed 5: 14 vs 13, seventh word odd -> two (whole footprint cannot fit).
    // Equal untrained Lore/Agility bonuses cancel; no runtime seed search.
    for visible in [true, false] {
        for (seed, expected_form, expected_changed) in [
            (1, None, false),
            (2, Some("one"), true),
            (5, Some("two"), false),
        ] {
            let (mut game, item, _) =
                fixture_with_discovery(crate::ItemDiscovery::DiscernibleEffect);
            let build = game.creature(actor).unwrap().build().clone();
            let mut at = game.actors[&actor].location;
            at.position.x += 1;
            let target = game
                .spawn_actor(at, std::num::NonZeroU64::new(100).unwrap())
                .unwrap();
            game.configure_creature(
                target,
                crate::CreatureIdentity {
                    name: "recipient".into(),
                    faction: "fixture".into(),
                },
                build,
            )
            .unwrap();
            game.actors.get_mut(&target).unwrap().ready_at = 100_000;
            game.world.set_region_light(at.region, visible).unwrap();
            let (_, sight) = game.scene_snapshot(actor).unwrap();
            assert_eq!(game.actor_visible_in_sight(actor, target, &sight), visible);
            game.rules_rng = seed;
            let issued = game
                .admit_intention(
                    actor,
                    crate::Action::Zap {
                        item,
                        aim: crate::DeviceAim::Direction {
                            direction: crate::abilities::StepDirection::East,
                        },
                    },
                    crate::IntentionOrigin::Human,
                )
                .unwrap();
            let done = game.execute_next_intention().unwrap();
            assert_eq!(done.intention.id, issued);
            done.outcome.unwrap();
            assert_eq!(
                game.items[&item].charges,
                Some(1),
                "unchanged releases are still paid"
            );
            let resolutions: Vec<_> = game
                .event_log
                .iter()
                .filter_map(|event| match event {
                    crate::combat::CombatEvent::Resolved {
                        outcome, strike, ..
                    } if outcome.intention == Some(issued) => Some((outcome, strike)),
                    _ => None,
                })
                .collect();
            assert_eq!(resolutions.len(), 1, "one paid primary receipt, no replay");
            let (outcome, strike) = resolutions[0];
            assert_eq!(*strike, Some(0));
            assert_eq!((outcome.actor, outcome.target), (actor, target));
            assert_eq!(outcome.ability.as_ref().unwrap().as_str(), "fixture_ray");
            assert_eq!(outcome.check.unwrap().success, expected_form.is_none());
            assert_eq!(outcome.result, outcome.canonical_result());
            let expected_effects = expected_form.map_or_else(Vec::new, |id| {
                vec![ResolvedEffect::BalefulPolymorph {
                    form: Some(form(id)),
                    changed: expected_changed,
                }]
            });
            assert_eq!(
                outcome.effects, expected_effects,
                "receipt must contain the committed form result, not the provisional placeholder"
            );
            let changed = expected_changed;
            assert_eq!(
                game.actors[&actor]
                    .knowledge
                    .contains("fixture.polymorph_wand"),
                visible && changed
            );
            let observation = game.observe(actor).unwrap();
            let interactions = observation.interactions.unwrap();
            let row = interactions
                .inventory
                .iter()
                .find(|row| row.item == item)
                .unwrap();
            assert_eq!(
                row.device_activation,
                Some(if visible && changed {
                    crate::DeviceActivation::DirectionOrSelf
                } else {
                    crate::DeviceActivation::Direction
                })
            );
        }
    }
}

/// Count the complete ordinary paid path, not fixture creation or assertions.
/// These are baseline observations, not budgets or a performance improvement.
#[test]
fn body_compile_work_paid_polymorph_baseline() {
    let actor = ActorId(1);
    // Independently fixed launch/save/selection witnesses, as in the delivery
    // contract above. No seed search or substitute direct effect application.
    for (case, seed, expected_form, changed, draws) in [
        ("paid_resisted", 1_u64, None, false, 6_u64),
        ("paid_changed", 2, Some("one"), true, 7),
        ("paid_blocked_fit", 5, Some("two"), false, 7),
    ] {
        let (mut game, item, base) =
            fixture_with_discovery(crate::ItemDiscovery::DiscernibleEffect);
        let build = game.creature(actor).unwrap().build().clone();
        let mut at = game.actors[&actor].location;
        at.position.x += 1;
        let target = game
            .spawn_actor(at, std::num::NonZeroU64::new(100).unwrap())
            .unwrap();
        game.configure_creature(
            target,
            crate::CreatureIdentity {
                name: "work-count recipient".into(),
                faction: "fixture".into(),
            },
            build.clone(),
        )
        .unwrap();
        game.actors.get_mut(&target).unwrap().ready_at = 100_000;
        crate::test_creatures::injure(&mut game, target, 1).unwrap();
        let personal = build.ledger().entries()[1].clone();
        let tick = game.tick;
        game.rules_rng = seed;

        let before = crate::diagnostics::work_counts();
        let issued = game
            .admit_intention(
                actor,
                crate::Action::Zap {
                    item,
                    aim: crate::DeviceAim::Direction {
                        direction: crate::abilities::StepDirection::East,
                    },
                },
                crate::IntentionOrigin::Human,
            )
            .unwrap();
        let done = game.execute_next_intention().unwrap();
        assert_eq!(done.intention.id, issued);
        done.outcome.unwrap();
        let after = crate::diagnostics::work_counts();

        assert_eq!(game.tick - tick, 100, "ordinary item preparation is paid");
        assert_eq!(game.items[&item].charges, Some(1));
        assert_eq!(
            game.rules_rng,
            seed.wrapping_add(0x9e3779b97f4a7c15_u64.wrapping_mul(draws)),
            "body Health streams and measurement do not consume combat RNG"
        );
        let recipient = game.creature(target).unwrap();
        assert_eq!(recipient.build().ledger().entries()[1], personal);
        assert_eq!(recipient.health().injury(), 1);
        assert!(!recipient.health().dead());
        assert_eq!(
            recipient.build().active_body_package().unwrap().id,
            if changed {
                form("one")
            } else {
                base.id.clone()
            }
        );
        if !changed {
            assert_eq!(recipient.build(), &build);
        }
        let resolutions: Vec<_> = game
            .event_log
            .iter()
            .filter_map(|event| match event {
                crate::combat::CombatEvent::Resolved { outcome, .. }
                    if outcome.intention == Some(issued) =>
                {
                    Some(outcome)
                }
                _ => None,
            })
            .collect();
        assert_eq!(resolutions.len(), 1);
        assert_eq!(
            resolutions[0].effects,
            expected_form.map_or_else(Vec::new, |id| {
                vec![ResolvedEffect::BalefulPolymorph {
                    form: Some(form(id)),
                    changed,
                }]
            })
        );
        assert_eq!(
            game.actors[&actor]
                .knowledge
                .contains("fixture.polymorph_wand"),
            changed,
            "paid nonchanges do not identify this discernible-only device"
        );
        println!(
            "body_work_v1 {}",
            serde_json::json!({
                "case": case, "scope": "paid_admission_through_completion", "initial_hd": 2,
                "permanent_creature_compiles": after.permanent_creature_compiles - before.permanent_creature_compiles,
                "dynamic_creature_folds": after.dynamic_creature_folds - before.dynamic_creature_folds,
                "raw_hit_die_visits": after.raw_hit_die_visits - before.raw_hit_die_visits,
                "advancement_choice_visits": after.advancement_choice_visits - before.advancement_choice_visits,
                "creature_build_copies": after.creature_build_copies - before.creature_build_copies,
                "health_provenance_copies": after.health_provenance_copies - before.health_provenance_copies,
            })
        );
    }
}
