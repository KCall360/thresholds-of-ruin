//! Timed player replacement uses the existing ownership and finite-claim kernels.
use super::creature_fixture as fixture;
use serde_json::{json, Value};
use std::{num::NonZeroU64, sync::Arc};
use tor_simulation::{
    attributes::{ManaBinding, Skill},
    content_ids::PointPoolId,
    creatures::{Content, CreatureBuild, PurchaseLocation, PurchaseSelection},
    progression::{HdLedger, HdSource},
    rules::Ruleset,
    Action, ActorId, CreatureIdentity, Game, IntentionOrigin, ItemId, ItemSpec,
};
use tor_world::{Location, Position, RegionId};

fn at(x: i32) -> Location {
    Location {
        region: RegionId(1),
        position: Position { x, y: 1, z: 0 },
    }
}
fn fixture_game() -> Game {
    fixture_custom(|_| {})
}
#[test]
fn zero_damage_status_pauses_respec_but_damage_in_either_order_still_discards_it() {
    use tor_simulation::content_ids::ConditionId;
    for damage_first in [false, true] {
        let mut game = fixture_custom(|document| {
            document["conditions"].as_array_mut().unwrap().push(
                super::status_conditions::definition("fixture_stillness", Some(400)),
            );
        });
        let owner = ActorId(1);
        let intention = start(&mut game);
        wait_until(&mut game, 4);
        let original = game.preparation(owner).unwrap().clone();
        let damage = [super::abilities::damage(
            1,
            tor_simulation::combat::DamageType::Vital,
        )];
        if damage_first {
            game.apply_effects(owner, &damage).unwrap();
        }
        game.apply_condition(
            owner,
            &ConditionId::new("fixture_stillness").unwrap(),
            ActorId(2),
            200,
        )
        .unwrap();
        if !damage_first {
            let pending = game
                .preparation(owner)
                .expect("zero damage status does not discard respec");
            assert!(!pending.active);
            assert_eq!(pending.remaining, 6);
            assert_eq!(pending.plan, original.plan);
            assert_eq!(pending.item_payment, original.item_payment);
            assert_eq!(pending.origin_intention(), Some(intention));
            assert_eq!(round_trip(&game), game);
            game.apply_effects(owner, &damage).unwrap();
        }
        assert!(game.preparation(owner).is_none());
        assert_eq!(quantity(&game), Some(1));
        assert_eq!(round_trip(&game), game);
        wait_until(&mut game, 204);
        assert!(
            game.preparation(owner).is_none(),
            "thaw cannot resurrect discarded respec"
        );
        assert!(game.resume_intention(owner, intention).is_err());
    }
}

#[test]
fn a_paused_status_keeps_respec_remaining_work_through_lazy_mid_denial_restore() {
    use tor_simulation::content_ids::ConditionId;
    let mut game =
        fixture_custom(|document| {
            document["conditions"].as_array_mut().unwrap().push(
                super::status_conditions::definition("fixture_stillness", Some(400)),
            );
        });
    let owner = ActorId(1);
    let intention = start(&mut game);
    wait_until(&mut game, 4);
    game.apply_condition(
        owner,
        &ConditionId::new("fixture_stillness").unwrap(),
        ActorId(2),
        200,
    )
    .unwrap();
    let paused = game.preparation(owner).unwrap().clone();
    wait_until(&mut game, 8);
    assert_eq!(round_trip(&game), game);
    game = round_trip(&game);
    wait_until(&mut game, 204);
    assert!(!game.preparation(owner).unwrap().active);
    assert_eq!(game.preparation(owner).unwrap().remaining, 6);
    assert_eq!(game.preparation(owner).unwrap().plan, paused.plan);
    assert_eq!(round_trip(&game), game);
    game.resume_intention(owner, intention).unwrap();
}
fn fixture_custom(customize: impl FnOnce(&mut Value)) -> Game {
    let original = fixture::content();
    let mut document = serde_json::to_value(original.rules().document()).unwrap();
    document["advancement"]["respec_prepare"] = json!(10);
    document["advancement"]["respec_recovery"] = json!(3);
    document["advancement"]["received_loss_interval"] = json!(6);
    customize(&mut document);
    let rules = Ruleset::validate(serde_json::from_value(document).unwrap()).unwrap();
    let content = Content::new(rules, (*original.catalog()).clone()).unwrap();
    let mut game = Game::two_room_in_stone(42, content.clone());
    for x in [1, 2] {
        let actor = game
            .spawn_actor(at(x), NonZeroU64::new(1).unwrap())
            .unwrap();
        let mut build = CreatureBuild::new(
            Arc::clone(&content),
            fixture::species().id,
            HdLedger::seeded(vec![HdSource::Racial], 42).unwrap(),
            ManaBinding::Intellect,
            [Skill::Deception, Skill::Persuasion],
        )
        .unwrap();
        let preview = build
            .preview_purchase(
                tor_simulation::HdIndex(0),
                PointPoolId::new("advancement").unwrap(),
                PurchaseSelection::SkillRank(Skill::Perception),
            )
            .unwrap();
        build = preview.build().clone();
        game.configure_creature(
            actor,
            CreatureIdentity {
                name: format!("owner{x}"),
                faction: "allies".into(),
            },
            build,
        )
        .unwrap();
    }
    let mut spec = ItemSpec::ordinary("ration".into());
    spec.class = tor_simulation::ItemClass::Food;
    spec.edible = Some(tor_simulation::EdibleSpec {
        nutrition: tor_simulation::rules::AmountExpr::Fixed { amount: 9 },
    });
    spec.supply = Some(tor_simulation::content_ids::SupplyId::new("ration").unwrap());
    spec.stackable = true;
    game.place_item_stack(10, at(1), Some(ActorId(1)), 1, spec)
        .unwrap();
    game.refresh_navigation();
    game
}
fn request(game: &Game, actor: ActorId) -> Value {
    json!({"type":"respec", "value": {
        "basis":{"actor":actor.0,"branch":"admitted-branch","stream":"admitted-stream","reset":7,"build_epoch":game.creature(actor).unwrap().build_epoch().get()},
        "replacement":{"source":{"owner":0,"index":0},"replacement":null},
        "supplies":{"kind":"carried"}
    }})
}
fn action(value: Value) -> Action {
    serde_json::from_value(value).expect("ordinary typed ration replacement must decode")
}
fn start(game: &mut Game) -> tor_simulation::IntentionId {
    let actor = ActorId(1);
    let action = action(request(game, actor));
    let id = game
        .admit_intention(actor, action, IntentionOrigin::Human)
        .unwrap();
    game.execute_next_intention().unwrap().outcome.unwrap();
    id
}
fn quantity(game: &Game) -> Option<u64> {
    game.observe(ActorId(1))
        .unwrap()
        .inventory
        .iter()
        .find(|entry| entry.id == ItemId(10))
        .map(|entry| entry.quantity)
}
fn round_trip(game: &Game) -> Game {
    let mut shared = tor_simulation::checkpoint::SharedState::default();
    let checkpoint = game.checkpoint(&mut shared);
    let (checkpoint, shared) =
        serde_json::from_slice(&serde_json::to_vec(&(checkpoint, shared)).unwrap()).unwrap();
    Game::restore_checkpoint(checkpoint, &shared, game.content().clone()).unwrap()
}
fn wait_until(game: &mut Game, tick: u64) {
    // This fixture has two actors with one-tick waits. Each full pair of
    // successful waits advances at least one tick, including a paused owner.
    let budget = tick.saturating_sub(game.tick()).saturating_mul(2) + 2;
    for _ in 0..budget {
        if game.tick() >= tick {
            return;
        }
        game.act(game.next_actor().unwrap(), Action::Wait).unwrap();
    }
    panic!(
        "fixture stopped at {} before requested tick {tick} after {budget} waits",
        game.tick()
    );
}

#[test]
fn phase5_respec_holds_one_ration_then_atomically_replaces_at_completion() {
    let mut game = fixture_game();
    let actor = ActorId(1);
    let source = game.creature(actor).unwrap().build().clone();
    let epoch = game.creature(actor).unwrap().build_epoch();
    let permanent = source
        .preview_replacement(
            PurchaseLocation {
                owner: tor_simulation::HdIndex(0),
                index: 0,
            },
            None,
        )
        .unwrap();
    let id = start(&mut game);
    let pending = game.preparation(actor).unwrap();
    assert_eq!(pending.origin_intention(), Some(id));
    let encoded = serde_json::to_value(pending).unwrap();
    assert_eq!(encoded["progress"], json!({"kind":"respec"}));
    assert_eq!(encoded["item_payment"]["kind"], json!("held"));
    assert_eq!(quantity(&game), Some(1));
    assert_eq!(game.creature(actor).unwrap().build(), &source);
    assert_eq!(round_trip(&game), game);
    wait_until(&mut game, 9);
    assert!(game.preparation(actor).is_some());
    assert_eq!(quantity(&game), Some(1));
    wait_until(&mut game, 10);
    assert!(game.preparation(actor).is_none());
    assert_eq!(quantity(&game), None);
    assert_eq!(game.creature(actor).unwrap().build(), permanent.build());
    assert_eq!(
        game.creature(actor).unwrap().build_epoch().get(),
        epoch.get() + 1
    );
    assert_eq!(
        game.observe(actor)
            .unwrap()
            .combat
            .unwrap()
            .recovery_remaining,
        3
    );
    assert_eq!(round_trip(&game), game);
}

#[test]
fn phase5_respec_benign_pause_keeps_original_basis_lineage_and_cancel_releases() {
    let mut game = fixture_game();
    let actor = ActorId(1);
    let original = request(&game, actor);
    let id = start(&mut game);
    wait_until(&mut game, 4);
    game.pause_preparation(actor).unwrap();
    let paused = game.preparation(actor).unwrap().clone();
    assert!(!paused.active);
    assert_eq!(paused.remaining, 6);
    game = round_trip(&game);
    wait_until(&mut game, 8);
    assert_eq!(game.preparation(actor).unwrap().remaining, 6);
    game.resume_intention(actor, id).unwrap();
    while game.next_intention_actor() != Some(actor) {
        game.act(game.next_actor().unwrap(), Action::Wait).unwrap();
    }
    game.execute_next_intention().unwrap().outcome.unwrap();
    let resumed = game.preparation(actor).unwrap();
    assert_eq!(resumed.origin_intention(), Some(id));
    let encoded = serde_json::to_value(&resumed.plan).unwrap();
    assert_eq!(encoded["basis"], original["value"]["basis"]);
    assert_eq!(resumed.remaining, 6);
    game.cancel_intention(actor, resumed.intention.unwrap())
        .unwrap();
    assert!(game.preparation(actor).is_none());
    assert_eq!(quantity(&game), Some(1));
    assert!(game
        .admit_intention(
            actor,
            Action::Drop {
                item: ItemId(10),
                quantity: None
            },
            IntentionOrigin::Human
        )
        .is_ok());
}

#[test]
fn phase5_respec_actual_received_loss_discards_progress_and_requires_full_retry() {
    let mut game = fixture_game();
    let actor = ActorId(1);
    start(&mut game);
    wait_until(&mut game, 4);
    game.apply_effects(
        actor,
        &[super::abilities::damage(
            1,
            tor_simulation::combat::DamageType::Vital,
        )],
    )
    .unwrap();
    assert!(
        game.preparation(actor).is_none(),
        "unsafe interruption discards rather than suspends"
    );
    assert_eq!(quantity(&game), Some(1));
    let unchanged = game.clone();
    assert!(game
        .admit_intention(actor, action(request(&game, actor)), IntentionOrigin::Human)
        .is_err());
    assert_eq!(game, unchanged);
    wait_until(&mut game, 10);
    while game.next_actor() != Some(actor) {
        game.act(game.next_actor().unwrap(), Action::Wait).unwrap();
    }
    start(&mut game);
    assert_eq!(game.preparation(actor).unwrap().remaining, 10);
}

#[test]
fn phase5_respec_stale_queued_epoch_rejects_without_creating_a_ration_hold() {
    let mut game = fixture_game();
    let actor = ActorId(1);
    game.admit_intention(actor, action(request(&game, actor)), IntentionOrigin::Human)
        .unwrap();
    let mut build = game.creature(actor).unwrap().build().clone();
    let preview = build
        .preview_replacement(
            PurchaseLocation {
                owner: tor_simulation::HdIndex(0),
                index: 0,
            },
            None,
        )
        .unwrap();
    build.commit_replacement(preview).unwrap();
    game.rebuild_creature(actor, build).unwrap();
    game = round_trip(&game);
    assert!(game.execute_next_intention().unwrap().outcome.is_err());
    assert!(game.preparation(actor).is_none());
    assert_eq!(quantity(&game), Some(1));
}

#[test]
fn phase5_respec_strict_operands_reject_client_candidate_refunds_and_missing_basis() {
    let game = fixture_game();
    let valid = request(&game, ActorId(1));
    let decoded = action(valid.clone());
    assert_eq!(serde_json::to_value(decoded).unwrap(), valid);
    for field in ["candidate", "refunds", "price"] {
        let mut invalid = valid.clone();
        invalid["value"]["replacement"][field] = json!([]);
        assert!(serde_json::from_value::<Action>(invalid).is_err());
    }
    for field in ["basis", "replacement", "supplies"] {
        let mut invalid = valid.clone();
        invalid["value"].as_object_mut().unwrap().remove(field);
        assert!(serde_json::from_value::<Action>(invalid).is_err());
    }
    for (field, value) in [
        ("owner", json!(u64::MAX)),
        ("owner", json!(-1)),
        ("owner", json!("65536")),
        ("owner", json!(65536.0)),
        ("index", json!(256)),
    ] {
        let mut invalid = valid.clone();
        invalid["value"]["replacement"]["source"][field] = value;
        assert!(serde_json::from_value::<Action>(invalid).is_err());
    }
    let mut wide_owner = valid.clone();
    wide_owner["value"]["replacement"]["source"]["owner"] = json!(65_536);
    assert!(serde_json::from_value::<Action>(wide_owner).is_ok());
    let mut invalid = valid.clone();
    invalid["value"]["replacement"]["source"] = json!([0, 0]);
    assert!(serde_json::from_value::<Action>(invalid).is_err());
    let mut invalid = valid;
    invalid["value"]["replacement"]
        .as_object_mut()
        .unwrap()
        .remove("replacement");
    assert!(
        serde_json::from_value::<Action>(invalid).is_err(),
        "refund-only still requires explicit null"
    );
}

#[test]
fn phase5_respec_permanent_build_change_immediately_releases_its_ration() {
    let mut game = fixture_game();
    let actor = ActorId(1);
    start(&mut game);
    wait_until(&mut game, 4);
    let mut build = game.creature(actor).unwrap().build().clone();
    let preview = build
        .preview_replacement(
            PurchaseLocation {
                owner: tor_simulation::HdIndex(0),
                index: 0,
            },
            None,
        )
        .unwrap();
    build.commit_replacement(preview).unwrap();
    game.rebuild_creature(actor, build).unwrap();
    assert!(
        game.preparation(actor).is_none(),
        "changed epoch invalidates within the build lifecycle boundary"
    );
    assert_eq!(quantity(&game), Some(1));
    assert!(game
        .admit_intention(
            actor,
            Action::Drop {
                item: ItemId(10),
                quantity: None
            },
            IntentionOrigin::Human
        )
        .is_ok());
    assert_eq!(round_trip(&game), game);
}
#[test]
fn phase5_respec_restore_rejects_an_unheld_extra_explicit_ration() {
    let mut game = fixture_game();
    let mut spec = ItemSpec::ordinary("second ration".into());
    spec.supply = Some(tor_simulation::content_ids::SupplyId::new("ration").unwrap());
    game.place_item_stack(11, at(1), Some(ActorId(1)), 1, spec)
        .unwrap();
    start(&mut game);
    let mut shared = tor_simulation::checkpoint::SharedState::default();
    let snapshot = game.checkpoint(&mut shared);
    let mut value = serde_json::to_value((snapshot, shared)).unwrap();
    let actors = value[0]["actors"].as_u64().unwrap() as usize;
    let plan = &mut value[1]["actors"][actors]["1"]["pending"]["plan"];
    plan["supplies"] = json!({"kind":"explicit","items":[{"cost_ordinal":0,"item":10},{"cost_ordinal":0,"item":11}]});
    let (snapshot, shared) = serde_json::from_value(value).unwrap();
    assert!(
        Game::restore_checkpoint(snapshot, &shared, game.content().clone()).is_err(),
        "a selected exact source must own its reciprocal hold"
    );
}

#[test]
fn phase5_respec_progress_and_frozen_policy_cannot_be_substituted_on_restore() {
    let mut game = fixture_game();
    start(&mut game);
    let mut shared = tor_simulation::checkpoint::SharedState::default();
    let snapshot = game.checkpoint(&mut shared);
    let original = serde_json::to_value((snapshot, shared)).unwrap();
    let actors = original[0]["actors"].as_u64().unwrap() as usize;
    for mutation in 0..10 {
        let mut value = original.clone();
        let pending = &mut value[1]["actors"][actors]["1"]["pending"];
        match mutation {
            0 => pending["progress"] = json!({"kind":"item"}),
            1 => {
                pending["progress"] = json!({"kind":"ability","next_strike":0,"successful_primaries":0,"attempted_riders":[0,0],"linked_hold":null})
            }
            2 => pending["plan"]["basis"]["build_epoch"] = json!(2),
            3 => pending["plan"]["basis"]["actor"] = json!(2),
            4 => pending["plan"]["preparation"] = json!(9),
            5 => pending["plan"]["recovery"] = json!(2),
            6 => pending["plan"]["safety"]["received_loss_interval"] = json!(5),
            7 => pending["plan"]["ration"]["spend_at"] = json!("first_resolution"),
            8 => pending["plan"]["ration"]["quantity"] = json!(2),
            9 => pending["plan"]["replacement"]["source"]["index"] = json!(1),
            _ => unreachable!(),
        }
        let decoded: Result<
            (
                tor_simulation::checkpoint::Snapshot,
                tor_simulation::checkpoint::SharedState,
            ),
            _,
        > = serde_json::from_value(value);
        if mutation == 1 {
            assert!(
                decoded.is_ok(),
                "wrong progress kind must reach checkpoint admission"
            );
        }
        assert!(
            decoded.is_err()
                || decoded.is_ok_and(|(snapshot, shared)| Game::restore_checkpoint(
                    snapshot,
                    &shared,
                    game.content().clone()
                )
                .is_err()),
            "accepted corruption {mutation}"
        );
    }
    assert_eq!(round_trip(&game), game);
}

#[test]
fn phase5_respec_damage_while_paused_discards_without_refunding_elapsed_time() {
    let mut game = fixture_game();
    start(&mut game);
    wait_until(&mut game, 4);
    game.pause_preparation(ActorId(1)).unwrap();
    let now = game.tick();
    game.apply_effects(
        ActorId(1),
        &[super::abilities::damage(
            1,
            tor_simulation::combat::DamageType::Vital,
        )],
    )
    .unwrap();
    assert!(game.preparation(ActorId(1)).is_none());
    assert_eq!(game.tick(), now);
    assert_eq!(quantity(&game), Some(1));
    assert_eq!(
        game.creature(ActorId(1))
            .unwrap()
            .received_health_loss_remaining(),
        6
    );
    assert_eq!(round_trip(&game), game);
}

#[test]
fn phase5_respec_choice_rejects_valid_positional_aliases() {
    use tor_simulation::player_advancement::PurchaseChoice;
    let accepted: Vec<_> = [
        json!(["skill_rank", "perception"]),
        json!(["talent", "keen_senses"]),
    ]
    .into_iter()
    .filter(|value| serde_json::from_value::<PurchaseChoice>(value.clone()).is_ok())
    .collect();
    assert!(accepted.is_empty(), "accepted choice aliases {accepted:?}");
}
#[test]
fn phase5_respec_progress_rejects_valid_positional_aliases() {
    use tor_simulation::combat::PreparationProgress;
    let named = json!({"kind":"ability","next_strike":0,"successful_primaries":0,"attempted_riders":[0,0],"linked_hold":null});
    let progress = serde_json::from_value::<PreparationProgress>(named.clone()).unwrap();
    assert_eq!(serde_json::to_value(progress).unwrap(), named);
    let accepted: Vec<_> = [
        json!(["respec"]),
        json!(["item"]),
        json!(["ability", 0, 0, [0, 0], null]),
    ]
    .into_iter()
    .filter(|value| serde_json::from_value::<PreparationProgress>(value.clone()).is_ok())
    .collect();
    assert!(
        accepted.is_empty(),
        "accepted progress aliases {accepted:?}"
    );
}
#[test]
fn phase5_respec_action_rejects_valid_outer_positional_alias() {
    let game = fixture_game();
    let proposal = request(&game, ActorId(1));
    assert!(
        serde_json::from_value::<Action>(json!(["respec", proposal["value"]])).is_err(),
        "accepted outer action alias"
    );
}
#[test]
fn phase5_respec_action_rejects_valid_operand_positional_alias() {
    let game = fixture_game();
    let proposal = request(&game, ActorId(1));
    let mut positional = proposal.clone();
    positional["value"] = json!([
        proposal["value"]["basis"],
        proposal["value"]["replacement"],
        proposal["value"]["supplies"]
    ]);
    assert!(
        serde_json::from_value::<Action>(positional).is_err(),
        "accepted action value alias"
    );
}
#[test]
fn phase5_respec_plan_rejects_valid_positional_alias() {
    let mut game = fixture_game();
    start(&mut game);
    let plan = serde_json::to_value(&game.preparation(ActorId(1)).unwrap().plan).unwrap();
    let mut fields = plan.as_object().unwrap().clone();
    fields.remove("kind");
    assert!(
        serde_json::from_value::<tor_simulation::abilities::Plan>(json!(["respec", fields]))
            .is_err(),
        "accepted plan alias"
    );
}

fn hidden_location() -> Location {
    Location {
        region: RegionId(3),
        position: Position { x: 1, y: 1, z: 0 },
    }
}
fn add_enemy(game: &mut Game) -> ActorId {
    use std::collections::{BTreeMap, BTreeSet};
    game.add_region(tor_world::Region {
        id: RegionId(3),
        name: "Isolated room".into(),
        bounds: tor_world::Extent::new(8, 3, 1).unwrap(),
    })
    .unwrap();
    let enemy = game
        .spawn_actor(hidden_location(), NonZeroU64::new(1).unwrap())
        .unwrap();
    let build = game.creature(ActorId(2)).unwrap().build().clone();
    game.configure_creature(
        enemy,
        CreatureIdentity {
            name: "unseen enemy".into(),
            faction: "enemies".into(),
        },
        build,
    )
    .unwrap();
    game.configure_run(
        ActorId(1),
        BTreeSet::from([ActorId(1)]),
        None,
        BTreeMap::from([("allies".into(), BTreeSet::from(["enemies".into()]))]),
    )
    .unwrap();
    assert!(game
        .observe(ActorId(1))
        .unwrap()
        .visible_actors
        .iter()
        .all(|actor| actor.id != enemy));
    enemy
}
fn execute_for(game: &mut Game, actor: ActorId) -> tor_simulation::ActionOutcome {
    for _ in 0..100 {
        if game.next_intention_actor() == Some(actor) {
            return game.execute_next_intention().unwrap().outcome.unwrap();
        }
        if game.next_intention_actor().is_some() {
            game.execute_next_intention().unwrap().outcome.unwrap();
        } else {
            game.act(game.next_actor().unwrap(), Action::Wait).unwrap();
        }
    }
    panic!("queued fixture actor never became executable")
}
fn hidden_work(game: &mut Game, enemy: ActorId, name: &str) {
    game.admit_intention(
        enemy,
        Action::UseAbility {
            id: tor_simulation::content_ids::AbilityId::new(name).unwrap(),
            invocation: tor_simulation::abilities::AbilityInvocation::actor_target(enemy),
            supplies: Default::default(),
        },
        IntentionOrigin::Human,
    )
    .unwrap();
}
fn hidden_fixture() -> (Game, ActorId) {
    let mut game = fixture_custom(|document| {
        for name in ["private_plan_a", "private_plan_b"] {
            let mut ability = document["abilities"][0].clone();
            ability["id"] = json!(name);
            ability["name"] = json!(name);
            ability["basic_attack"] = json!(false);
            ability["cost"] = Value::Null;
            ability["supply_costs"] = json!([]);
            ability["cooldown"] = Value::Null;
            ability["reach"] = json!({"kind":"self_only"});
            ability["timing"] =
                json!({"source":{"kind":"fixed","prepare":100,"recover":5},"scaled_by":null});
            ability["strikes"] = json!({"kind":"authored","strikes":[{"offset":0,"unlock":null,"check":null,"on_success":[{"kind":"heal","amount":{"kind":"fixed","amount":1}}],"on_failure":[],"descriptors":[],"riders":[]}]});
            document["abilities"].as_array_mut().unwrap().push(ability);
            document["constants"]["default_abilities"]
                .as_array_mut()
                .unwrap()
                .push(json!(name));
        }
    });
    let enemy = add_enemy(&mut game);
    (game, enemy)
}
#[test]
fn phase5_respec_unseen_queued_and_prepared_work_cannot_change_eligibility_or_outcome() {
    for prepared in [false, true] {
        let (base, enemy) = hidden_fixture();
        let mut left = base.clone();
        let mut right = base;
        hidden_work(&mut left, enemy, "private_plan_a");
        hidden_work(&mut right, enemy, "private_plan_b");
        if prepared {
            execute_for(&mut left, enemy);
            execute_for(&mut right, enemy);
            assert!(left.preparation(enemy).is_some());
            assert!(right.preparation(enemy).is_some());
        }
        assert_eq!(
            left.observe(ActorId(1)).unwrap(),
            right.observe(ActorId(1)).unwrap()
        );
        // Equal public injuries refuse identically; no private plan is queried.
        let mut hurt_left = left.clone();
        let mut hurt_right = right.clone();
        for game in [&mut hurt_left, &mut hurt_right] {
            game.apply_effects(
                ActorId(1),
                &[super::abilities::damage(
                    1,
                    tor_simulation::combat::DamageType::Vital,
                )],
            )
            .unwrap();
        }
        let a = hurt_left.admit_intention(
            ActorId(1),
            action(request(&hurt_left, ActorId(1))),
            IntentionOrigin::Human,
        );
        let b = hurt_right.admit_intention(
            ActorId(1),
            action(request(&hurt_right, ActorId(1))),
            IntentionOrigin::Human,
        );
        assert_eq!(a, b);
        assert!(a.is_err());
        assert_eq!(quantity(&hurt_left), Some(1));
        assert_eq!(quantity(&hurt_right), Some(1));
        for game in [&mut left, &mut right] {
            game.admit_intention(
                ActorId(1),
                action(request(game, ActorId(1))),
                IntentionOrigin::Human,
            )
            .unwrap();
        }
        let a = execute_for(&mut left, ActorId(1));
        let b = execute_for(&mut right, ActorId(1));
        assert_eq!(a, b);
        assert_eq!(left.preparation(ActorId(1)), right.preparation(ActorId(1)));
        assert_eq!(quantity(&left), Some(1));
        assert_eq!(quantity(&right), Some(1));
        let due = left.tick() + 10;
        wait_until(&mut left, due);
        wait_until(&mut right, due);
        assert!(left.preparation(ActorId(1)).is_none());
        assert!(right.preparation(ActorId(1)).is_none());
        assert_eq!(quantity(&left), None);
        assert_eq!(quantity(&right), None);
        assert_eq!(
            left.observe(ActorId(1)).unwrap(),
            right.observe(ActorId(1)).unwrap()
        );
        assert_eq!(left.combat_events(), right.combat_events());
    }
}

#[test]
fn phase5_respec_visible_danger_discards_active_and_paused_attempts_and_retry_is_full() {
    for paused in [false, true] {
        let mut game = fixture_game();
        let enemy = add_enemy(&mut game);
        let actor = ActorId(1);
        start(&mut game);
        wait_until(&mut game, 4);
        if paused {
            game.pause_preparation(actor).unwrap();
        }
        game.teleport(enemy, at(3)).unwrap();
        assert!(game
            .observe(actor)
            .unwrap()
            .visible_actors
            .iter()
            .any(|entry| entry.id == enemy));
        game.act(game.next_actor().unwrap(), Action::Wait).unwrap();
        assert!(game.preparation(actor).is_none());
        assert_eq!(quantity(&game), Some(1));
        let before = game.clone();
        assert!(game
            .admit_intention(actor, action(request(&game, actor)), IntentionOrigin::Human)
            .is_err());
        assert_eq!(game, before);
        game.teleport(enemy, hidden_location()).unwrap();
        while game.next_actor() != Some(actor) {
            game.act(game.next_actor().unwrap(), Action::Wait).unwrap();
        }
        start(&mut game);
        assert_eq!(game.preparation(actor).unwrap().remaining, 10);
        assert_eq!(round_trip(&game), game);
    }
}

#[test]
fn phase5_respec_uses_current_earned_recovery_and_read_only_preview_never_rewinds_it() {
    let mut game = fixture_custom(|document| {
        document["health_recovery"] =
            json!({"base_period":10,"minimum_period":10,"reduction_per_hd":0})
    });
    let actor = ActorId(1);
    game.apply_effects(
        actor,
        &[super::abilities::damage(
            4,
            tor_simulation::combat::DamageType::Vital,
        )],
    )
    .unwrap();
    wait_until(&mut game, 6);
    while game.next_actor() != Some(actor) {
        game.act(game.next_actor().unwrap(), Action::Wait).unwrap();
    }
    let proposal = action(request(&game, actor));
    let Action::Respec {
        basis, replacement, ..
    } = &proposal
    else {
        unreachable!()
    };
    let before = game.clone();
    let preview = game
        .preview_player_replacement(actor, basis, replacement)
        .unwrap();
    assert_eq!(game, before);
    assert_eq!(preview.runtime().health.injury(), 4);
    let started = game.tick();
    start(&mut game);
    let mut replay = round_trip(&game);
    for subject in [&mut game, &mut replay] {
        wait_until(subject, started + 10);
    }
    assert_eq!(game, replay);
    assert!(game.preparation(actor).is_none());
    let current = game.creature(actor).unwrap();
    assert_eq!(current.health().injury(), 3);
    assert_eq!(current.health_recovery().elapsed(), 6);
    assert_eq!(current.received_health_loss_remaining(), 0);
    assert_eq!(quantity(&game), None);
}

fn lethal_replacement() -> (Game, Action) {
    let mut game = super::player_advancement::game_with_runtime_sources();
    let actor = ActorId(1);
    let basis = request(&game, actor)["value"]["basis"].clone();
    game.act(actor,serde_json::from_value(json!({"type":"purchase","value":{"basis":basis,"purchase":{"owner":0,"pool":"advancement","selection":{"kind":"skill_rank","skill":"perception"}}}})).unwrap()).unwrap();
    game.apply_condition(
        actor,
        &tor_simulation::content_ids::ConditionId::new("fear").unwrap(),
        ActorId(2),
        10000,
    )
    .unwrap();
    let remaining = game.health(actor).unwrap().0;
    fixture::injure(&mut game, actor, u32::try_from(remaining - 5).unwrap()).unwrap();
    let mut spec = ItemSpec::ordinary("last ration".into());
    spec.supply = Some(tor_simulation::content_ids::SupplyId::new("ration").unwrap());
    game.place_item_stack(10, at(1), Some(actor), 1, spec)
        .unwrap();
    wait_until(&mut game, 600);
    while game.next_actor() != Some(actor) {
        game.act(game.next_actor().unwrap(), Action::Wait).unwrap();
    }
    let mut request = request(&game, actor);
    request["value"]["replacement"]["replacement"] =
        json!({"pool":"advancement","selection":{"kind":"talent","talent":"lethal_choice"}});
    (game, action(request))
}
#[test]
fn phase5_respec_lethal_last_ration_finalizes_only_post_spend_inventory_once() {
    use tor_simulation::{combat::CombatEvent, resources::Resource};
    let (mut game, proposal) = lethal_replacement();
    let actor = ActorId(1);
    let Action::Respec {
        basis, replacement, ..
    } = &proposal
    else {
        unreachable!()
    };
    let original = game.clone();
    let preview = game
        .preview_player_replacement(actor, basis, replacement)
        .unwrap();
    assert!(preview.runtime().health.dead());
    assert_eq!(game, original);
    let original_epoch = game.creature(actor).unwrap().build_epoch();
    let origin = game
        .admit_intention(actor, proposal.clone(), IntentionOrigin::Human)
        .unwrap();
    execute_for(&mut game, actor);
    assert_eq!(quantity(&game), Some(1));
    let due = game.tick() + 600;
    let mut replay = round_trip(&game);
    for subject in [&mut game, &mut replay] {
        wait_until(subject, due);
        assert!(!subject.alive(actor));
        assert!(subject.preparation(actor).is_none());
        let after = subject.creature(actor).unwrap();
        assert_eq!(after.build_epoch().get(), original_epoch.get() + 1);
        assert_eq!(after.health(), preview.runtime().health);
        assert_eq!(after.health_recovery(), preview.runtime().health_recovery);
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
        assert!(subject.equipment(actor).unwrap().is_empty());
        let view = subject.observe(ActorId(2)).unwrap();
        assert!(
            view.ground_items.iter().all(|item| item.id != ItemId(10)),
            "the deleted ration cannot be dropped by death cleanup"
        );
        assert!(view.ground_items.iter().any(|item| item.id == ItemId(100)));
        assert_eq!(
            view.ground_items
                .iter()
                .filter(|item| item.class == tor_simulation::ItemClass::Corpse)
                .count(),
            1
        );
        assert_eq!(
            subject
                .combat_events()
                .iter()
                .filter(|event| matches!(event,CombatEvent::Died {actor:dead} if *dead==actor))
                .count(),
            1
        );
        assert_eq!(subject.combat_events().iter().filter(|event|matches!(event,CombatEvent::RespecCompleted {actor:owner,intention:Some(id),..} if *owner==actor && *id==origin)).count(),1);
        assert_eq!(round_trip(subject), *subject);
    }
    assert_eq!(game, replay);
}
#[test]
fn phase5_respec_lethal_preview_and_exhausted_action_boundary_preserve_ration_atomically() {
    let (game, proposal) = lethal_replacement();
    let mut shared = tor_simulation::checkpoint::SharedState::default();
    let mut snapshot = serde_json::to_value(game.checkpoint(&mut shared)).unwrap();
    snapshot["next_item_id"] = json!(u64::MAX);
    let mut exhausted = Game::restore_checkpoint(
        serde_json::from_value(snapshot).unwrap(),
        &shared,
        game.content().clone(),
    )
    .unwrap();
    let before = exhausted.clone();
    let Action::Respec {
        basis, replacement, ..
    } = &proposal
    else {
        unreachable!()
    };
    assert_eq!(
        exhausted
            .preview_player_replacement(ActorId(1), basis, replacement)
            .map(|_| ()),
        Err(tor_simulation::GameError::IdentityExhausted)
    );
    assert_eq!(
        exhausted.admit_intention(ActorId(1), proposal, IntentionOrigin::Human),
        Err(tor_simulation::GameError::IdentityExhausted)
    );
    assert_eq!(exhausted, before);
    assert_eq!(quantity(&exhausted), Some(1));
}
#[test]
fn phase5_respec_never_abandons_an_existing_paid_preparation_to_bypass_debt() {
    let mut game = super::player_advancement::game_with_runtime_sources();
    let actor = ActorId(1);
    let basis = request(&game, actor)["value"]["basis"].clone();
    game.act(actor,serde_json::from_value(json!({"type":"purchase","value":{"basis":basis,"purchase":{"owner":0,"pool":"advancement","selection":{"kind":"skill_rank","skill":"perception"}}}})).unwrap()).unwrap();
    wait_until(&mut game, 600);
    while game.next_actor() != Some(actor) {
        game.act(game.next_actor().unwrap(), Action::Wait).unwrap();
    }
    let id = game
        .admit_intention(
            actor,
            Action::UseAbility {
                id: tor_simulation::content_ids::AbilityId::new("basic_melee").unwrap(),
                invocation: tor_simulation::abilities::AbilityInvocation::actor_target(ActorId(2)),
                supplies: Default::default(),
            },
            IntentionOrigin::Human,
        )
        .unwrap();
    execute_for(&mut game, actor);
    assert!(game
        .creature(actor)
        .unwrap()
        .costs()
        .reservation(id)
        .is_some());
    for paused in [false, true] {
        if paused {
            game.pause_preparation(actor).unwrap();
        }
        let before = game.clone();
        assert_eq!(
            game.admit_intention(actor, action(request(&game, actor)), IntentionOrigin::Human),
            Err(tor_simulation::GameError::ActorBusy)
        );
        assert_eq!(game, before);
        assert!(game
            .creature(actor)
            .unwrap()
            .costs()
            .reservation(id)
            .is_some());
    }
}
fn shifted_clock(game: &Game, tick: u64) -> Game {
    let mut shared = tor_simulation::checkpoint::SharedState::default();
    let mut snapshot = serde_json::to_value(game.checkpoint(&mut shared)).unwrap();
    let mut pool = serde_json::to_value(shared).unwrap();
    assert_eq!(game.tick(), 0);
    snapshot["tick"] = json!(tick);
    for table in pool["actors"].as_array_mut().unwrap() {
        for actor in table.as_object_mut().unwrap().values_mut() {
            actor["ready_at"] = json!(tick);
            actor["active_since"] = json!(tick);
        }
    }
    Game::restore_checkpoint(
        serde_json::from_value(snapshot).unwrap(),
        &serde_json::from_value(pool).unwrap(),
        game.content().clone(),
    )
    .unwrap()
}
#[test]
fn phase5_respec_checks_full_prepare_and_recovery_horizon_before_holding() {
    let base = fixture_game();
    let actor = ActorId(1);
    let mut overflow = shifted_clock(&base, u64::MAX - 12);
    let before = overflow.clone();
    assert_eq!(
        overflow.admit_intention(
            actor,
            action(request(&overflow, actor)),
            IntentionOrigin::Human
        ),
        Err(tor_simulation::GameError::Schedule(
            tor_simulation::ScheduleError::TimeExhausted
        ))
    );
    assert_eq!(overflow, before);
    assert_eq!(quantity(&overflow), Some(1));
    let mut exact = shifted_clock(&base, u64::MAX - 13);
    start(&mut exact);
    assert_eq!(quantity(&exact), Some(1));
    wait_until(&mut exact, u64::MAX - 3);
    assert!(exact.preparation(actor).is_none());
    assert_eq!(quantity(&exact), None);
    assert_eq!(
        exact
            .observe(actor)
            .unwrap()
            .combat
            .unwrap()
            .recovery_remaining,
        3
    );
    assert_eq!(round_trip(&exact), exact);
}
#[test]
fn phase5_respec_ration_policy_is_required_known_one_unit_and_commit_only() {
    let original = serde_json::to_value(fixture::ruleset().document()).unwrap();
    for (field, value) in [
        ("quantity", json!(0)),
        ("quantity", json!(2)),
        ("spend_at", json!("first_resolution")),
        ("supply", json!("unknown_ration")),
    ] {
        let mut bad = original.clone();
        bad["advancement"]["respec_ration"][field] = value;
        assert!(Ruleset::validate(serde_json::from_value(bad).unwrap()).is_err());
    }
    let mut missing = original;
    missing["advancement"]
        .as_object_mut()
        .unwrap()
        .remove("respec_ration");
    assert!(serde_json::from_value::<tor_simulation::rules::RulesetDocument>(missing).is_err());
}

#[test]
fn phase5_respec_fresh_identical_admission_cannot_resume_paused_work() {
    let mut game = fixture_game();
    let actor = ActorId(1);
    let proposal = action(request(&game, actor));
    let origin = start(&mut game);
    wait_until(&mut game, 4);
    game.pause_preparation(actor).unwrap();
    let before = game.clone();
    assert_eq!(game.preparation(actor).unwrap().remaining, 6);
    assert_eq!(
        game.admit_intention(actor, proposal, IntentionOrigin::Human),
        Err(tor_simulation::GameError::ActorBusy)
    );
    assert_eq!(
        game, before,
        "rejection preserves identity allocation, hold, progress and lineage"
    );
    let mut replay = round_trip(&game);
    for subject in [&mut game, &mut replay] {
        subject.resume_intention(actor, origin).unwrap();
        execute_for(subject, actor);
        assert_eq!(
            subject.preparation(actor).unwrap().origin_intention(),
            Some(origin)
        );
        assert_eq!(subject.preparation(actor).unwrap().remaining, 6);
    }
    assert_eq!(game, replay);
}
#[test]
fn phase5_respec_fresh_restored_action_cannot_execute_as_resume_preparation() {
    let mut game = fixture_game();
    let actor = ActorId(1);
    let proposal = action(request(&game, actor));
    let origin = start(&mut game);
    wait_until(&mut game, 4);
    game.pause_preparation(actor).unwrap();
    let queued = game
        .admit_intention(actor, Action::Wait, IntentionOrigin::Human)
        .unwrap();
    assert_ne!(queued, origin);
    assert_eq!(round_trip(&game), game);
    let mut shared = tor_simulation::checkpoint::SharedState::default();
    let snapshot = game.checkpoint(&mut shared);
    let mut value = serde_json::to_value((snapshot, shared)).unwrap();
    let entry = value[0]["intentions"]["entries"][0].as_u64().unwrap() as usize;
    value[1]["intentions"]["entries"][entry]["work"] =
        serde_json::to_value(tor_simulation::IntentionWork::Action(proposal)).unwrap();
    let (snapshot, shared) = serde_json::from_value(value).unwrap();
    let mut restored = Game::restore_checkpoint(snapshot, &shared, game.content().clone()).unwrap();
    let pending = restored.preparation(actor).unwrap().clone();
    while restored.next_intention_actor() != Some(actor) {
        restored
            .act(restored.next_actor().unwrap(), Action::Wait)
            .unwrap();
    }
    let result = restored.execute_next_intention().unwrap();
    assert_eq!(result.intention.id, queued);
    assert_eq!(result.outcome, Err(tor_simulation::GameError::ActorBusy));
    assert_eq!(restored.preparation(actor), Some(&pending));
    assert_eq!(quantity(&restored), Some(1));
    restored.resume_intention(actor, origin).unwrap();
    execute_for(&mut restored, actor);
    assert_eq!(restored.preparation(actor).unwrap().remaining, 6);
}
// The clock projection regressions use existing public observations as their oracle.
fn projection_subject(game: &mut Game) -> ActorId {
    game.add_region(tor_world::Region {
        id: RegionId(3),
        name: "isolated clock".into(),
        bounds: tor_world::Extent::new(8, 3, 1).unwrap(),
    })
    .unwrap();
    let actor = game
        .spawn_actor(hidden_location(), NonZeroU64::new(100).unwrap())
        .unwrap();
    game.configure_creature(
        actor,
        CreatureIdentity {
            name: "clock subject".into(),
            faction: "allies".into(),
        },
        game.creature(ActorId(1)).unwrap().build().clone(),
    )
    .unwrap();
    let mut ration = ItemSpec::ordinary("clock ration".into());
    ration.supply = Some(tor_simulation::content_ids::SupplyId::new("ration").unwrap());
    game.place_item_stack(11, hidden_location(), Some(actor), 1, ration)
        .unwrap();
    actor
}
fn projection_freeze(game: &mut Game, active: bool) {
    use std::collections::BTreeSet;
    let active = if active {
        BTreeSet::from([RegionId(1), RegionId(3)])
    } else {
        BTreeSet::from([RegionId(1)])
    };
    let (applied, _) = game
        .transition_regions(
            &tor_simulation::RegionTransition {
                active: active.clone(),
                loaded: BTreeSet::from([RegionId(1), RegionId(2), RegionId(3)]),
            },
            &mut tor_simulation::MemoryRecords::default(),
        )
        .unwrap();
    assert_eq!(
        applied.active.contains(&RegionId(3)),
        active.contains(&RegionId(3))
    );
    assert_eq!(
        game.region_state(RegionId(3)),
        Some(if active.contains(&RegionId(3)) {
            tor_simulation::RegionState::Active
        } else {
            tor_simulation::RegionState::Frozen
        })
    );
}
#[test]
fn phase5_respec_projection_uses_actor_active_time_when_active_paused_frozen_and_reopened() {
    for paused in [false, true] {
        let mut game = fixture_game();
        let actor = projection_subject(&mut game);
        game.admit_intention(actor, action(request(&game, actor)), IntentionOrigin::Human)
            .unwrap();
        execute_for(&mut game, actor);
        wait_until(&mut game, 4);
        let view = game.observe(actor).unwrap().combat.unwrap();
        assert_eq!(view.preparation_remaining, Some(6));
        assert!(view.preparation_active);
        if paused {
            game.pause_preparation(actor).unwrap();
            // Explicit pause pins its input boundary active. A real ordinary
            // response acknowledges that boundary without resuming the work.
            while game.next_actor() != Some(actor) {
                game.act(game.next_actor().unwrap(), Action::Wait).unwrap();
            }
            game.act(actor, Action::Wait).unwrap();
            assert!(!game.preparation(actor).unwrap().active);
            assert_eq!(game.preparation(actor).unwrap().remaining, 6);
        }
        projection_freeze(&mut game, false);
        let before = game.clone();
        assert_eq!(
            game.observe(actor)
                .unwrap()
                .combat
                .unwrap()
                .preparation_remaining,
            Some(6)
        );
        assert_eq!(game, before);
        wait_until(&mut game, 12);
        let mut reopened = round_trip(&game);
        for subject in [&mut game, &mut reopened] {
            let before = subject.clone();
            let view = subject.observe(actor).unwrap().combat.unwrap();
            let timing = subject.preparation_timing(actor).unwrap();
            assert_eq!(timing.preparation_remaining, view.preparation_remaining);
            assert_eq!(timing.preparation_active, view.preparation_active);
            assert_eq!(timing.recovery_remaining, view.recovery_remaining);
            assert_eq!(
                view.preparation_remaining,
                Some(6),
                "frozen actor accrues no preparation time"
            );
            assert_eq!(view.preparation_active, !paused);
            assert_eq!(*subject, before);
            projection_freeze(subject, true);
            wait_until(subject, 14);
            assert_eq!(
                subject
                    .observe(actor)
                    .unwrap()
                    .combat
                    .unwrap()
                    .preparation_remaining,
                Some(if paused { 6 } else { 4 })
            );
        }
        assert_eq!(game, reopened);
    }
}
#[test]
fn phase5_respec_projection_keeps_frozen_recovery_until_thaw_and_reopen() {
    let mut game = fixture_game();
    let actor = projection_subject(&mut game);
    while game.next_actor() != Some(actor) {
        game.act(game.next_actor().unwrap(), Action::Wait).unwrap();
    }
    game.act(actor, Action::Wait).unwrap();
    wait_until(&mut game, 4);
    assert_eq!(
        game.observe(actor)
            .unwrap()
            .combat
            .unwrap()
            .recovery_remaining,
        96
    );
    projection_freeze(&mut game, false);
    wait_until(&mut game, 12);
    let mut reopened = round_trip(&game);
    for subject in [&mut game, &mut reopened] {
        let before = subject.clone();
        let view = subject.observe(actor).unwrap().combat.unwrap();
        let timing = subject.preparation_timing(actor).unwrap();
        assert_eq!(timing.preparation_remaining, view.preparation_remaining);
        assert_eq!(timing.preparation_active, view.preparation_active);
        assert_eq!(timing.recovery_remaining, view.recovery_remaining);
        assert_eq!(
            view.recovery_remaining, 96,
            "frozen actor accrues no recovery time"
        );
        assert!(view.preparation_remaining.is_none());
        assert_eq!(*subject, before);
        projection_freeze(subject, true);
        wait_until(subject, 14);
        assert_eq!(
            subject
                .observe(actor)
                .unwrap()
                .combat
                .unwrap()
                .recovery_remaining,
            94
        );
    }
    assert_eq!(game, reopened);
}

#[test]
fn phase5_respec_enclosing_work_named_record_rejects_valid_positional_alias() {
    let game = fixture_game();
    let proposal = action(request(&game, ActorId(1)));
    let Action::Respec {
        basis,
        replacement,
        supplies,
    } = proposal
    else {
        unreachable!()
    };
    let work = tor_simulation::Work::Respec {
        basis: basis.clone(),
        replacement: replacement.clone(),
        supplies: supplies.clone(),
    };
    let named = serde_json::to_value(&work).unwrap();
    assert_eq!(
        serde_json::from_value::<tor_simulation::Work>(named).unwrap(),
        work
    );
    assert!(
        serde_json::from_value::<tor_simulation::Work>(json!([
            "respec",
            basis,
            replacement,
            supplies
        ]))
        .is_err(),
        "new saved Respec Work requires a named record"
    );
}
#[test]
fn phase5_respec_enclosing_completion_named_record_rejects_valid_positional_checkpoint_alias() {
    let mut game = fixture_game();
    start(&mut game);
    wait_until(&mut game, 10);
    let event = game
        .combat_events()
        .iter()
        .find(|event| {
            matches!(
                event,
                tor_simulation::combat::CombatEvent::RespecCompleted { .. }
            )
        })
        .unwrap()
        .clone();
    let tor_simulation::combat::CombatEvent::RespecCompleted {
        actor,
        basis,
        replacement,
        intention,
    } = &event
    else {
        unreachable!()
    };
    let mut shared = tor_simulation::checkpoint::SharedState::default();
    let snapshot = game.checkpoint(&mut shared);
    let value = serde_json::to_value(&snapshot).unwrap();
    let valid_event: tor_simulation::combat::CombatEvent =
        serde_json::from_slice(&serde_json::to_vec(&event).unwrap()).unwrap();
    assert_eq!(valid_event, event);
    let valid_snapshot = serde_json::from_slice(&serde_json::to_vec(&value).unwrap()).unwrap();
    assert_eq!(
        Game::restore_checkpoint(valid_snapshot, &shared, game.content().clone()).unwrap(),
        game
    );
    let mut bad = value;
    let events = bad["events"].as_array_mut().unwrap();
    let index = events
        .iter()
        .position(|value| value.get("RespecCompleted").is_some())
        .unwrap();
    let bad_event = json!({"RespecCompleted":[actor,basis,replacement,intention]});
    events[index] = bad_event.clone();
    assert!(
        serde_json::from_value::<tor_simulation::checkpoint::Snapshot>(bad.clone()).is_err(),
        "Value's struct-variant reader requires an object"
    );
    let direct = serde_json::from_slice::<tor_simulation::combat::CombatEvent>(
        &serde_json::to_vec(&bad_event).unwrap(),
    );
    let checkpoint = serde_json::from_slice::<tor_simulation::checkpoint::Snapshot>(
        &serde_json::to_vec(&bad).unwrap(),
    );
    assert!(
        direct.is_err() && checkpoint.is_err(),
        "completion byte readers require named fields: direct accepted={}, checkpoint accepted={}",
        direct.is_ok(),
        checkpoint.is_ok()
    );
}

#[test]
fn phase5_respec_nested_finite_records_require_named_shapes() {
    let mut game = fixture_game();
    let original = request(&game, ActorId(1));
    for selection in [
        json!(["carried"]),
        json!(["explicit",[{"cost_ordinal":0,"item":10}]]),
    ] {
        let mut bad = original.clone();
        bad["value"]["supplies"] = selection;
        assert!(serde_json::from_value::<Action>(bad).is_err());
    }
    let mut exact = original;
    exact["value"]["supplies"] = json!({"kind":"explicit","items":[{"cost_ordinal":0,"item":10}]});
    game.admit_intention(ActorId(1), action(exact), IntentionOrigin::Human)
        .unwrap();
    execute_for(&mut game, ActorId(1));
    assert_eq!(round_trip(&game), game);
    let mut plan = serde_json::to_value(&game.preparation(ActorId(1)).unwrap().plan).unwrap();
    plan["supplies"] = json!(["explicit",[{"cost_ordinal":0,"item":10}]]);
    assert!(serde_json::from_value::<tor_simulation::abilities::Plan>(plan).is_err());
    let mut shared = tor_simulation::checkpoint::SharedState::default();
    let snapshot = game.checkpoint(&mut shared);
    let mut value = serde_json::to_value((snapshot, shared)).unwrap();
    let actors = value[0]["actors"].as_u64().unwrap() as usize;
    value[1]["actors"][actors]["1"]["pending"]["item_payment"] =
        json!(["held",[{"cost_ordinal":0,"item":10}]]);
    assert!(serde_json::from_value::<(
        tor_simulation::checkpoint::Snapshot,
        tor_simulation::checkpoint::SharedState
    )>(value)
    .is_err());
}

#[test]
fn phase5_respec_boundary_pause_after_freeze_preserves_work_through_reopen_and_resume() {
    let mut game = fixture_game();
    let actor = projection_subject(&mut game);
    let origin = game
        .admit_intention(actor, action(request(&game, actor)), IntentionOrigin::Human)
        .unwrap();
    execute_for(&mut game, actor);
    let plan = game.preparation(actor).unwrap().plan.clone();
    let epoch = game.creature(actor).unwrap().build_epoch();
    wait_until(&mut game, 4);
    projection_freeze(&mut game, false);
    wait_until(&mut game, 12);
    assert_eq!(
        game.preparation_timing(actor)
            .unwrap()
            .preparation_remaining,
        Some(6)
    );
    game.suspend_intention(actor, origin).unwrap();
    let paused = game.preparation(actor).unwrap();
    assert!(!paused.active);
    assert_eq!(
        paused.remaining, 6,
        "a frozen actor has only performed four ticks"
    );
    assert_eq!(paused.plan, plan);
    assert_eq!(paused.origin_intention(), Some(origin));
    let mut reopened = round_trip(&game);
    for subject in [&mut game, &mut reopened] {
        projection_freeze(subject, true);
        assert_eq!(subject.tick(), 12);
        subject.resume_intention(actor, origin).unwrap();
        execute_for(subject, actor);
        assert_eq!(subject.tick(), 12, "thaw adds no new recovery debt");
        let resumed = subject.preparation(actor).unwrap();
        assert_eq!(resumed.remaining, 6);
        assert_eq!(resumed.plan, plan);
        assert_eq!(resumed.origin_intention(), Some(origin));
        wait_until(subject, 17);
        assert_eq!(
            subject
                .preparation_timing(actor)
                .unwrap()
                .preparation_remaining,
            Some(1)
        );
        assert_eq!(subject.creature(actor).unwrap().build_epoch(), epoch);
        wait_until(subject, 18);
        assert!(subject.preparation(actor).is_none());
        assert_eq!(
            subject.creature(actor).unwrap().build_epoch().get(),
            epoch.get() + 1
        );
        assert!(!subject
            .observe(actor)
            .unwrap()
            .inventory
            .iter()
            .any(|item| item.id == ItemId(11)));
    }
    assert_eq!(game, reopened);
}

#[test]
fn phase5_respec_boundary_damage_to_loaded_frozen_actor_discards_without_false_debt() {
    // The public resolved-effect API can target a loaded frozen actor. Ordinary
    // host reachability is a separate admission policy; core clock math is exact.
    let mut game = fixture_game();
    let actor = projection_subject(&mut game);
    let origin = game
        .admit_intention(actor, action(request(&game, actor)), IntentionOrigin::Human)
        .unwrap();
    execute_for(&mut game, actor);
    let build = game.creature(actor).unwrap().build().clone();
    wait_until(&mut game, 4);
    projection_freeze(&mut game, false);
    wait_until(&mut game, 12);
    assert_eq!(
        game.region_state(RegionId(3)),
        Some(tor_simulation::RegionState::Frozen)
    );
    game.apply_effects(
        actor,
        &[super::abilities::damage(
            1,
            tor_simulation::combat::DamageType::Vital,
        )],
    )
    .unwrap();
    assert!(game.preparation(actor).is_none());
    assert_eq!(game.creature(actor).unwrap().build(), &build);
    assert_eq!(
        game.creature(actor)
            .unwrap()
            .received_health_loss_remaining(),
        6
    );
    assert_eq!(
        game.preparation_timing(actor).unwrap().recovery_remaining,
        0,
        "discarding unpaid frozen work cannot add world-time recovery"
    );
    assert_eq!(
        game.combat_events()
            .iter()
            .filter(|event| matches!(event,
        tor_simulation::combat::CombatEvent::Interrupted { actor: target, intention: Some(id) }
        if *target == actor && *id == origin))
            .count(),
        1
    );
    let mut reopened = round_trip(&game);
    for subject in [&mut game, &mut reopened] {
        projection_freeze(subject, true);
        assert_eq!(
            subject
                .preparation_timing(actor)
                .unwrap()
                .recovery_remaining,
            0
        );
        assert_eq!(
            subject
                .creature(actor)
                .unwrap()
                .received_health_loss_remaining(),
            6
        );
        // Release must retire the held reference as well as the preparation.
        subject
            .admit_intention(
                actor,
                Action::Drop {
                    item: ItemId(11),
                    quantity: None,
                },
                IntentionOrigin::Human,
            )
            .unwrap();
    }
    assert_eq!(game, reopened);
}

#[test]
fn phase5_respec_boundary_byte_operands_reject_positional_paid_actions_in_both_tag_orders() {
    let mut game = fixture_game();
    let mut bandage = ItemSpec::ordinary("byte control bandage".into());
    bandage.supply = Some(tor_simulation::content_ids::SupplyId::new("bandage").unwrap());
    game.place_item_stack(12, at(1), Some(ActorId(1)), 1, bandage)
        .unwrap();
    let care = json!({"id":"bind_wounds", "invocation":{"aim":{"kind":"actor_target","target":1},"weapon":{"kind":"default"}}, "supplies":{"kind":"carried"}});
    let respec = request(&game, ActorId(1));
    let purchase = json!({"type":"purchase","value":{
        "basis":respec["value"]["basis"],
        "purchase":{"owner":0,"pool":"advancement","selection":{"kind":"skill_rank","skill":"athletics"}}
    }});
    let mut accepted = Vec::new();
    for (kind, value, fields) in [
        (
            "respec",
            respec["value"].clone(),
            vec!["basis", "replacement", "supplies"],
        ),
        (
            "purchase",
            purchase["value"].clone(),
            vec!["basis", "purchase"],
        ),
        ("use_ability", care, vec!["id", "invocation", "supplies"]),
    ] {
        for type_first in [true, false] {
            let encode = |value: &Value| {
                if type_first {
                    format!("{{\"type\":\"{kind}\",\"value\":{value}}}")
                } else {
                    format!("{{\"value\":{value},\"type\":\"{kind}\"}}")
                }
            };
            let valid = encode(&value);
            let control: Action = serde_json::from_slice(valid.as_bytes()).unwrap();
            if kind == "use_ability" {
                let mut candidate = game.clone();
                candidate
                    .admit_intention(ActorId(1), control.clone(), IntentionOrigin::Human)
                    .unwrap();
            }
            assert_eq!(
                serde_json::to_value(control).unwrap(),
                json!({"type":kind,"value":value})
            );
            let alias = Value::Array(fields.iter().map(|key| value[*key].clone()).collect());
            if serde_json::from_slice::<Action>(encode(&alias).as_bytes()).is_ok() {
                accepted.push((kind, type_first));
            }
        }
    }
    assert!(
        accepted.is_empty(),
        "positional action operands accepted: {accepted:?}"
    );
}

#[test]
fn phase5_respec_post_spend_corpse_exhaustion_rolls_back_control_advancement_and_replay() {
    let (mut game, proposal) = lethal_replacement();
    let actor = ActorId(1);
    let origin = game
        .admit_intention(actor, proposal, IntentionOrigin::Human)
        .unwrap();
    execute_for(&mut game, actor);
    let due = game.tick() + 600;
    assert_eq!(quantity(&game), Some(1));
    assert_eq!(
        serde_json::to_value(game.preparation(actor).unwrap()).unwrap()["item_payment"]["kind"],
        json!("held")
    );
    let mut shared = tor_simulation::checkpoint::SharedState::default();
    let mut snapshot = serde_json::to_value(game.checkpoint(&mut shared)).unwrap();
    let mut pool = serde_json::to_value(shared).unwrap();
    // A paused bounded-run checkpoint may retain another actor's future wait
    // deadline. An enabled checkpoint must instead be at its next decision.
    // Resuming the public run-control transaction reaches completion without
    // passing a fresh action's early allocator guard.
    let actors = snapshot["actors"].as_u64().unwrap() as usize;
    pool["actors"][actors]["2"]["ready_at"] = json!(due + 1);
    snapshot["combat"]["run"] = json!({"kind":"arena","state":{
        "limits":{"ticks":100000,"actions":10000},"started_at":game.tick(),"actions":0,
        "progress":{"kind":"active","control":{"kind":"paused","remaining":0}},"deceased":null
    }});
    let shared = serde_json::from_value(pool).unwrap();
    let restore = |snapshot: Value| {
        Game::restore_checkpoint(
            serde_json::from_value(snapshot).unwrap(),
            &shared,
            game.content().clone(),
        )
        .unwrap()
    };
    let mut control = restore(snapshot.clone());
    assert_eq!(round_trip(&control), control);
    assert!(!control.arena_execution_enabled());
    assert_eq!(control.next_actor(), None);
    assert_eq!(control.next_scheduled_actor(), Some(ActorId(2)));
    let epoch = control.creature(actor).unwrap().build_epoch();
    control.control_arena(false, 0).unwrap();
    assert_eq!(control.tick(), due);
    assert!(!control.alive(actor));
    assert_eq!(
        control.creature(actor).unwrap().build_epoch().get(),
        epoch.get() + 1
    );
    assert_eq!(quantity(&control), None);
    assert_eq!(
        control
            .combat_events()
            .iter()
            .filter(|event| matches!(event,
        tor_simulation::combat::CombatEvent::RespecCompleted { intention: Some(id), .. }
        if *id == origin))
            .count(),
        1
    );
    snapshot["next_item_id"] = json!(u64::MAX);
    let mut exhausted = restore(snapshot);
    let mut reopened = round_trip(&exhausted);
    for subject in [&mut exhausted, &mut reopened] {
        let before = subject.clone();
        assert_eq!(
            subject.control_arena(false, 0),
            Err(tor_simulation::GameError::IdentityExhausted)
        );
        assert_eq!(
            *subject, before,
            "late finalization failure rolls back Spend, events, epoch, timers and holds"
        );
        assert_eq!(quantity(subject), Some(1));
        assert_eq!(
            subject.preparation(actor).unwrap().origin_intention(),
            Some(origin)
        );
        assert_eq!(subject.creature(actor).unwrap().build_epoch(), epoch);
    }
    assert_eq!(exhausted, reopened);
}

#[test]
fn public_plan_owner_respec_and_idle_have_identical_paid_records_and_later_snapshots() {
    let mut idle = fixture_custom(|document| {
        document["advancement"]["respec_prepare"] = json!(1000);
        let mut ability = document["abilities"][0].clone();
        ability["id"] = json!("paid_plan_probe");
        ability["name"] = json!("Paid plan probe");
        ability["basic_attack"] = json!(false);
        ability["tags"] = json!([]);
        ability["cost"] = json!({"resource":"focus","start":1,"resolution":1});
        ability["supply_costs"] = json!([]);
        ability["cooldown"] = Value::Null;
        ability["reach"] = json!({"kind":"sight","range":6});
        ability["timing"] =
            json!({"source":{"kind":"fixed","prepare":100,"recover":100},"scaled_by":null});
        ability["strikes"] = json!({"kind":"authored","strikes":[{"offset":0,"unlock":null,"check":null,
            "on_success":[{"kind":"observe_public_plan"},{"kind":"apply_qualified_condition_to_source","condition":"readied","duration":300,"ability_tag":"weapon_attack"}],
            "on_failure":[],"descriptors":[],"riders":[]}]});
        document["abilities"].as_array_mut().unwrap().push(ability);
        document["constants"]["default_abilities"]
            .as_array_mut()
            .unwrap()
            .push(json!("paid_plan_probe"));
    });
    let mut private = idle.clone();
    let subject = ActorId(1);
    let observer = ActorId(2);
    let initial_focus = idle
        .creature(observer)
        .unwrap()
        .costs()
        .available(tor_simulation::resources::Resource::Focus);
    idle.act(subject, Action::Wait).unwrap();
    start(&mut private);
    assert!(idle.preparation(subject).is_none());
    assert!(matches!(
        &private.preparation(subject).unwrap().plan,
        tor_simulation::abilities::Plan::Respec(_)
    ));
    assert_eq!(idle.tick(), private.tick());
    assert_eq!(
        idle.observe(observer).unwrap(),
        private.observe(observer).unwrap(),
        "actual private Respec must not alter the observer's public evidence"
    );
    for game in [&mut idle, &mut private] {
        game.admit_intention(
            observer,
            Action::UseAbility {
                id: tor_simulation::content_ids::AbilityId::new("paid_plan_probe").unwrap(),
                invocation: tor_simulation::abilities::AbilityInvocation::actor_target(subject),
                supplies: Default::default(),
            },
            IntentionOrigin::Human,
        )
        .unwrap();
        execute_for(game, observer);
        for _ in 0..1000 {
            if game.public_plan_observation(observer).is_some() {
                break;
            }
            game.act(game.next_actor().unwrap(), Action::Wait).unwrap();
        }
        assert!(
            game.public_plan_observation(observer).is_some(),
            "the ordinary paid ability must finish"
        );
    }
    let record = idle.public_plan_observation(observer).unwrap().clone();
    assert_eq!(
        record.state,
        tor_simulation::public_plan::PublicPlanState::NoPublicPlan
    );
    assert_eq!(
        record.observed_at, 100,
        "the paid result is captured at the preparation boundary"
    );
    assert_eq!(
        idle.tick(),
        100,
        "the idle subject offers an input boundary at resolution"
    );
    assert_eq!(
        private.tick(),
        200,
        "the busy subject offers no earlier input boundary"
    );
    assert_eq!(private.public_plan_observation(observer), Some(&record));
    assert_eq!(
        serde_json::to_value(&record).unwrap(),
        json!({"observed_at":100,"subject_name":"owner1","state":{"kind":"no_public_plan"}}),
        "the paid result contains no private work label, operation, basis or timing"
    );
    assert_eq!(idle.tick() - record.observed_at, 0);
    assert_eq!(private.tick() - record.observed_at, 100);
    for game in [&idle, &private] {
        let timing = game.preparation_timing(observer).unwrap();
        assert_eq!(timing.preparation_remaining, None);
        assert_eq!(
            game.tick() + timing.recovery_remaining,
            200,
            "private work cannot change the observer's actual readiness deadline"
        );
        let creature = game.creature(observer).unwrap();
        assert_eq!(
            creature
                .costs()
                .available(tor_simulation::resources::Resource::Focus),
            initial_focus - 2
        );
        let readied = creature
            .conditions()
            .instances()
            .find(|instance| instance.condition.as_str() == "readied")
            .unwrap();
        assert_eq!(
            game.tick() + readied.remaining,
            400,
            "the 300-active-tick grant expires at the same deadline"
        );
        let qualification = readied.qualification.as_ref().unwrap();
        assert_eq!(qualification.opponent, subject);
        assert_eq!(qualification.ability_tag.as_str(), "weapon_attack");
    }
    assert_eq!(idle.observe(observer).unwrap().combat.unwrap().events, private.observe(observer).unwrap().combat.unwrap().events,
        "the actual paid resolution discloses the same event despite different scheduler yield points");
    // Advance through real ordinary subject inputs, never rewrite deadlines or
    // private state merely to align the comparison. These Wait inputs naturally
    // replace the last-action event list; the paid event was compared above.
    for _ in 0..1000 {
        if idle.tick() == private.tick() {
            break;
        }
        assert!(idle.tick() < private.tick());
        assert_eq!(idle.next_actor(), Some(subject));
        idle.act(subject, Action::Wait).unwrap();
    }
    assert_eq!(
        idle.tick(),
        private.tick(),
        "ordinary input boundaries must reach the same global tick"
    );
    assert_eq!(idle.public_plan_observation(observer), Some(&record));
    assert!(matches!(
        &private.preparation(subject).unwrap().plan,
        tor_simulation::abilities::Plan::Respec(_)
    ));
    assert_eq!(
        idle.creature(observer).unwrap(),
        private.creature(observer).unwrap(),
        "Focus payment, Readied qualification and recovery are identical"
    );
    let idle_view = idle.observe(observer).unwrap().combat.unwrap();
    let private_view = private.observe(observer).unwrap().combat.unwrap();
    assert_eq!(idle_view.public_plan, private_view.public_plan);
    assert_eq!(idle_view.own_stats, private_view.own_stats);
    assert_eq!(
        idle.preparation_timing(observer),
        private.preparation_timing(observer)
    );
    private.pause_preparation(subject).unwrap();
    let reopened = round_trip(&private);
    assert_eq!(reopened.public_plan_observation(observer), Some(&record));
    let before = reopened.clone();
    assert_eq!(
        reopened
            .observe(observer)
            .unwrap()
            .combat
            .unwrap()
            .public_plan
            .as_deref(),
        Some(&record)
    );
    assert_eq!(
        reopened, before,
        "later owner-private pause/reopen and pure inspection do not refresh the paid record"
    );
}

#[test]
fn edible_ration_and_replacement_compete_for_the_same_single_physical_unit() {
    for eat_first in [false, true] {
        let mut game = fixture_game();
        let actor = ActorId(1);
        if eat_first {
            game.admit_intention(
                actor,
                Action::Eat { item: ItemId(10) },
                IntentionOrigin::Human,
            )
            .unwrap();
            game.execute_next_intention().unwrap().outcome.unwrap();
            wait_until(&mut game, 1);
        } else {
            start(&mut game);
            wait_until(&mut game, 13);
        }
        assert_eq!(
            quantity(&game),
            None,
            "both paths spend the same finite item"
        );
        let requested = if eat_first {
            action(request(&game, actor))
        } else {
            Action::Eat { item: ItemId(10) }
        };
        let before = game.clone();
        assert!(game
            .admit_intention(actor, requested, IntentionOrigin::Human)
            .is_err());
        assert_eq!(
            game, before,
            "the other use cannot mint an independent ration balance"
        );
    }
}

#[test]
fn timed_respec_uses_current_losses_instead_of_resetting_them_from_its_permanent_preview() {
    use tor_simulation::attributes::{Attribute, Attributes, ByAttribute};
    use tor_simulation::rules::{AmountExpr, Effect};
    let mut game = fixture_game();
    let actor = ActorId(1);
    let mut build = game.creature(actor).unwrap().build().clone();
    build
        .set_initial_attributes(
            Attributes::new(ByAttribute {
                strength: 2,
                ..ByAttribute::default()
            })
            .unwrap(),
        )
        .unwrap();
    game.rebuild_creature(actor, build).unwrap();
    game.apply_effects(
        actor,
        &[
            Effect::LoseAttribute {
                attribute: Attribute::Strength,
                amount: AmountExpr::Fixed { amount: 1 },
            },
            Effect::LoseMaximumHealth {
                amount: AmountExpr::Fixed { amount: 2 },
            },
        ],
    )
    .unwrap();
    let before = game.creature(actor).unwrap().into_owned();
    start(&mut game);
    wait_until(&mut game, 4);
    game = round_trip(&game);
    wait_until(&mut game, 10);
    assert!(game.preparation(actor).is_none());
    let after = game.creature(actor).unwrap();
    assert_eq!(after.restorable_losses(), before.restorable_losses());
    assert_eq!(after.derived().attributes.get(Attribute::Strength), 1);
    assert_eq!(after.health(), before.health());
    assert_eq!(quantity(&game), None);
    assert_eq!(round_trip(&game), game);
}
