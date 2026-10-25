//! Core paid-effect contracts. The native fixture explicitly supplies trusted
//! structural topology; canonical authored/generated source equality is server-owned.
use super::*;
use crate::spatial_scope::registry::{Budget, CellRun, PrimaryRun, RoomSurfaces, Scope};
use crate::spatial_scope::{Membership, PassageId, PrimarySpace, RoomId};
use crate::{Action, ActorId, Game, IntentionOrigin, ItemDiscovery, ItemId};
use tor_world::{Location, Position, RegionId};

fn at(x: i32) -> Location {
    Location {
        region: RegionId(1),
        position: Position { x, y: 1, z: 0 },
    }
}
fn run(first_x: i32, last_x: i32) -> CellRun {
    CellRun {
        region: RegionId(1),
        z: 0,
        y: 1,
        first_x,
        last_x,
    }
}
fn fixture(passage: bool, lit: bool) -> (Game, ItemId) {
    let mut game = super::ray_tests::fixture_with(|_, ability, species| {
        ability.invocation = InvocationKind::ActorTarget;
        ability.reach = crate::rules::Reach::SelfOnly;
        ability.interaction = crate::rules::Interaction::NonHostile;
        ability.supply_costs.clear();
        let crate::rules::StrikeSource::Authored { strikes } = &mut ability.strikes else {
            unreachable!()
        };
        strikes[0].check = None;
        strikes[0].on_success = vec![crate::rules::Effect::RoomLight];
        strikes[0].on_failure.clear();
        strikes[0].riders.clear();
        species.modifiers.retain(|m|!matches!(&m.op,crate::rules::ModifierOp::GrantAbility{ability:id} if id==&ability.id));
    });
    let room = RoomId::new(1).unwrap();
    let primary = if passage {
        PrimarySpace::Passage(PassageId::new(1).unwrap())
    } else {
        PrimarySpace::Room(room)
    };
    let scope = Scope {
        id: "fixture-room-light".into(),
        members: vec![RegionId(1)],
        primary: vec![PrimaryRun {
            cells: run(1, 4),
            membership: Membership::new(primary, None).unwrap(),
        }],
        rooms: if passage {
            vec![]
        } else {
            vec![RoomSurfaces {
                room,
                runs: vec![run(1, 4)],
            }]
        },
    };
    let budget = Budget {
        scopes: 1,
        members: 1,
        rooms: 1,
        passages: 1,
        runs: 2,
        label_bytes: 64,
        expanded_refs: 8,
    };
    scope.validate(&budget).unwrap();
    for x in 1..=5 {
        assert!(game.world.contains(at(x)));
    }
    game.register_spatial_scope(scope, &budget).unwrap();
    assert!(game
        .spatial_registry()
        .lookup("fixture-room-light", at(1))
        .is_some());
    game.world.set_region_light(RegionId(1), lit).unwrap();
    let mut spec = crate::ItemSpec::ordinary("plain light stick".into());
    spec.identity = "fixture.light".into();
    spec.concealed = true;
    spec.class = crate::ItemClass::Tool;
    spec.device = Some(crate::DeviceSpec {
        ability: crate::content_ids::AbilityId::new("fixture_ray").unwrap(),
        initial_charges: crate::ChargeRange {
            minimum: 2,
            maximum: 2,
        },
        discovery: ItemDiscovery::OnUse,
    });
    let item = game
        .place_item_stack(10, at(1), Some(ActorId(1)), 1, spec)
        .unwrap();
    assert_eq!(game.items[&item].charges, Some(2));
    (game, item)
}
fn admit(game: &mut Game, item: ItemId) -> crate::IntentionId {
    game.admit_intention(
        ActorId(1),
        Action::Zap {
            item,
            aim: crate::DeviceAim::Direction {
                direction: StepDirection::West,
            },
        },
        IntentionOrigin::Human,
    )
    .unwrap()
}
fn complete(game: &mut Game, item: ItemId) {
    let tick = game.tick;
    let rng = game.rules_rng;
    let id = admit(game, item);
    let done = game.execute_next_intention().unwrap();
    assert_eq!(done.intention.id, id);
    done.outcome.unwrap();
    assert_eq!(game.tick, tick + 100);
    assert_eq!(game.items[&item].charges, Some(1));
    assert_eq!(game.rules_rng, rng);
    assert!(game.actors[&ActorId(1)].knowledge.contains("fixture.light"));
}
#[test]
fn room_light_paid_device_lights_exact_room_surfaces_without_rng_or_targeting() {
    let (mut game, item) = fixture(false, false);
    game.set_combat_diagnostics(true);
    assert_eq!(game.world.is_lit(at(4)), Some(false));
    complete(&mut game, item);
    assert_eq!(game.world.is_lit(at(4)), Some(true));
    assert_eq!(
        game.world.is_lit(at(5)),
        Some(false),
        "unlisted cell is not room illumination"
    );
    // Self-device payloads resolve as Exposure, not a generic ability combat
    // diagnostic. The authoritative resolution event still records actual change.
    assert!(game.combat_diagnostics().unwrap().records().is_empty());
    let canonical = game
        .combat_events()
        .iter()
        .find_map(|event| match event {
            crate::combat::CombatEvent::Resolved { outcome, .. }
                if outcome
                    .effects
                    .iter()
                    .any(|effect| matches!(effect, ResolvedEffect::RoomLight { .. })) =>
            {
                Some(outcome)
            }
            _ => None,
        })
        .unwrap();
    assert_eq!(
        canonical.effects,
        vec![ResolvedEffect::RoomLight { changed: true }]
    );
    game.set_combat_diagnostics(false);
    let mut shared = crate::checkpoint::SharedState::default();
    let snapshot = game.checkpoint(&mut shared);
    let shared = serde_json::from_slice(&serde_json::to_vec(&shared).unwrap()).unwrap();
    let snapshot = serde_json::from_slice(&serde_json::to_vec(&snapshot).unwrap()).unwrap();
    let reopened = Game::restore_checkpoint(snapshot, &shared, game.content().clone()).unwrap();
    assert_eq!(game, reopened);
}
#[test]
fn room_light_passage_and_already_lit_room_still_pay_and_identify() {
    for (passage, lit) in [(true, false), (false, true)] {
        let (mut game, item) = fixture(passage, lit);
        let world = game.world.clone();
        complete(&mut game, item);
        assert_eq!(
            game.world, world,
            "no terrain or illumination change on paid no-op"
        );
    }
}
#[test]
fn room_light_closed_effect_shapes_and_nonhostile_self_source() {
    let source: crate::rules::Effect = serde_json::from_str(r#"{"kind":"room_light"}"#).unwrap();
    assert_eq!(source, crate::rules::Effect::RoomLight);
    assert_eq!(source.recipient(), crate::rules::EffectRecipient::Source);
    assert!(
        serde_json::from_str::<crate::rules::Effect>(r#"{"kind":"room_light","room":1}"#).is_err()
    );
    assert!(serde_json::from_str::<ResolvedEffect>(r#"{"kind":"room_light"}"#).is_err());
    assert_eq!(
        serde_json::from_str::<ResolvedEffect>(r#"{"kind":"room_light","changed":false}"#).unwrap(),
        ResolvedEffect::RoomLight { changed: false }
    );
    let (game, _) = fixture(false, false);
    let valid = game.content.rules.document().clone();
    crate::rules::Ruleset::validate(valid.clone()).unwrap();
    for hostile in [false, true] {
        let mut invalid = valid.clone();
        let ability = invalid
            .abilities
            .iter_mut()
            .find(|a| a.id.as_str() == "fixture_ray")
            .unwrap();
        if hostile {
            ability.interaction = crate::rules::Interaction::HostileToTarget;
        } else {
            ability.reach = crate::rules::Reach::Melee;
        }
        assert!(crate::rules::Ruleset::validate(invalid).is_err());
    }
}

#[test]
fn room_light_rejects_check_bearing_source_before_item_admission() {
    let (game, _) = fixture(false, false);
    let mut invalid = game.content.rules.document().clone();
    let ability = invalid
        .abilities
        .iter_mut()
        .find(|a| a.id.as_str() == "fixture_ray")
        .unwrap();
    let crate::rules::StrikeSource::Authored { strikes } = &mut ability.strikes else {
        unreachable!()
    };
    strikes[0].check = Some(crate::rules::CheckDef {
        actor: crate::rules::CheckActor::User,
        skill: crate::rules::SkillSelector::Named {
            skill: crate::attributes::Skill::Lore,
        },
        against: crate::rules::Against::Opposed {
            skill: crate::rules::SkillSelector::Named {
                skill: crate::attributes::Skill::Lore,
            },
        },
    });
    assert!(crate::rules::Ruleset::validate(invalid).is_err());
}

#[test]
fn room_light_complete_surface_validation_precedes_any_world_mutation() {
    let (mut game, _) = fixture(false, false);
    let mut scope = game.spatial_registry().scopes().next().unwrap().clone();
    scope.rooms[0].runs.push(run(99, 99));
    assert!(!game.world.contains(at(99)));
    let budget = Budget {
        scopes: 1,
        members: 1,
        rooms: 1,
        passages: 0,
        runs: 3,
        label_bytes: 64,
        expanded_refs: 9,
    };
    scope.validate(&budget).unwrap();
    // Trusted native setup can supply structurally valid but physically absent
    // cells. Runtime must still reject atomically; this is not canonical source acceptance.
    game.spatial_registry = tor_world::Shared::new(Default::default());
    game.register_spatial_scope(scope, &budget).unwrap();
    let before = game.clone();
    assert!(game
        .apply_effects(ActorId(1), &[crate::rules::Effect::RoomLight])
        .is_err());
    assert_eq!(game, before);
    assert_eq!(game.world.is_lit(at(1)), Some(false));
}

#[test]
fn room_light_empty_device_needs_no_spatial_payload_or_loaded_room() {
    let (mut game, item) = fixture(false, false);
    game.items
        .edit(item, |item| item.charges = Some(0))
        .unwrap();
    game.spatial_registry = tor_world::Shared::new(Default::default());
    let rng = game.rules_rng;
    let tick = game.tick;
    let world = game.world.clone();
    let id = admit(&mut game, item);
    let done = game.execute_next_intention().unwrap();
    assert_eq!(done.intention.id, id);
    done.outcome.unwrap();
    assert_eq!(game.tick, tick + 100);
    assert_eq!(game.items[&item].charges, Some(0));
    assert_eq!(game.rules_rng, rng);
    assert_eq!(game.world, world);
    assert!(!game.actors[&ActorId(1)].knowledge.contains("fixture.light"));
}

#[test]
fn room_light_is_not_a_natural_melee_payload() {
    let (game, _) = fixture(false, false);
    let species = game.content.catalog().species.values().next().unwrap();
    let natural = &species.melee;
    let create = |strikes| {
        crate::attacks::NaturalAttack::new(
            natural.skill(),
            natural.bonus(),
            natural.wind_up(),
            natural.recovery(),
            strikes,
        )
    };
    create(natural.strikes().to_vec()).unwrap();
    for branch in 0..3 {
        let mut strikes = natural.strikes().to_vec();
        let strike = &mut strikes[0];
        strike.on_success.clear();
        strike.on_failure.clear();
        strike.riders.clear();
        match branch {
            0 => strike.on_success.push(crate::rules::Effect::RoomLight),
            1 => strike.on_failure.push(crate::rules::Effect::RoomLight),
            _ => strike.riders.push(crate::rules::RiderDef {
                cancellable: false,
                trigger: crate::rules::RiderTrigger::Success,
                condition: None,
                check: None,
                effects: vec![crate::rules::Effect::RoomLight],
                descriptors: vec![],
            }),
        }
        crate::rules::validate_strike_shapes(&strikes).unwrap();
        assert!(create(strikes).is_err());
    }
}

#[test]
fn room_light_direct_and_exposure_returns_use_actual_publication_not_pure_marker() {
    for exposure in [false, true] {
        let (mut game, item) = fixture(false, false);
        assert_eq!(game.world.is_lit(at(4)), Some(false));
        let tick = game.tick;
        let rng = game.rules_rng;
        let effects = game
            .effect_plan(ActorId(1), &[crate::rules::Effect::RoomLight])
            .unwrap();
        let plan = if exposure {
            ResolutionPlan::Exposure {
                actor: ActorId(1),
                target: ActorId(1),
                effects: &effects,
                descriptors: &[],
            }
        } else {
            ResolutionPlan::Effects {
                actor: ActorId(1),
                target: ActorId(1),
                effects: &effects,
            }
        };
        let creature = game.creature(ActorId(1)).unwrap();
        let snapshot = Snapshot::capture(&creature).with_context(
            game.resolution_context(plan, ActorId(1), ActorId(1))
                .unwrap(),
        );
        let mut pure_rng = rng;
        let pure = super::resolve::resolve(plan, &snapshot, &mut pure_rng, &mut ()).unwrap();
        assert_eq!(
            pure.effects,
            vec![ResolvedEffect::RoomLight { changed: false }]
        );
        let old_return_oracle = pure.result == OutcomeStatus::Applied;
        assert!(!old_return_oracle);
        assert_eq!(pure_rng, rng);
        let apply = |game: &mut Game| {
            if exposure {
                game.with_effect_publication(|candidate| {
                    candidate.apply_exposure_in_transaction(
                        ActorId(1),
                        &[crate::rules::Effect::RoomLight],
                        &[],
                    )
                })
            } else {
                game.apply_effects(ActorId(1), &[crate::rules::Effect::RoomLight])
            }
        };
        let actual = apply(&mut game).unwrap();
        // Explicit sensitivity to the old prepublication-return expression;
        // this is not a claim that a separate historical runtime red was run.
        assert!(actual);
        assert_ne!(actual, old_return_oracle);
        assert_eq!(game.world.is_lit(at(4)), Some(true));
        assert_eq!(game.world.is_lit(at(5)), Some(false));
        assert!(!apply(&mut game).unwrap());
        assert_eq!(game.tick, tick);
        assert_eq!(game.rules_rng, rng);
        assert_eq!(game.items[&item].charges, Some(2));
        assert!(!game.actors[&ActorId(1)].knowledge.contains("fixture.light"));
        let changed = game
            .combat_events()
            .iter()
            .filter_map(|event| match event {
                crate::combat::CombatEvent::Resolved { outcome, .. } => {
                    outcome.effects.iter().find_map(|effect| match effect {
                        ResolvedEffect::RoomLight { changed } => Some(*changed),
                        _ => None,
                    })
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(changed, vec![true, false]);
    }
}
