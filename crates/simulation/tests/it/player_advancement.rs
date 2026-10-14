//! Typed ordinary purchase authority reuses the permanent build kernel.
use super::creature_fixture as fixture;
use serde_json::{json, Value};
use std::{collections::BTreeSet, num::NonZeroU64};
use tor_simulation::{
    attributes::Skill,
    content_ids::PointPoolId,
    creatures::{record::CreatureRecord, BuildError, PurchaseSelection},
    progression::HdSource,
    Action, ActorId, CreatureIdentity, Game, GameError, IntentionOrigin,
};
use tor_world::{Location, Position, RegionId};

fn game() -> Game {
    game_with_build(fixture::build(fixture::species()))
}
fn game_with_build(build: tor_simulation::creatures::CreatureBuild) -> Game {
    let mut game = Game::two_room_in_stone(42, build.content().clone());
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
                name: format!("owner {x}"),
                faction: "neutral".into(),
            },
            build.clone(),
        )
        .unwrap();
    }
    game.configure_run(
        ActorId(1),
        BTreeSet::from([ActorId(1)]),
        None,
        Default::default(),
    )
    .unwrap();
    game
}
fn request(game: &Game, actor: ActorId) -> Value {
    json!({"type":"purchase", "value": {
        "basis":{"actor":actor.0,"branch":"original-branch","stream":"original-stream","reset":7,"build_epoch":game.creature(actor).unwrap().build_epoch().get()},
        "purchase":{"owner":0,"pool":"advancement","selection":{"kind":"skill_rank","skill":"perception"}}
    }})
}
fn action(request: Value) -> Action {
    serde_json::from_value(request).expect("ordinary typed purchase must decode")
}
fn recorded(game: &Game, actor: ActorId) -> Value {
    serde_json::to_value(CreatureRecord::capture(&game.creature(actor).unwrap())).unwrap()
}
fn round_trip(game: &Game) -> Game {
    let mut shared = tor_simulation::checkpoint::SharedState::default();
    let snapshot = game.checkpoint(&mut shared);
    let bytes = serde_json::to_vec(&(snapshot, shared)).unwrap();
    let (snapshot, shared) = serde_json::from_slice(&bytes).unwrap();
    Game::restore_checkpoint(snapshot, &shared, game.content().clone()).unwrap()
}

#[test]
fn phase5_purchase_preview_is_pure_and_commit_matches_its_basis() {
    let mut game = game();
    let actor = ActorId(1);
    let original = game.clone();
    let permanent = game
        .creature(actor)
        .unwrap()
        .build()
        .preview_purchase(
            0,
            PointPoolId::new("advancement").unwrap(),
            PurchaseSelection::SkillRank(Skill::Perception),
        )
        .unwrap();
    let proposal = action(request(&game, actor));
    let Action::Purchase { basis, purchase } = &proposal else {
        unreachable!()
    };
    let preview = game
        .preview_player_purchase(actor, basis, purchase)
        .unwrap();
    assert_eq!(preview.permanent(), &permanent);
    assert_eq!(game, original);
    let result = game.act(actor, proposal.clone()).unwrap();
    assert_eq!(game.creature(actor).unwrap().build(), preview.build());
    assert_eq!(game.creature(actor).unwrap().build_epoch().get(), 2);
    assert_eq!(result.at_tick, 0);
    assert_eq!(
        result.next_tick, 0,
        "the candidate purchase recovery is explicitly zero"
    );
    assert_eq!(game.creature(actor).unwrap().build().unspent_points(), 0);
    let after = game.clone();
    assert_eq!(
        game.act(actor, proposal),
        Err(GameError::Build(BuildError::StaleAdvancementPreview))
    );
    assert_eq!(game, after, "a stale direct action cannot spend twice");
}

#[test]
fn phase5_queued_purchase_rechecks_build_epoch_before_spending() {
    let mut game = game();
    let actor = ActorId(1);
    let proposal = action(request(&game, actor));
    let original = game.creature(actor).unwrap().build().clone();
    game.admit_intention(actor, proposal.clone(), IntentionOrigin::Human)
        .unwrap();
    assert_eq!(game.creature(actor).unwrap().build(), &original);
    assert_eq!(
        game.pending_intention(actor).unwrap().work,
        tor_simulation::IntentionWork::Action(proposal)
    );
    let mut changed = original;
    changed.add_hit_die(HdSource::Racial, 42).unwrap();
    game.rebuild_creature(actor, changed).unwrap();
    let before = game.creature(actor).unwrap().into_owned();
    let mut replay = round_trip(&game);
    for state in [&mut game, &mut replay] {
        let execution = state.execute_next_intention().unwrap();
        assert_eq!(
            execution.outcome,
            Err(GameError::Build(BuildError::StaleAdvancementPreview))
        );
        assert_eq!(state.creature(actor).unwrap().as_ref(), &before);
        assert!(state.pending_intention(actor).is_none());
        assert_eq!(state.tick(), 0);
    }
    assert_eq!(game, replay);
}

#[test]
fn phase5_purchase_rejects_foreign_basis_and_value_equivalent_old_epoch_atomically() {
    for foreign in [false, true] {
        let mut game = game();
        let actor = ActorId(1);
        let mut proposal = request(&game, actor);
        if foreign {
            proposal["value"]["basis"]["actor"] = json!(2);
        } else {
            let original = game.creature(actor).unwrap().build().clone();
            let mut changed = original.clone();
            changed.add_hit_die(HdSource::Racial, 42).unwrap();
            game.rebuild_creature(actor, changed).unwrap();
            game.rebuild_creature(actor, original.clone()).unwrap();
            assert_eq!(game.creature(actor).unwrap().build(), &original);
            assert_eq!(game.creature(actor).unwrap().build_epoch().get(), 3);
        }
        let before = game.clone();
        assert_eq!(
            game.act(actor, action(proposal)),
            Err(GameError::Build(BuildError::StaleAdvancementPreview))
        );
        assert_eq!(game, before);
    }
}

#[test]
fn phase5_purchase_admission_requires_ready_actor() {
    let mut game = game();
    let actor = ActorId(1);
    game.act(actor, Action::Wait).unwrap();
    assert_eq!(game.tick(), 0);
    assert_eq!(game.next_actor(), Some(ActorId(2)));
    let proposal = action(request(&game, actor));
    let before = game.clone();
    assert_eq!(
        game.admit_intention(actor, proposal, IntentionOrigin::Human),
        Err(GameError::ActorBusy)
    );
    assert_eq!(game, before);
}

#[test]
fn phase5_purchase_rejects_active_and_paused_preparations_without_abandoning_them() {
    for paused in [false, true] {
        let mut game = game();
        let actor = ActorId(1);
        let attack = Action::UseAbility {
            id: game.basic_attack(actor).unwrap(),
            invocation: tor_simulation::abilities::AbilityInvocation::ActorTarget {
                target: ActorId(2),
            },
            supplies: Default::default(),
        };
        let intention = game
            .admit_intention(actor, attack, IntentionOrigin::Human)
            .unwrap();
        assert!(game.execute_next_intention().unwrap().outcome.is_ok());
        assert!(game.preparation(actor).is_some());
        if paused {
            game.suspend_intention(actor, intention).unwrap();
        }
        let proposal = action(request(&game, actor));
        let before = game.clone();
        assert_eq!(
            game.admit_intention(actor, proposal, IntentionOrigin::Human),
            Err(GameError::ActorBusy)
        );
        assert_eq!(game, before);
    }
}

#[test]
fn phase5_purchase_does_not_regrant_starting_ranks_or_reset_runtime_state() {
    let original = game();
    let actor = ActorId(1);
    let mut shared = tor_simulation::checkpoint::SharedState::default();
    let snapshot = original.checkpoint(&mut shared);
    let mut value = serde_json::to_value((snapshot, shared)).unwrap();
    let table = value[0]["actors"].as_u64().unwrap() as usize;
    let state = &mut value[1]["actors"][table]["1"]["combat"]["creature"];
    state["injury"] = json!(2);
    state["received_health_loss_remaining"] = json!(600);
    state["cooldowns"] = json!([{"ability":"basic_melee","remaining":100}]);
    let stamina = state["pools"]["stamina"]["balance"].as_u64().unwrap();
    assert!(stamina > 0);
    state["pools"]["stamina"]["balance"] = json!(stamina - 1);
    let (snapshot, shared) = serde_json::from_value(value).unwrap();
    let mut game = Game::restore_checkpoint(snapshot, &shared, original.content().clone()).unwrap();
    let mut before = recorded(&game, actor);
    let starting = game
        .creature(actor)
        .unwrap()
        .build()
        .starting_skills()
        .to_owned();
    game.act(actor, action(request(&game, actor))).unwrap();
    let mut after = recorded(&game, actor);
    assert_eq!(before["earned_progress"]["peak_threat"], json!(1));
    assert_eq!(
        after["earned_progress"]["peak_threat"],
        json!(2),
        "the committed permanent purchase raises recorded peak threat"
    );
    before["earned_progress"]["peak_threat"] = json!(2);
    for key in ["build", "build_epoch"] {
        before.as_object_mut().unwrap().remove(key);
        after.as_object_mut().unwrap().remove(key);
    }
    assert_eq!(after, before);
    assert_eq!(
        game.creature(actor).unwrap().build().starting_skills(),
        &starting
    );
    assert_eq!(round_trip(&game), game);
}

#[test]
fn phase5_purchase_action_carries_exact_typed_basis_and_rejects_invented_authority() {
    let game = game();
    let original = request(&game, ActorId(1));
    assert_eq!(
        serde_json::to_value(action(original.clone())).unwrap(),
        original
    );
    for field in ["basis", "purchase"] {
        let mut value = original.clone();
        value["value"].as_object_mut().unwrap().remove(field);
        assert!(serde_json::from_value::<Action>(value).is_err());
    }
    for field in ["actor", "branch", "stream", "reset", "build_epoch"] {
        let mut value = original.clone();
        value["value"]["basis"]
            .as_object_mut()
            .unwrap()
            .remove(field);
        assert!(serde_json::from_value::<Action>(value).is_err());
    }
    for (field, invalid) in [
        ("actor", json!(0)),
        ("build_epoch", json!(0)),
        ("branch", json!("")),
        ("stream", json!("bad\nstream")),
        ("branch", json!("x".repeat(129))),
        ("reset", json!(-1)),
    ] {
        let mut value = original.clone();
        value["value"]["basis"][field] = invalid;
        assert!(serde_json::from_value::<Action>(value).is_err());
    }
    for field in ["candidate", "refund", "points"] {
        let mut value = original.clone();
        value["value"]["purchase"][field] = json!(999);
        assert!(serde_json::from_value::<Action>(value).is_err());
    }
    for field in ["basis", "purchase"] {
        let mut value = original.clone();
        value["value"][field] = json!([]);
        assert!(serde_json::from_value::<Action>(value).is_err());
    }
    let mut invalid_owner = original.clone();
    invalid_owner["value"]["purchase"]["owner"] = json!(256);
    assert!(serde_json::from_value::<Action>(invalid_owner).is_err());
    let mut invalid_selection = original.clone();
    invalid_selection["value"]["purchase"]["selection"]["refund"] = json!(1);
    assert!(serde_json::from_value::<Action>(invalid_selection).is_err());
    let duplicate = serde_json::to_string(&original)
        .unwrap()
        .replace("\"reset\":7", "\"reset\":7,\"reset\":7");
    assert!(serde_json::from_str::<Action>(&duplicate).is_err());
}

#[test]
fn phase5_purchase_admission_rejects_frozen_actor_without_spending() {
    let mut game = game();
    game.add_region(tor_world::Region {
        id: RegionId(3),
        name: "isolated room".into(),
        bounds: tor_world::Extent::new(8, 3, 1).unwrap(),
    })
    .unwrap();
    let actor = game
        .spawn_actor(
            Location {
                region: RegionId(3),
                position: Position { x: 1, y: 1, z: 0 },
            },
            NonZeroU64::new(100).unwrap(),
        )
        .unwrap();
    let build = game.creature(ActorId(1)).unwrap().build().clone();
    game.configure_creature(
        actor,
        CreatureIdentity {
            name: "frozen owner".into(),
            faction: "neutral".into(),
        },
        build,
    )
    .unwrap();
    let proposal = action(request(&game, actor));
    let (applied, _) = game
        .transition_regions(
            &tor_simulation::RegionTransition {
                active: BTreeSet::from([RegionId(1)]),
                loaded: BTreeSet::from([RegionId(1), RegionId(2), RegionId(3)]),
            },
            &mut tor_simulation::MemoryRecords::default(),
        )
        .unwrap();
    assert!(
        !applied.active.contains(&RegionId(3)),
        "fixture must really freeze the unpinned actor"
    );
    assert!(applied.loaded.contains(&RegionId(3)));
    assert_eq!(
        game.region_state(RegionId(3)),
        Some(tor_simulation::RegionState::Frozen)
    );
    assert_eq!(game.known_actor_region(actor), Some(RegionId(3)));
    let before = game.clone();
    assert_eq!(
        game.admit_intention(actor, proposal.clone(), IntentionOrigin::Human),
        Err(GameError::ActorBusy)
    );
    assert_eq!(game, before);
    game.transition_regions(
        &tor_simulation::RegionTransition {
            active: BTreeSet::from([RegionId(1), RegionId(3)]),
            loaded: BTreeSet::from([RegionId(1), RegionId(2), RegionId(3)]),
        },
        &mut tor_simulation::MemoryRecords::default(),
    )
    .unwrap();
    assert_eq!(
        game.region_state(RegionId(3)),
        Some(tor_simulation::RegionState::Active)
    );
    game.admit_intention(actor, proposal, IntentionOrigin::Human)
        .expect("the exact proposal is valid once its owner is thawed");
}

#[test]
fn phase5_purchase_epoch_exhaustion_rejects_preview_and_commit_without_mutation() {
    let original = game();
    let actor = ActorId(1);
    let mut shared = tor_simulation::checkpoint::SharedState::default();
    let snapshot = original.checkpoint(&mut shared);
    let mut value = serde_json::to_value((snapshot, shared)).unwrap();
    let table = value[0]["actors"].as_u64().unwrap() as usize;
    value[1]["actors"][table]["1"]["combat"]["creature"]["build_epoch"] = json!(u64::MAX);
    let (snapshot, shared) = serde_json::from_value(value).unwrap();
    let mut game = Game::restore_checkpoint(snapshot, &shared, original.content().clone()).unwrap();
    let before = game.clone();
    let proposal = action(request(&game, actor));
    let Action::Purchase { basis, purchase } = &proposal else {
        unreachable!()
    };
    assert_eq!(
        game.preview_player_purchase(actor, basis, purchase),
        Err(GameError::Creature(
            tor_simulation::creatures::CreatureStateError::BuildEpochExhausted
        ))
    );
    assert_eq!(
        game.act(actor, proposal),
        Err(GameError::Creature(
            tor_simulation::creatures::CreatureStateError::BuildEpochExhausted
        ))
    );
    assert_eq!(game, before);
}

fn authored_build(
    document: tor_simulation::rules::RulesetDocument,
    hd: usize,
) -> tor_simulation::creatures::CreatureBuild {
    use std::collections::BTreeMap;
    use tor_simulation::creatures::{Content, CreatureBuild, ScenarioCatalog};
    let species = fixture::species();
    let id = species.id.clone();
    let content = Content::new(
        tor_simulation::rules::Ruleset::validate(document).unwrap(),
        ScenarioCatalog {
            species: BTreeMap::from([(id.clone(), species)]),
            templates: BTreeMap::new(),
        },
    )
    .unwrap();
    CreatureBuild::new(
        content,
        id,
        tor_simulation::progression::HdLedger::seeded(vec![HdSource::Racial; hd], 42).unwrap(),
        tor_simulation::attributes::ManaBinding::Intellect,
        [Skill::HeavyWeaponry, Skill::Spellcasting],
    )
    .unwrap()
}

#[test]
fn phase5_purchase_uses_exact_owner_named_pool_and_choice_without_borrowing() {
    use tor_simulation::rules::{PointGrant, PointPoolDef, PurchaseKind};
    let pool = |id| PointPoolId::new(id).unwrap();
    let mut document = fixture::ruleset().document().clone();
    document.talents.push(fixture::talent(
        "fixture_health",
        vec![fixture::add(tor_simulation::rules::Stat::Health, 2)],
    ));
    document.talents.push(fixture::talent(
        "fixture_capacity",
        vec![fixture::add(
            tor_simulation::rules::Stat::ResourceMax {
                resource: tor_simulation::resources::Resource::Stamina,
            },
            2,
        )],
    ));
    document.point_pools = vec![
        PointPoolDef {
            id: pool("skills"),
            name: "Skills".into(),
            allowed: vec![PurchaseKind::SkillRank],
        },
        PointPoolDef {
            id: pool("talents"),
            name: "Talents".into(),
            allowed: vec![PurchaseKind::Talent],
        },
    ];
    let grants = vec![
        PointGrant {
            pool: pool("skills"),
            points: 2,
        },
        PointGrant {
            pool: pool("talents"),
            points: 1,
        },
    ];
    document.constants.racial_point_grants = grants.clone();
    for class in &mut document.classes {
        class.point_grants = grants.clone();
    }
    let mut game = game_with_build(authored_build(document, 2));
    let actor = ActorId(1);
    for (owner, pool_id, selection) in [
        (
            1,
            "talents",
            json!({"kind":"talent","talent":"fixture_health"}),
        ),
        (
            0,
            "skills",
            json!({"kind":"skill_rank","skill":"perception"}),
        ),
        (
            1,
            "skills",
            json!({"kind":"skill_rank","skill":"heavy_weaponry"}),
        ),
    ] {
        let mut value = request(&game, actor);
        value["value"]["purchase"] = json!({"owner":owner,"pool":pool_id,"selection":selection});
        let proposal = action(value);
        let Action::Purchase { basis, purchase } = &proposal else {
            unreachable!()
        };
        let original = game.clone();
        let before = game.creature(actor).unwrap().build().clone();
        let preview = game
            .preview_player_purchase(actor, basis, purchase)
            .unwrap();
        assert_eq!(game, original);
        game.act(actor, proposal).unwrap();
        let after = game.creature(actor).unwrap().build().clone();
        assert_eq!(&after, preview.build());
        for ordinal in 0..2 {
            for (old, new) in before
                .point_balances(ordinal)
                .unwrap()
                .into_iter()
                .zip(after.point_balances(ordinal).unwrap())
            {
                let spent = u16::from(ordinal == owner && old.pool.as_str() == pool_id);
                assert_eq!(new.spent, old.spent + spent);
                assert_eq!(new.granted, old.granted);
            }
        }
        assert_eq!(after.ledger(), before.ledger());
        assert_eq!(after.starting_skills(), before.starting_skills());
        assert_eq!(round_trip(&game), game);
    }
    for (owner, pool_id, selection, error) in [
        (
            0,
            "skills",
            json!({"kind":"talent","talent":"fixture_capacity"}),
            BuildError::PurchaseNotAllowed,
        ),
        (
            2,
            "skills",
            json!({"kind":"skill_rank","skill":"perception"}),
            BuildError::InvalidOwnership,
        ),
        (
            0,
            "missing",
            json!({"kind":"skill_rank","skill":"perception"}),
            BuildError::UnknownPointPool,
        ),
    ] {
        let mut value = request(&game, actor);
        value["value"]["purchase"] = json!({"owner":owner,"pool":pool_id,"selection":selection});
        let before = game.clone();
        assert_eq!(
            game.admit_intention(actor, action(value), IntentionOrigin::Human),
            Err(GameError::Build(error))
        );
        assert_eq!(game, before);
    }
}

#[test]
fn phase5_purchase_reuses_permanent_eligibility_rank_cap_and_duplicate_checks() {
    use tor_simulation::{
        content_ids::TalentId,
        rules::{Requirement, TalentDef},
    };
    let mut document = fixture::ruleset().document().clone();
    document.talents.push(fixture::talent(
        "fixture_health",
        vec![fixture::add(tor_simulation::rules::Stat::Health, 2)],
    ));
    document.talents.push(TalentDef {
        form_choices: vec![],
        id: TalentId::new("later_choice").unwrap(),
        name: "Later choice".into(),
        prerequisites: vec![],
        requirement: Requirement::TotalHd { minimum: 10 },
        modifiers: vec![],
    });
    let mut build = authored_build(document, 6);
    for owner in 0..4 {
        build.train(owner, Skill::HeavyWeaponry).unwrap();
    }
    build
        .select_talent(4, TalentId::new("fixture_health").unwrap())
        .unwrap();
    let mut game = game_with_build(build);
    let actor = ActorId(1);
    for (selection, error) in [
        (
            json!({"kind":"skill_rank","skill":"heavy_weaponry"}),
            BuildError::RankCap,
        ),
        (
            json!({"kind":"talent","talent":"fixture_health"}),
            BuildError::DuplicateTalent,
        ),
        (
            json!({"kind":"talent","talent":"later_choice"}),
            BuildError::IneligibleTalent,
        ),
    ] {
        let mut value = request(&game, actor);
        value["value"]["purchase"]["owner"] = json!(5);
        value["value"]["purchase"]["selection"] = selection;
        let proposal = action(value);
        let Action::Purchase { basis, purchase } = &proposal else {
            unreachable!()
        };
        let before = game.clone();
        assert_eq!(
            game.preview_player_purchase(actor, basis, purchase),
            Err(GameError::Build(error))
        );
        assert_eq!(
            game.admit_intention(actor, proposal, IntentionOrigin::Human),
            Err(GameError::Build(error))
        );
        assert_eq!(game, before);
    }
}

fn at_tick(game: &Game, tick: u64) -> Game {
    let mut shared = tor_simulation::checkpoint::SharedState::default();
    let mut snapshot = serde_json::to_value(game.checkpoint(&mut shared)).unwrap();
    let mut pool = serde_json::to_value(shared).unwrap();
    assert_eq!(game.tick(), 0);
    snapshot["tick"] = json!(tick);
    for table in pool["actors"].as_array_mut().unwrap() {
        for actor in table.as_object_mut().unwrap().values_mut() {
            assert_eq!(actor["ready_at"], json!(0));
            actor["ready_at"] = json!(tick);
            actor["active_since"] = json!(tick);
        }
    }
    let shared = serde_json::from_value(pool).unwrap();
    Game::restore_checkpoint(
        serde_json::from_value(snapshot).unwrap(),
        &shared,
        game.content().clone(),
    )
    .unwrap()
}

#[test]
fn phase5_purchase_recovery_is_authored_and_overflow_rejects_before_spending() {
    let mut document = fixture::ruleset().document().clone();
    document.advancement.purchase_recovery = 23;
    let original = game_with_build(authored_build(document, 2));
    let mut game = original.clone();
    let actor = ActorId(1);
    game.act(actor, action(request(&game, actor))).unwrap();
    let mut second = request(&game, actor);
    second["value"]["purchase"]["owner"] = json!(1);
    let before = game.clone();
    assert_eq!(
        game.admit_intention(actor, action(second.clone()), IntentionOrigin::Human),
        Err(GameError::ActorBusy)
    );
    assert_eq!(game, before);
    game.act(ActorId(2), Action::Wait).unwrap();
    assert_eq!(game.tick(), 23);
    game.admit_intention(actor, action(second), IntentionOrigin::Human)
        .unwrap();

    let mut overflow = at_tick(&original, u64::MAX - 22);
    let proposal = action(request(&overflow, actor));
    let before = overflow.clone();
    let error = GameError::Schedule(tor_simulation::ScheduleError::TimeExhausted);
    assert_eq!(
        overflow.admit_intention(actor, proposal.clone(), IntentionOrigin::Human),
        Err(error)
    );
    assert_eq!(overflow.act(actor, proposal), Err(error));
    assert_eq!(overflow, before);
    let mut exact = at_tick(&original, u64::MAX - 23);
    exact.act(actor, action(request(&exact, actor))).unwrap();
    assert_eq!(exact.creature(actor).unwrap().build_epoch().get(), 2);
}

#[test]
fn phase5_purchase_projects_runtime_read_only_then_settles_once_at_commit() {
    let mut game = game();
    let actor = ActorId(1);
    fixture::injure(&mut game, actor, 2).unwrap();
    game.act(actor, Action::Wait).unwrap();
    game.act(ActorId(2), Action::Wait).unwrap();
    assert_eq!(game.tick(), 100);
    let mut expected = recorded(&game, actor);
    assert_eq!(expected["received_health_loss_remaining"], json!(500));
    let proposal = action(request(&game, actor));
    let Action::Purchase { basis, purchase } = &proposal else {
        unreachable!()
    };
    let original = game.clone();
    game.preview_player_purchase(actor, basis, purchase)
        .unwrap();
    assert_eq!(
        game, original,
        "preview cannot settle the lazy actor, observe or advance anything"
    );
    game.act(actor, proposal).unwrap();
    let mut actual = recorded(&game, actor);
    assert_eq!(expected["earned_progress"]["peak_threat"], json!(1));
    assert_eq!(
        actual["earned_progress"]["peak_threat"],
        json!(2),
        "the committed permanent purchase raises recorded peak threat"
    );
    expected["earned_progress"]["peak_threat"] = json!(2);
    for key in ["build", "build_epoch"] {
        expected.as_object_mut().unwrap().remove(key);
        actual.as_object_mut().unwrap().remove(key);
    }
    assert_eq!(
        actual, expected,
        "the ordinary boundary retains exactly the projected current runtime"
    );
    assert_eq!(round_trip(&game), game);
}

#[test]
fn phase5_purchase_fills_only_current_choose_again_slot_after_drain_and_restore() {
    let mut build = authored_build(fixture::ruleset().document().clone(), 2);
    build.train(1, Skill::Perception).unwrap();
    let starting = *build.starting_skills();
    let mut game = game_with_build(build);
    let actor = ActorId(1);
    let mut old_proposal = request(&game, actor);
    old_proposal["value"]["purchase"]["owner"] = json!(1);
    let mut drained = game.creature(actor).unwrap().build().clone();
    assert!(drained.remove_latest().is_some());
    game.rebuild_creature(actor, drained.clone()).unwrap();
    drained.add_hit_die(HdSource::Racial, 42).unwrap();
    assert!(drained.choices()[1].purchases.is_empty());
    game.rebuild_creature(actor, drained).unwrap();
    let before = game.clone();
    assert_eq!(
        game.act(actor, action(old_proposal)),
        Err(GameError::Build(BuildError::StaleAdvancementPreview))
    );
    assert_eq!(game, before);
    let mut fresh = request(&game, actor);
    fresh["value"]["purchase"]["owner"] = json!(1);
    game.act(actor, action(fresh)).unwrap();
    let creature = game.creature(actor).unwrap();
    assert!(creature.build().choices()[0].purchases.is_empty());
    assert_eq!(creature.build().choices()[1].purchases.len(), 1);
    assert_eq!(creature.build().starting_skills(), &starting);
    assert_eq!(creature.build().unspent_points(), 1);
    assert_eq!(creature.build_epoch().get(), 4);
}

#[test]
fn phase5_purchase_reduced_maximum_death_uses_one_lifecycle_boundary() {
    use tor_simulation::{
        content_ids::TalentId,
        rules::{Modifier, ModifierOp, Requirement, Scope, Stat, TalentDef},
    };
    let mut document = fixture::ruleset().document().clone();
    document.talents.push(TalentDef {
        form_choices: vec![],
        id: TalentId::new("fragile_choice").unwrap(),
        name: "Fragile choice".into(),
        prerequisites: vec![],
        requirement: Requirement::TotalHd { minimum: 1 },
        modifiers: vec![Modifier {
            scope: Scope::Always,
            op: ModifierOp::Add {
                stat: Stat::Health,
                value: -10,
            },
        }],
    });
    let mut game = game_with_build(authored_build(document, 1));
    let actor = ActorId(1);
    let maximum = game.health(actor).unwrap().1;
    fixture::injure(&mut game, actor, maximum - 5).unwrap();
    let injury = game.creature(actor).unwrap().health().injury();
    let quiet = game
        .creature(actor)
        .unwrap()
        .received_health_loss_remaining();
    let mut value = request(&game, actor);
    value["value"]["purchase"]["selection"] = json!({"kind":"talent","talent":"fragile_choice"});
    let proposal = action(value);
    let Action::Purchase { basis, purchase } = &proposal else {
        unreachable!()
    };
    let original = game.clone();
    let preview = game
        .preview_player_purchase(actor, basis, purchase)
        .unwrap();
    assert!(injury >= preview.runtime().health.maximum());
    assert!(preview.runtime().health.dead());
    assert_eq!(game, original);
    game.act(actor, proposal).unwrap();
    let creature = game.creature(actor).unwrap();
    assert!(creature.health().dead());
    assert_eq!(creature.health().injury(), injury);
    assert_eq!(
        creature.received_health_loss_remaining(),
        quiet,
        "a maximum reduction is not received damage"
    );
    assert_eq!(creature.build_epoch().get(), 2);
    assert_eq!(game.combat_events().iter().filter(|event| matches!(event, tor_simulation::combat::CombatEvent::Died { actor: dead } if *dead == actor)).count(), 1);
    assert_eq!(round_trip(&game), game);
}

pub(super) fn game_with_runtime_sources() -> Game {
    use tor_simulation::{
        content_ids::{DescriptorId, TalentId},
        resources::Resource,
        rules::{Modifier, ModifierOp, Requirement, Scope, Selector, Stat, TalentDef},
    };
    let add = |stat, value| Modifier {
        scope: Scope::Always,
        op: ModifierOp::Add { stat, value },
    };
    let mut document = fixture::ruleset().document().clone();
    document
        .abilities
        .iter_mut()
        .find(|ability| ability.id.as_str() == "basic_melee")
        .unwrap()
        .cost = Some(tor_simulation::rules::Cost {
        resource: Resource::Stamina,
        start: 0,
        resolution: 1,
    });
    document
        .conditions
        .iter_mut()
        .find(|condition| condition.id.as_str() == "fear")
        .unwrap()
        .modifiers = vec![
        add(Stat::Health, 7),
        add(
            Stat::ResourceMax {
                resource: Resource::Focus,
            },
            4,
        ),
    ];
    document.talents.push(TalentDef {
        form_choices: vec![],
        id: TalentId::new("calm_choice").unwrap(),
        name: "Calm choice".into(),
        prerequisites: vec![],
        requirement: Requirement::TotalHd { minimum: 1 },
        modifiers: vec![
            Modifier {
                scope: Scope::Always,
                op: ModifierOp::Immunity {
                    selector: Selector::Descriptor {
                        descriptor: DescriptorId::new("fear").unwrap(),
                    },
                },
            },
            Modifier {
                scope: Scope::Always,
                op: ModifierOp::Set {
                    stat: Stat::ResourceMax {
                        resource: Resource::Focus,
                    },
                    value: 1,
                },
            },
        ],
    });
    document.talents.push(TalentDef {
        form_choices: vec![],
        id: TalentId::new("lethal_choice").unwrap(),
        name: "Lethal choice".into(),
        prerequisites: vec![],
        requirement: Requirement::TotalHd { minimum: 1 },
        modifiers: vec![add(Stat::Health, -10)],
    });
    let mut game = game_with_build(authored_build(document, 1));
    let actor = ActorId(1);
    let mut spec = tor_simulation::ItemSpec::ordinary("known ring".into());
    spec.class = tor_simulation::ItemClass::Ring;
    spec.concealed = true;
    spec.appearance = "unmarked ring".into();
    spec.equipment = Some(tor_simulation::EquipmentSpec {
        slot: tor_simulation::EquipmentSlot::new("ring").unwrap(),
        attack: None,
        modifiers: vec![
            add(Stat::Health, 5),
            add(Stat::HealthRecoveryPeriod, 100),
            add(
                Stat::ResourceMax {
                    resource: Resource::Stamina,
                },
                3,
            ),
        ],
    });
    game.place_item_stack(
        100,
        Location {
            region: RegionId(1),
            position: Position { x: 1, y: 1, z: 0 },
        },
        Some(actor),
        1,
        spec,
    )
    .unwrap();
    let slot = game
        .anatomy(actor)
        .unwrap()
        .slots
        .iter()
        .position(|slot| slot.as_str() == "ring")
        .unwrap();
    game.equip_authored(
        actor,
        tor_simulation::ItemId(100),
        tor_simulation::EquipmentSlotId(slot as u16),
    )
    .unwrap();
    runtime_effects(
        &mut game,
        vec![
            tor_simulation::abilities::ResolvedEffect::ApplyCondition {
                id: tor_simulation::content_ids::ConditionId::new("fear").unwrap(),
                duration: 1_000,
            },
            tor_simulation::abilities::ResolvedEffect::Restore {
                resource: Resource::Focus,
                amount: 1_000,
            },
            tor_simulation::abilities::ResolvedEffect::Damage { amount: 4 },
        ],
    );
    game
}
fn runtime_effects(game: &mut Game, effects: Vec<tor_simulation::abilities::ResolvedEffect>) {
    game.apply(tor_simulation::abilities::Outcome {
        actor: ActorId(2),
        target: ActorId(1),
        ability: None,
        intention: None,
        result: tor_simulation::abilities::OutcomeStatus::Applied,
        check: None,
        participation: tor_simulation::abilities::AttemptedParticipation::target(),
        target_rider_participated: false,
        effects,
    })
    .unwrap();
}

#[test]
fn phase5_queued_purchase_replays_exact_provenance_and_current_runtime_not_preview_snapshot() {
    let mut game = game_with_runtime_sources();
    let actor = ActorId(1);
    let proposal = action(request(&game, actor));
    let Action::Purchase { basis, purchase } = &proposal else {
        unreachable!()
    };
    let original = game.clone();
    let preview = game
        .preview_player_purchase(actor, basis, purchase)
        .unwrap();
    assert_eq!(game, original);
    assert_eq!(
        preview.runtime().health.maximum(),
        preview.derived().maximum_health + 12
    );
    assert_eq!(preview.runtime().health.injury(), 4);
    assert!(preview.runtime().removed_conditions.is_empty());
    let identity = game
        .admit_intention(actor, proposal.clone(), IntentionOrigin::Human)
        .unwrap();
    runtime_effects(
        &mut game,
        vec![tor_simulation::abilities::ResolvedEffect::Heal { amount: 2 }],
    );
    game.identify_item(actor, tor_simulation::ItemId(100))
        .unwrap();
    let mut before = recorded(&game, actor);
    assert_ne!(before["injury"], json!(preview.runtime().health.injury()));
    let equipment = game.equipment(actor).unwrap().clone();
    assert!(!equipment.is_empty());
    assert!(!game.creature(actor).unwrap().conditions().is_empty());
    let mut replay = round_trip(&game);
    assert_eq!(
        replay.pending_intention(actor).unwrap().work,
        tor_simulation::IntentionWork::Action(proposal.clone())
    );
    for subject in [&mut game, &mut replay] {
        let execution = subject.execute_next_intention().unwrap();
        assert_eq!(execution.intention.id, identity);
        assert_eq!(
            execution.intention.work,
            tor_simulation::IntentionWork::Action(proposal.clone())
        );
        assert_eq!(execution.action, Some(proposal.clone()));
        assert!(execution.outcome.is_ok());
        assert_eq!(subject.equipment(actor).unwrap(), &equipment);
        assert!(subject
            .observe(actor)
            .unwrap()
            .inventory
            .iter()
            .any(|item| item.id == tor_simulation::ItemId(100) && item.identified));
    }
    assert_eq!(game, replay);
    let mut after = recorded(&game, actor);
    assert_eq!(before["earned_progress"]["peak_threat"], json!(1));
    assert_eq!(
        after["earned_progress"]["peak_threat"],
        json!(2),
        "the committed permanent purchase raises recorded peak threat"
    );
    before["earned_progress"]["peak_threat"] = json!(2);
    for key in ["build", "build_epoch"] {
        before.as_object_mut().unwrap().remove(key);
        after.as_object_mut().unwrap().remove(key);
    }
    assert_eq!(after, before);
}

#[test]
fn phase5_purchase_preview_discloses_effective_clamps_and_condition_removal_read_only() {
    use tor_simulation::resources::Resource;
    let mut game = game_with_runtime_sources();
    let actor = ActorId(1);
    let mut value = request(&game, actor);
    value["value"]["purchase"]["selection"] = json!({"kind":"talent","talent":"calm_choice"});
    let proposal = action(value);
    let Action::Purchase { basis, purchase } = &proposal else {
        unreachable!()
    };
    let original = game.clone();
    let before = game.creature(actor).unwrap();
    assert!(
        before.costs().resources().balance(Resource::Focus) > 1,
        "fixture must fund an enabled resource before reducing its maximum"
    );
    assert_eq!(
        before.costs().resources().balance(Resource::Focus),
        before.costs().resources().maximum(Resource::Focus)
    );
    let preview = game
        .preview_player_purchase(actor, basis, purchase)
        .unwrap();
    assert_eq!(game, original);
    assert_eq!(preview.runtime().resources.focus.maximum, 1);
    assert_eq!(preview.runtime().resources.focus.balance, 1);
    assert_eq!(
        preview.runtime().health.maximum(),
        preview.derived().maximum_health + 5
    );
    assert_eq!(preview.runtime().health.injury(), before.health().injury());
    assert_eq!(preview.runtime().build_epoch.get(), 2);
    assert_eq!(
        preview.runtime().removed_conditions,
        vec![tor_simulation::player_advancement::RemovedCondition {
            condition: tor_simulation::content_ids::ConditionId::new("fear").unwrap(),
            applications: 1,
            stacks: 1,
        }]
    );
    assert!(
        preview.runtime().resources.focus.balance
            < before.costs().resources().balance(Resource::Focus)
    );
    game.act(actor, proposal).unwrap();
    let after = game.creature(actor).unwrap();
    assert_eq!(after.health(), preview.runtime().health);
    assert_eq!(after.health_recovery(), preview.runtime().health_recovery);
    assert_eq!(after.build_epoch(), preview.runtime().build_epoch);
    for resource in Resource::ALL {
        assert_eq!(
            after.costs().resources().balance(resource),
            preview.runtime().resources.get(resource).balance
        );
        assert_eq!(
            after.costs().resources().maximum(resource),
            preview.runtime().resources.get(resource).maximum
        );
    }
    assert!(after.conditions().is_empty());
    assert!(!game.equipment(actor).unwrap().is_empty());
}

fn lethal_equipped_purchase() -> (Game, Action) {
    let mut game = game_with_runtime_sources();
    let actor = ActorId(1);
    let remaining = game.health(actor).unwrap().0;
    fixture::injure(&mut game, actor, remaining - 5).unwrap();
    let mut value = request(&game, actor);
    value["value"]["purchase"]["selection"] = json!({"kind":"talent","talent":"lethal_choice"});
    (game, action(value))
}

#[test]
fn phase5_lethal_equipped_purchase_preview_matches_finalized_death_state_once() {
    use tor_simulation::resources::Resource;
    let (mut game, proposal) = lethal_equipped_purchase();
    let actor = ActorId(1);
    let Action::Purchase { basis, purchase } = &proposal else {
        unreachable!()
    };
    let original = game.clone();
    let preview = game
        .preview_player_purchase(actor, basis, purchase)
        .unwrap();
    assert!(preview.runtime().health.dead());
    assert_eq!(
        game, original,
        "death preview must not publish equipment, corpse or events"
    );
    game.act(actor, proposal).unwrap();
    let after = game.creature(actor).unwrap();
    assert_eq!(preview.runtime().health, after.health());
    assert_eq!(preview.runtime().health_recovery, after.health_recovery());
    assert_eq!(preview.runtime().build_epoch, after.build_epoch());
    for resource in Resource::ALL {
        assert_eq!(
            preview.runtime().resources.get(resource).balance,
            after.costs().resources().balance(resource)
        );
        assert_eq!(
            preview.runtime().resources.get(resource).maximum,
            after.costs().resources().maximum(resource)
        );
    }
    assert_eq!(
        preview
            .runtime()
            .removed_conditions
            .iter()
            .map(|row| u32::from(row.applications))
            .sum::<u32>(),
        1
    );
    assert!(after.conditions().is_empty());
    assert_eq!(preview.runtime().canceled_resource_holds, 0);
    assert!(game.equipment(actor).unwrap().is_empty());
    assert_eq!(game.combat_events().iter().filter(|event| matches!(event, tor_simulation::combat::CombatEvent::Died { actor: dead } if *dead == actor)).count(), 1);
    let observation = game.observe(actor).unwrap();
    assert_eq!(
        observation
            .ground_items
            .iter()
            .filter(|item| item.class == tor_simulation::ItemClass::Corpse)
            .count(),
        1
    );
    assert!(observation
        .ground_items
        .iter()
        .any(|item| item.id == tor_simulation::ItemId(100)));
    assert_eq!(round_trip(&game), game);
}

#[test]
fn phase5_lethal_equipped_purchase_preview_rejects_corpse_identity_exhaustion() {
    let (game, proposal) = lethal_equipped_purchase();
    let mut shared = tor_simulation::checkpoint::SharedState::default();
    let mut snapshot = serde_json::to_value(game.checkpoint(&mut shared)).unwrap();
    snapshot["next_item_id"] = json!(u64::MAX);
    let mut game = Game::restore_checkpoint(
        serde_json::from_value(snapshot).unwrap(),
        &shared,
        game.content().clone(),
    )
    .unwrap();
    let before = game.clone();
    let Action::Purchase { basis, purchase } = &proposal else {
        unreachable!()
    };
    assert_eq!(
        game.preview_player_purchase(ActorId(1), basis, purchase)
            .map(|_| ()),
        Err(GameError::IdentityExhausted)
    );
    assert_eq!(
        game.act(ActorId(1), proposal),
        Err(GameError::IdentityExhausted)
    );
    assert_eq!(game, before);
}

#[test]
fn phase5_lethal_equipped_purchase_preview_reports_final_canceled_holds_without_releasing_them() {
    let (mut game, proposal) = lethal_equipped_purchase();
    let actor = ActorId(1);
    game.admit_intention(
        actor,
        Action::UseAbility {
            id: game.basic_attack(actor).unwrap(),
            invocation: tor_simulation::abilities::AbilityInvocation::ActorTarget {
                target: ActorId(2),
            },
            supplies: Default::default(),
        },
        IntentionOrigin::Human,
    )
    .unwrap();
    assert!(game.execute_next_intention().unwrap().outcome.is_ok());
    assert_eq!(
        game.creature(actor).unwrap().costs().reservations().len(),
        1
    );
    let Action::Purchase { basis, purchase } = &proposal else {
        unreachable!()
    };
    let original = game.clone();
    let preview = game
        .preview_player_purchase(actor, basis, purchase)
        .unwrap();
    assert!(preview.runtime().health.dead());
    assert_eq!(preview.runtime().canceled_resource_holds, 1);
    assert_eq!(game, original);
    assert_eq!(
        game.admit_intention(actor, proposal, IntentionOrigin::Human),
        Err(GameError::ActorBusy)
    );
    assert_eq!(game, original);
}
