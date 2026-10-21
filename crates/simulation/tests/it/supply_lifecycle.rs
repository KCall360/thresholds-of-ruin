//! Ordinary finite-supply actions share atomic preparation and settlement.
use serde_json::{json, Value};
use std::{num::NonZeroU64, sync::Arc};
use tor_simulation::{
    attributes::{ManaBinding, Skill},
    content_ids::AbilityId,
    creatures::{Content, CreatureBuild},
    progression::{HdLedger, HdSource},
    rules::Ruleset,
    Action, ActorId, CreatureIdentity, Game, IntentionOrigin,
};
use tor_world::{Location, Position, RegionId};

fn ability() -> AbilityId {
    AbilityId::new("test_care").unwrap()
}
fn at(x: i32) -> Location {
    Location {
        region: RegionId(1),
        position: Position { x, y: 1, z: 0 },
    }
}
fn fixture(cooldown: Option<u64>, supplies: bool, reach: Value) -> (Game, ActorId, ActorId) {
    fixture_quantity(cooldown, u64::from(supplies), reach)
}
fn fixture_quantity(
    cooldown: Option<u64>,
    quantity: u64,
    reach: Value,
) -> (Game, ActorId, ActorId) {
    fixture_offsets(cooldown, quantity, reach, &[0])
}
fn fixture_offsets(
    cooldown: Option<u64>,
    quantity: u64,
    reach: Value,
    offsets: &[u64],
) -> (Game, ActorId, ActorId) {
    fixture_custom(cooldown, quantity, reach, offsets, |_| {})
}
fn fixture_custom(
    cooldown: Option<u64>,
    quantity: u64,
    reach: Value,
    offsets: &[u64],
    customize: impl FnOnce(&mut Value),
) -> (Game, ActorId, ActorId) {
    let original = super::creature_fixture::content();
    let mut document = serde_json::to_value(original.rules().document()).unwrap();
    let mut entry = document["abilities"][0].clone();
    entry["id"] = json!("test_care");
    entry["name"] = json!("Test Care");
    entry["basic_attack"] = json!(false);
    entry["cost"] = Value::Null;
    entry["cooldown"] = json!(cooldown);
    entry["supply_costs"] = if quantity > 0 {
        json!([{"supply":"bandage", "quantity":quantity, "access":"carried_or_ground_touch", "spend_at":"first_resolution"}])
    } else {
        json!([])
    };
    entry["reach"] = reach;
    entry["timing"] =
        json!({"source":{"kind":"fixed", "prepare":10, "recover":5}, "scaled_by":null});
    entry["strikes"] = json!({"kind":"authored", "strikes":[{
        "offset":0, "unlock":null, "check":null,
        "on_success":[{"kind":"heal", "amount":{"kind":"fixed", "amount":3}}],
        "on_failure":[], "descriptors":[], "riders":[]
    }]});
    let strike = entry["strikes"]["strikes"][0].clone();
    entry["strikes"]["strikes"] = offsets
        .iter()
        .map(|offset| {
            let mut row = strike.clone();
            row["offset"] = json!(offset);
            row
        })
        .collect();
    entry["ai"] = json!({"priority":101, "hostile":false, "minimum_resource_percent":0, "range_preference":"any"});
    customize(&mut entry);
    document["abilities"].as_array_mut().unwrap().push(entry);
    document["constants"]["default_abilities"]
        .as_array_mut()
        .unwrap()
        .push(json!("test_care"));
    let rules =
        Ruleset::validate(serde_json::from_value(document).expect("current care schema")).unwrap();
    let content = Content::new(rules, (*original.catalog()).clone()).unwrap();
    let mut game = Game::two_room_in_stone(42, content.clone());
    let mut ids = Vec::new();
    for x in [1, 2] {
        let actor = game
            .spawn_actor(at(x), NonZeroU64::new(1).unwrap())
            .unwrap();
        let build = CreatureBuild::new(
            Arc::clone(&content),
            super::creature_fixture::species().id,
            HdLedger::seeded(vec![HdSource::Racial], 42).unwrap(),
            ManaBinding::Intellect,
            [Skill::Deception, Skill::Persuasion],
        )
        .unwrap();
        game.configure_creature(
            actor,
            CreatureIdentity {
                name: format!("carer{x}"),
                faction: "allies".into(),
            },
            build,
        )
        .unwrap();
        ids.push(actor);
    }
    game.refresh_navigation();
    (game, ids[0], ids[1])
}
fn own_action(actor: ActorId) -> Action {
    serde_json::from_value(json!({"type":"use_ability", "value":{
        "id":"test_care", "invocation":{"aim":{"kind":"actor_target","target":actor},"weapon":{"kind":"default"}}, "supplies":{"kind":"carried"}
    }}))
    .expect("typed supply selection")
}
fn start(game: &mut Game, actor: ActorId) -> tor_simulation::IntentionId {
    let id = game
        .admit_intention(actor, own_action(actor), IntentionOrigin::Human)
        .unwrap();
    while game.next_intention_actor() != Some(actor) {
        game.act(game.next_actor().unwrap(), Action::Wait).unwrap();
    }
    game.execute_next_intention().unwrap().outcome.unwrap();
    id
}
fn remaining(game: &Game, actor: ActorId) -> u64 {
    game.creature(actor)
        .unwrap()
        .cooldowns()
        .remaining(&ability())
}

#[test]
fn cooldown_begins_only_at_preparation_and_cancel_does_not_refund_it() {
    let (mut game, actor, _) = fixture(Some(100), false, json!({"kind":"self_only"}));
    let original = game.clone();
    let plan = game.ability_plan(actor, &ability(), actor).unwrap();
    assert_eq!(game, original, "planning must not start a cooldown");
    assert_eq!(serde_json::to_value(plan).unwrap()["cooldown"], json!(100));
    let id = game
        .admit_intention(actor, own_action(actor), IntentionOrigin::Human)
        .unwrap();
    assert_eq!(
        remaining(&game, actor),
        0,
        "queueing must not start a cooldown"
    );
    game.execute_next_intention().unwrap().outcome.unwrap();
    assert_eq!(remaining(&game, actor), 100);
    game.cancel_intention(actor, id).unwrap();
    assert_eq!(remaining(&game, actor), 100);
    let original = game.clone();
    assert!(game
        .admit_intention(actor, own_action(actor), IntentionOrigin::Human)
        .is_err());
    assert_eq!(game, original, "cooling-down rejection is atomic");
}

#[test]
fn resume_retains_origin_and_does_not_restart_a_live_cooldown() {
    let (mut game, actor, _) = fixture(Some(100), false, json!({"kind":"self_only"}));
    let id = start(&mut game, actor);
    game.pause_preparation(actor).unwrap();
    for _ in 0..6 {
        game.act(game.next_actor().unwrap(), Action::Wait).unwrap();
    }
    let before = remaining(&game, actor);
    assert!((1..100).contains(&before));
    game.resume_intention(actor, id).unwrap();
    while game.next_intention_actor() != Some(actor) {
        game.act(game.next_actor().unwrap(), Action::Wait).unwrap();
    }
    let now = remaining(&game, actor);
    game.execute_next_intention().unwrap().outcome.unwrap();
    assert_eq!(remaining(&game, actor), now);
    assert_eq!(
        game.preparation(actor).unwrap().origin_intention(),
        Some(id)
    );
}

#[test]
fn self_or_ally_touch_accepts_self_and_same_faction_without_hostility() {
    let (game, actor, ally) = fixture(
        None,
        false,
        json!({"kind":"self_or_ally", "range":{"kind":"touch"}}),
    );
    assert!(game.ability_plan(actor, &ability(), actor).is_ok());
    assert!(game.ability_plan(actor, &ability(), ally).is_ok());
}

fn bandages(game: &mut Game, id: u64, owner: Option<ActorId>, quantity: u64, x: i32) {
    let mut spec =
        serde_json::to_value(tor_simulation::ItemSpec::ordinary("bandage".into())).unwrap();
    spec["supply"] = json!("bandage");
    spec["stackable"] = json!(true);
    game.place_item_stack(
        id,
        at(x),
        owner,
        quantity,
        serde_json::from_value(spec).unwrap(),
    )
    .unwrap();
}
fn explicit_action(actor: ActorId, item: u64) -> Action {
    serde_json::from_value(json!({"type":"use_ability", "value":{
        "id":"test_care", "invocation":{"aim":{"kind":"actor_target","target":actor},"weapon":{"kind":"default"}},
        "supplies":{"kind":"explicit", "items":[{"cost_ordinal":0,"item":item}]}
    }}))
    .unwrap()
}
fn quantity(game: &Game, observer: ActorId, item: u64) -> Option<u64> {
    let view = game.observe(observer).unwrap();
    view.inventory
        .iter()
        .find(|entry| entry.id.0 == item)
        .map(|entry| entry.quantity)
        .or_else(|| {
            view.ground_items
                .iter()
                .find(|entry| entry.id.0 == item)
                .map(|entry| entry.quantity)
        })
}
fn round_trip(game: &Game) -> Game {
    let mut shared = tor_simulation::checkpoint::SharedState::default();
    let checkpoint = game.checkpoint(&mut shared);
    let (checkpoint, shared) =
        serde_json::from_slice(&serde_json::to_vec(&(checkpoint, shared)).unwrap()).unwrap();
    Game::restore_checkpoint(checkpoint, &shared, game.content().clone()).unwrap()
}

#[test]
fn last_ground_bandage_has_one_atomic_winner_and_no_loser_cooldown() {
    let (mut game, first, second) = fixture(Some(100), true, json!({"kind":"self_only"}));
    game.teleport(second, at(3)).unwrap();
    bandages(&mut game, 10, None, 1, 2);
    for actor in [first, second] {
        game.admit_intention(actor, explicit_action(actor, 10), IntentionOrigin::Human)
            .unwrap();
    }
    game.execute_next_intention().unwrap().outcome.unwrap();
    assert_eq!(
        quantity(&game, first, 10),
        Some(1),
        "preparation holds rather than spends"
    );
    let lost = game.execute_next_intention().unwrap();
    assert_eq!(lost.intention.actor, second);
    assert!(lost.outcome.is_err());
    assert!(game.preparation(second).is_none());
    assert_eq!(remaining(&game, second), 0);
    assert_eq!(remaining(&game, first), 100);
    assert_eq!(round_trip(&game), game);
    while game.preparation(first).is_some() {
        game.act(game.next_actor().unwrap(), Action::Wait).unwrap();
    }
    assert_eq!(quantity(&game, first, 10), None);
}

#[test]
fn cancellation_releases_exact_hold_and_cooldown_still_blocks_same_ability() {
    let (mut game, actor, _) = fixture(Some(100), true, json!({"kind":"self_only"}));
    bandages(&mut game, 10, Some(actor), 2, 1);
    let id = start(&mut game, actor);
    game.pause_preparation(actor).unwrap();
    let before = game.clone();
    assert!(game
        .admit_intention(
            actor,
            Action::Drop {
                item: tor_simulation::ItemId(10),
                quantity: Some(2)
            },
            IntentionOrigin::Human
        )
        .is_err());
    assert_eq!(
        game, before,
        "transfer preflight cannot release its own pending hold"
    );
    assert_eq!(quantity(&game, actor, 10), Some(2));
    game.cancel_intention(actor, id).unwrap();
    assert_eq!(quantity(&game, actor, 10), Some(2));
    assert_eq!(remaining(&game, actor), 100);
    assert!(game
        .admit_intention(
            actor,
            Action::Drop {
                item: tor_simulation::ItemId(10),
                quantity: Some(2)
            },
            IntentionOrigin::Human
        )
        .is_ok());
}

#[test]
fn ordinary_planning_cannot_disclose_a_visible_stacks_held_quantity() {
    let (mut game, first, second) = fixture(None, true, json!({"kind":"self_only"}));
    game.teleport(second, at(3)).unwrap();
    bandages(&mut game, 10, None, 1, 2);
    let before = game.observe(second).unwrap().ground_items;
    game.admit_intention(first, explicit_action(first, 10), IntentionOrigin::Human)
        .unwrap();
    game.execute_next_intention().unwrap().outcome.unwrap();
    assert_eq!(game.observe(second).unwrap().ground_items, before);
    assert_eq!(quantity(&game, second, 10), Some(1));
}

#[test]
fn foreign_carried_and_remote_ground_supplies_never_satisfy_explicit_selection() {
    let (mut game, actor, other) = fixture(None, true, json!({"kind":"self_only"}));
    bandages(&mut game, 10, Some(other), 1, 2);
    bandages(&mut game, 11, None, 1, 4);
    let before = game.clone();
    for item in [10, 11, 99] {
        assert!(game
            .admit_intention(actor, explicit_action(actor, item), IntentionOrigin::Human)
            .is_err());
        assert_eq!(game, before);
    }
}

#[test]
fn fragmented_carried_cost_is_bounded_and_consumes_each_exact_fragment() {
    for count in [32, 33] {
        let (mut game, actor, _) = fixture_quantity(None, count, json!({"kind":"self_only"}));
        for item in 10..10 + count {
            bandages(&mut game, item, Some(actor), 1, 1);
        }
        let before = game.clone();
        if count == 33 {
            assert!(game
                .admit_intention(actor, own_action(actor), IntentionOrigin::Human)
                .is_err());
            assert_eq!(game, before);
        } else {
            start(&mut game, actor);
            assert_eq!(round_trip(&game), game);
            while game.preparation(actor).is_some() {
                game.act(game.next_actor().unwrap(), Action::Wait).unwrap();
            }
            for item in 10..10 + count {
                assert_eq!(quantity(&game, actor, item), None);
            }
        }
    }
}

#[test]
fn unknown_bandages_are_not_automatically_used_or_disclosed_as_supplies() {
    let (mut game, actor, _) = fixture(None, true, json!({"kind":"self_only"}));
    let mut spec = serde_json::to_value(tor_simulation::ItemSpec::ordinary(
        "strange wrapping".into(),
    ))
    .unwrap();
    spec["supply"] = json!("bandage");
    spec["concealed"] = json!(true);
    game.place_item_stack(
        10,
        at(1),
        Some(actor),
        1,
        serde_json::from_value(spec).unwrap(),
    )
    .unwrap();
    let before = game.clone();
    assert!(game.observe(actor).unwrap().inventory[0]
        .known_supply
        .is_none());
    assert!(game
        .admit_intention(actor, own_action(actor), IntentionOrigin::Human)
        .is_err());
    assert_eq!(game, before);
    game.identify_item(actor, tor_simulation::ItemId(10))
        .unwrap();
    start(&mut game, actor);
    assert!(game.preparation(actor).is_some());
}

#[test]
fn exact_holds_and_payment_tags_reject_corrupted_checkpoints() {
    let (mut game, actor, _) = fixture(None, true, json!({"kind":"self_only"}));
    bandages(&mut game, 10, Some(actor), 2, 1);
    bandages(&mut game, 11, Some(actor), 1, 1);
    start(&mut game, actor);
    let mut shared = tor_simulation::checkpoint::SharedState::default();
    let snapshot = game.checkpoint(&mut shared);
    let original = serde_json::to_value((snapshot, shared)).unwrap();
    let items = original[0]["items"].as_u64().unwrap() as usize;
    let actors = original[0]["actors"].as_u64().unwrap() as usize;
    let restore = |value: Value| -> bool {
        let Ok((snapshot, shared)) = serde_json::from_value(value) else {
            return false;
        };
        Game::restore_checkpoint(snapshot, &shared, game.content().clone()).is_ok()
    };
    assert!(restore(original.clone()));
    for mutation in 0..10 {
        let mut invalid = original.clone();
        let entry = &mut invalid[1]["items"][items]["10"];
        match mutation {
            0 => entry["holds"][0]["quantity"] = json!(0),
            1 => entry["holds"][0]["quantity"] = json!(3),
            2 => entry["holds"][0]["key"]["lineage"]["origin"] = json!(999999),
            3 => entry["holds"][0]["key"]["lineage"]["actor"] = json!(2),
            4 => entry["holds"][0]["key"]["cost_ordinal"] = json!(1),
            5 => entry["holds"] = json!([]),
            6 => {
                let hold = entry["holds"][0].clone();
                entry["holds"] = json!([hold.clone(), hold]);
            }
            7 => {
                invalid[1]["actors"][actors][actor.0.to_string()]["pending"]["item_payment"] =
                    json!({"kind":"settled"})
            }
            8 => {
                invalid[1]["actors"][actors][actor.0.to_string()]["pending"]["item_payment"]
                    ["refs"][0]["item"] = json!(11)
            }
            9 => {
                invalid[1]["actors"][actors][actor.0.to_string()]["pending"]["plan"]["supplies"] = json!({"kind":"explicit","items":[{"cost_ordinal":0,"item":10},{"cost_ordinal":0,"item":11}]})
            }
            _ => unreachable!(),
        }
        assert!(!restore(invalid), "accepted corruption {mutation}");
    }
}

#[test]
fn ordinary_ai_chooses_affordable_generic_self_heal_with_exact_visible_supply() {
    let (mut game, actor, _) = fixture(None, true, json!({"kind":"self_only"}));
    bandages(&mut game, 10, None, 1, 1);
    let maximum = game.creature(actor).unwrap().health().maximum();
    game.apply_effects(
        actor,
        &[super::abilities::damage(
            u32::try_from(maximum / 2 + 1).unwrap(),
            tor_simulation::combat::DamageType::Vital,
        )],
    )
    .unwrap();
    game.configure_ai(actor, tor_simulation::ai::AiProfile::general())
        .unwrap();
    game.admit_ai_intention(actor).unwrap();
    let execution = game.execute_next_intention().unwrap();
    execution.outcome.unwrap();
    let action = serde_json::to_value(execution.action.unwrap()).unwrap();
    assert_eq!(action["type"], json!("use_ability"));
    assert_eq!(action["value"]["id"], json!("test_care"));
    assert_eq!(
        action["value"]["invocation"],
        json!({"aim":{"kind":"actor_target","target":actor},"weapon":{"kind":"default"}})
    );
    assert_eq!(
        action["value"]["supplies"],
        json!({"kind":"explicit","items":[{"cost_ordinal":0,"item":10}]})
    );
}

fn drink_case(quantity_before: u64, lethal: bool, injured: bool) {
    let (mut game, actor, observer) = fixture(None, false, json!({"kind":"self_only"}));
    let maximum = game.creature(actor).unwrap().health().maximum();
    if injured {
        game.apply_effects(
            actor,
            &[super::abilities::damage(
                5,
                tor_simulation::combat::DamageType::Vital,
            )],
        )
        .unwrap();
    }
    let mut spec = tor_simulation::ItemSpec::ordinary("finite potion".into());
    spec.class = tor_simulation::ItemClass::Potion;
    spec.stackable = true;
    spec.consumable = Some(tor_simulation::ConsumableSpec {
        descriptors: vec![],
        discovery: tor_simulation::ItemDiscovery::SelfEffect,
        effects: vec![if lethal {
            super::abilities::damage(
                u32::try_from(maximum).unwrap(),
                tor_simulation::combat::DamageType::Vital,
            )
        } else {
            tor_simulation::rules::Effect::Heal {
                amount: tor_simulation::rules::AmountExpr::Fixed { amount: 3 },
            }
        }],
    });
    game.place_item_stack(10, at(1), Some(actor), quantity_before, spec)
        .unwrap();
    game.admit_intention(
        actor,
        Action::Drink {
            item: tor_simulation::ItemId(10),
        },
        IntentionOrigin::Human,
    )
    .unwrap();
    game.execute_next_intention().unwrap().outcome.unwrap();
    assert_eq!(quantity(&game, actor, 10), Some(quantity_before));
    assert_eq!(round_trip(&game), game);
    while game.preparation(actor).is_some() {
        game.act(game.next_actor().unwrap(), Action::Wait).unwrap();
    }
    assert_eq!(game.alive(actor), !lethal);
    assert_eq!(
        quantity(&game, if lethal { observer } else { actor }, 10),
        if quantity_before == 1 { None } else { Some(1) }
    );
    if !lethal {
        assert_eq!(
            game.creature(actor).unwrap().health().injury(),
            if injured { 2 } else { 0 }
        );
    }
    assert_eq!(round_trip(&game), game);
}

#[test]
fn last_unit_drink_heals_after_exact_source_spend() {
    drink_case(1, false, true);
}
#[test]
fn last_unit_drink_spends_when_full_health_prevents_change() {
    drink_case(1, false, false);
}
#[test]
fn last_unit_lethal_drink_dies_without_rereading_deleted_source() {
    drink_case(1, true, false);
}
#[test]
fn multiunit_lethal_drink_drops_only_the_unconsumed_remainder() {
    drink_case(2, true, false);
}

#[test]
fn another_actor_can_split_free_units_and_merge_them_back_without_rebinding_hold() {
    let (mut game, first, second) = fixture(None, true, json!({"kind":"self_only"}));
    bandages(&mut game, 10, None, 3, 2);
    game.admit_intention(first, explicit_action(first, 10), IntentionOrigin::Human)
        .unwrap();
    game.execute_next_intention().unwrap().outcome.unwrap();
    let origin = game.preparation(first).unwrap().origin_intention();
    let result = game
        .act(
            second,
            Action::Take {
                item: tor_simulation::ItemId(10),
                quantity: Some(2),
            },
        )
        .unwrap();
    let tor_simulation::OutcomeKind::Taken { result: split, .. } = result.kind else {
        panic!("expected split")
    };
    assert_ne!(split.0, 10);
    assert_eq!(quantity(&game, first, 10), Some(1));
    assert_eq!(quantity(&game, second, split.0), Some(2));
    assert_eq!(game.preparation(first).unwrap().origin_intention(), origin);
    assert_eq!(round_trip(&game), game);
    game.act(
        second,
        Action::Drop {
            item: split,
            quantity: Some(2),
        },
    )
    .unwrap();
    assert_eq!(quantity(&game, first, 10), Some(3));
    assert_eq!(quantity(&game, second, split.0), None);
    assert_eq!(round_trip(&game), game);
    while game.preparation(first).is_some() {
        game.act(game.next_actor().unwrap(), Action::Wait).unwrap();
    }
    assert_eq!(quantity(&game, first, 10), Some(2));
}

#[test]
fn death_and_forced_actor_displacement_release_every_unspent_fragment() {
    for death in [false, true] {
        let (mut game, actor, other) = fixture_quantity(Some(100), 2, json!({"kind":"self_only"}));
        bandages(&mut game, 10, Some(actor), 1, 1);
        bandages(&mut game, 11, Some(actor), 1, 1);
        start(&mut game, actor);
        if death {
            let maximum = game.creature(actor).unwrap().health().maximum();
            game.apply_effects(
                actor,
                &[super::abilities::damage(
                    u32::try_from(maximum).unwrap(),
                    tor_simulation::combat::DamageType::Vital,
                )],
            )
            .unwrap();
        } else {
            game.teleport(actor, at(4)).unwrap();
        }
        assert!(game.preparation(actor).is_none());
        assert_eq!(remaining(&game, actor), 100);
        assert_eq!(round_trip(&game), game);
        for item in [10, 11] {
            assert_eq!(
                quantity(&game, if death { other } else { actor }, item),
                Some(1)
            );
        }
    }
}

#[test]
fn supply_holds_survive_both_endpoints_frozen_detached_and_thawed() {
    use std::collections::BTreeSet;
    let (mut game, actor, _) = fixture(Some(100), true, json!({"kind":"self_only"}));
    bandages(&mut game, 10, Some(actor), 1, 1);
    start(&mut game, actor);
    let original = game.preparation(actor).unwrap().clone();
    let mut records = tor_simulation::MemoryRecords::default();
    game.transition_regions(
        &tor_simulation::RegionTransition {
            active: BTreeSet::new(),
            loaded: BTreeSet::from([RegionId(1), RegionId(2)]),
        },
        &mut records,
    )
    .unwrap();
    assert_eq!(game.preparation(actor), Some(&original));
    assert_eq!(round_trip(&game), game);
    game.transition_regions(&tor_simulation::RegionTransition::default(), &mut records)
        .unwrap();
    let mut restored = round_trip(&game);
    restored
        .transition_regions(
            &tor_simulation::RegionTransition {
                active: BTreeSet::from([RegionId(1), RegionId(2)]),
                loaded: BTreeSet::from([RegionId(1), RegionId(2)]),
            },
            &mut records,
        )
        .unwrap();
    assert_eq!(restored.preparation(actor), Some(&original));
    assert_eq!(remaining(&restored, actor), 100);
    assert_eq!(quantity(&restored, actor, 10), Some(1));
}

#[test]
fn held_ground_supply_changes_execution_to_wait_without_changing_ai_preview() {
    let (mut game, holder, actor) = fixture(Some(100), true, json!({"kind":"self_only"}));
    bandages(&mut game, 10, None, 1, 1);
    bandages(&mut game, 11, None, 1, 1);
    let max = game.creature(actor).unwrap().health().maximum();
    game.apply_effects(
        actor,
        &[super::abilities::damage(
            u32::try_from(max / 2 + 1).unwrap(),
            tor_simulation::combat::DamageType::Vital,
        )],
    )
    .unwrap();
    game.configure_ai(actor, tor_simulation::ai::AiProfile::general())
        .unwrap();
    let mut free = game.clone();
    game.admit_intention(holder, explicit_action(holder, 10), IntentionOrigin::Human)
        .unwrap();
    free.admit_intention(holder, explicit_action(holder, 11), IntentionOrigin::Human)
        .unwrap();
    game.execute_next_intention().unwrap().outcome.unwrap();
    free.execute_next_intention().unwrap().outcome.unwrap();
    // Only the private exact item claim differs. Public facts and scheduler
    // boundary remain identical, including the other actor's visible work tell.
    assert_eq!(game.next_actor(), Some(actor));
    assert_eq!(free.next_actor(), Some(actor));
    assert_eq!(game.observe(actor).unwrap(), free.observe(actor).unwrap());
    let preview = game.next_ai_action().unwrap();
    assert_eq!(preview, (actor, explicit_action(actor, 10)));
    assert_eq!(free.next_ai_action(), Some(preview));
    game.admit_ai_intention(actor).unwrap();
    let mut restored = round_trip(&game);
    let before_tick = game.tick();
    let execution = game.execute_next_intention().unwrap();
    assert_eq!(execution.action, Some(Action::Wait));
    execution.outcome.unwrap();
    let replay = restored.execute_next_intention().unwrap();
    assert_eq!(replay.action, Some(Action::Wait));
    replay.outcome.unwrap();
    assert_eq!(game, restored);
    assert_eq!(remaining(&game, actor), 0);
    assert!(game.preparation(actor).is_none());
    assert_eq!(quantity(&game, actor, 10), Some(1));
    assert!(game.tick() > before_tick || game.next_actor() != Some(actor));
    free.admit_ai_intention(actor).unwrap();
    let execution = free.execute_next_intention().unwrap();
    assert_eq!(execution.action, Some(explicit_action(actor, 10)));
    execution.outcome.unwrap();
}

#[test]
fn ai_reconsiders_physical_supply_removed_after_admission() {
    let (mut game, holder, actor) = fixture(Some(100), true, json!({"kind":"self_only"}));
    bandages(&mut game, 10, None, 1, 1);
    let max = game.creature(actor).unwrap().health().maximum();
    game.apply_effects(
        actor,
        &[super::abilities::damage(
            u32::try_from(max / 2 + 1).unwrap(),
            tor_simulation::combat::DamageType::Vital,
        )],
    )
    .unwrap();
    game.configure_ai(actor, tor_simulation::ai::AiProfile::general())
        .unwrap();
    game.admit_ai_intention(actor).unwrap();
    game.act(
        holder,
        Action::Take {
            item: tor_simulation::ItemId(10),
            quantity: None,
        },
    )
    .unwrap();
    let mut restored = round_trip(&game);
    let execution = game.execute_next_intention().unwrap();
    assert!(!matches!(execution.action, Some(Action::UseAbility { .. })));
    execution.outcome.unwrap();
    let replay = restored.execute_next_intention().unwrap();
    assert_eq!(execution.action, replay.action);
    replay.outcome.unwrap();
    assert_eq!(game, restored);
    assert_eq!(remaining(&game, actor), 0);
}

#[test]
fn finite_ai_preflight_reuses_its_one_disclosed_decision_scene() {
    let (mut game, actor, _) = fixture(None, true, json!({"kind":"self_only"}));
    bandages(&mut game, 10, None, 1, 1);
    let max = game.creature(actor).unwrap().health().maximum();
    game.apply_effects(
        actor,
        &[super::abilities::damage(
            u32::try_from(max / 2 + 1).unwrap(),
            tor_simulation::combat::DamageType::Vital,
        )],
    )
    .unwrap();
    game.configure_ai(actor, tor_simulation::ai::AiProfile::general())
        .unwrap();
    game.admit_ai_intention(actor).unwrap();
    let before = tor_simulation::diagnostics::work_counts();
    game.execute_next_intention().unwrap().outcome.unwrap();
    let after = tor_simulation::diagnostics::work_counts();
    assert_eq!(after.decision_scenes - before.decision_scenes, 1);
    assert_eq!(
        after.ai_precommit_ordinary_scenes - before.ai_precommit_ordinary_scenes,
        0
    );
}

#[test]
fn resuming_after_natural_cooldown_expiry_cannot_start_a_second_countdown() {
    let (mut game, actor, _) = fixture(Some(1), true, json!({"kind":"self_only"}));
    bandages(&mut game, 10, Some(actor), 1, 1);
    let origin = start(&mut game, actor);
    game.pause_preparation(actor).unwrap();
    for _ in 0..6 {
        game.act(game.next_actor().unwrap(), Action::Wait).unwrap();
    }
    assert_eq!(remaining(&game, actor), 0);
    assert_eq!(
        game.preparation(actor).unwrap().origin_intention(),
        Some(origin)
    );
    start(&mut game, actor);
    assert_eq!(remaining(&game, actor), 0);
    assert_eq!(
        game.preparation(actor).unwrap().origin_intention(),
        Some(origin)
    );
    assert_eq!(quantity(&game, actor, 10), Some(1));
}

#[test]
fn paid_tail_keeps_deleted_exact_source_without_claims_or_live_lookup_on_resume() {
    let (mut game, actor, _) = fixture_offsets(Some(100), 1, json!({"kind":"self_only"}), &[0, 10]);
    bandages(&mut game, 10, Some(actor), 1, 1);
    let origin = game
        .admit_intention(actor, explicit_action(actor, 10), IntentionOrigin::Human)
        .unwrap();
    game.execute_next_intention().unwrap().outcome.unwrap();
    while !matches!(
        game.preparation(actor).map(|p| p.progress),
        Some(tor_simulation::combat::PreparationProgress::Ability { next_strike: 1, .. })
    ) {
        game.act(game.next_actor().unwrap(), Action::Wait).unwrap();
    }
    assert_eq!(quantity(&game, actor, 10), None);
    assert!(matches!(
        game.preparation(actor).unwrap().item_payment,
        tor_simulation::supplies::ItemPayment::Settled
    ));
    game.pause_preparation(actor).unwrap();
    game = round_trip(&game);
    let selected = serde_json::to_value(&game.preparation(actor).unwrap().plan).unwrap();
    assert_eq!(
        selected["supplies"],
        json!({"kind":"explicit","items":[{"cost_ordinal":0,"item":10}]})
    );
    game.admit_intention(actor, explicit_action(actor, 10), IntentionOrigin::Human)
        .unwrap();
    game.execute_next_intention().unwrap().outcome.unwrap();
    assert_eq!(
        game.preparation(actor).unwrap().origin_intention(),
        Some(origin)
    );
    while game.preparation(actor).is_some() {
        game.act(game.next_actor().unwrap(), Action::Wait).unwrap();
    }
    assert_eq!(quantity(&game, actor, 10), None);
    assert_eq!(round_trip(&game), game);
}

fn two_cost_fixture(offsets: &[u64]) -> (Game, ActorId, ActorId) {
    fixture_custom(None, 1, json!({"kind":"self_only"}), offsets, |entry| {
        entry["supply_costs"].as_array_mut().unwrap().push(json!({
            "supply":"ration", "quantity":1, "access":"carried_only", "spend_at":"first_resolution"
        }));
    })
}
fn two_cost_action(actor: ActorId, second_item: u64) -> Action {
    serde_json::from_value(json!({"type":"use_ability", "value":{
        "id":"test_care", "invocation":{"aim":{"kind":"actor_target","target":actor},"weapon":{"kind":"default"}}, "supplies":{"kind":"explicit", "items":[
            {"cost_ordinal":0,"item":10}, {"cost_ordinal":1,"item":second_item}
        ]}
    }}))
    .unwrap()
}
fn ration(game: &mut Game, actor: ActorId) {
    let mut spec = tor_simulation::ItemSpec::ordinary("ration".into());
    spec.supply = Some(tor_simulation::content_ids::SupplyId::new("ration").unwrap());
    game.place_item_stack(11, at(1), Some(actor), 1, spec)
        .unwrap();
}

#[test]
fn one_exact_source_cannot_cover_two_distinct_cost_ordinals_at_admission() {
    let (mut game, actor, _) = two_cost_fixture(&[0]);
    bandages(&mut game, 10, Some(actor), 2, 1);
    ration(&mut game, actor);
    let before = game.clone();
    assert!(game
        .admit_intention(actor, two_cost_action(actor, 10), IntentionOrigin::Human)
        .is_err());
    assert_eq!(game, before);
}

#[test]
fn settled_selection_rejects_reusing_one_source_for_two_distinct_cost_ordinals() {
    let (mut game, actor, _) = two_cost_fixture(&[0, 10]);
    bandages(&mut game, 10, Some(actor), 1, 1);
    ration(&mut game, actor);
    game.admit_intention(actor, two_cost_action(actor, 11), IntentionOrigin::Human)
        .unwrap();
    game.execute_next_intention().unwrap().outcome.unwrap();
    while !matches!(
        game.preparation(actor).map(|p| p.progress),
        Some(tor_simulation::combat::PreparationProgress::Ability { next_strike: 1, .. })
    ) {
        game.act(game.next_actor().unwrap(), Action::Wait).unwrap();
    }
    assert!(matches!(
        game.preparation(actor).unwrap().item_payment,
        tor_simulation::supplies::ItemPayment::Settled
    ));
    assert_eq!(quantity(&game, actor, 10), None);
    assert_eq!(quantity(&game, actor, 11), None);
    assert_eq!(round_trip(&game), game);
    let mut shared = tor_simulation::checkpoint::SharedState::default();
    let saved = game.checkpoint(&mut shared);
    let mut value = serde_json::to_value((saved, shared)).unwrap();
    let actors = value[0]["actors"].as_u64().unwrap() as usize;
    value[1]["actors"][actors][actor.0.to_string()]["pending"]["plan"]["supplies"]["items"][1]
        ["item"] = json!(10);
    let (saved, shared) = serde_json::from_value(value).unwrap();
    assert!(Game::restore_checkpoint(saved, &shared, game.content().clone()).is_err());
}

#[test]
fn cancelled_same_work_restarts_with_a_new_origin_and_cannot_release_its_successor() {
    let (mut game, actor, _) = fixture(None, true, json!({"kind":"self_only"}));
    bandages(&mut game, 10, Some(actor), 1, 1);
    let old = start(&mut game, actor);
    game.pause_preparation(actor).unwrap();
    game.cancel_intention(actor, old).unwrap();
    let new = start(&mut game, actor);
    assert_ne!(old, new);
    assert_eq!(
        game.preparation(actor).unwrap().origin_intention(),
        Some(new)
    );
    let before = game.clone();
    assert!(game.cancel_intention(actor, old).is_err());
    assert_eq!(game, before);
    assert_eq!(round_trip(&game), game);
    while game.preparation(actor).is_some() {
        game.act(game.next_actor().unwrap(), Action::Wait).unwrap();
    }
    assert_eq!(quantity(&game, actor, 10), None);
}

#[test]
fn failed_second_supply_cost_preserves_first_stack_tokens_resources_rng_and_cooldown() {
    let (mut game, taker, actor) = fixture_custom(
        Some(100),
        1,
        json!({"kind":"self_only"}),
        &[0],
        |entry| {
            entry["cost"] = json!({"resource":"stamina","start":1,"resolution":1});
            entry["supply_costs"].as_array_mut().unwrap().push(json!({
            "supply":"ration","quantity":1,"access":"carried_or_ground_touch","spend_at":"first_resolution"
        }));
            entry["strikes"]["strikes"][0]["check"] = json!({
                "actor":"user","skill":{"kind":"selected_weapon"},
                "against":{"kind":"difficulty","formula":{"base":10,"skill":null,"attribute":null}}
            });
        },
    );
    bandages(&mut game, 10, Some(actor), 1, 2);
    let mut spec = tor_simulation::ItemSpec::ordinary("ration".into());
    spec.supply = Some(tor_simulation::content_ids::SupplyId::new("ration").unwrap());
    game.place_item_stack(11, at(1), None, 1, spec).unwrap();
    game.apply_condition(
        actor,
        &tor_simulation::content_ids::ConditionId::new("readied").unwrap(),
        actor,
        500,
    )
    .unwrap();
    assert_eq!(
        game.ability_plan(actor, &ability(), actor)
            .unwrap()
            .tokens()
            .len(),
        1
    );
    game.admit_intention(actor, two_cost_action(actor, 11), IntentionOrigin::Human)
        .unwrap();
    game.admit_intention(
        taker,
        Action::Take {
            item: tor_simulation::ItemId(11),
            quantity: None,
        },
        IntentionOrigin::Human,
    )
    .unwrap();
    game.execute_next_intention().unwrap().outcome.unwrap();
    let before_creature = game.creature(actor).unwrap().into_owned();
    let before_tick = game.tick();
    let mut shared = tor_simulation::checkpoint::SharedState::default();
    let before_rng =
        serde_json::to_value(game.checkpoint(&mut shared)).unwrap()["rules_rng"].clone();
    let failed = game.execute_next_intention().unwrap();
    assert_eq!(failed.intention.actor, actor);
    assert_eq!(
        failed.outcome,
        Err(tor_simulation::GameError::ItemUnavailable)
    );
    assert_eq!(game.creature(actor).unwrap().as_ref(), &before_creature);
    assert_eq!(game.tick(), before_tick);
    assert_eq!(
        serde_json::to_value(game.checkpoint(&mut shared)).unwrap()["rules_rng"],
        before_rng
    );
    assert!(game.preparation(actor).is_none());
    assert_eq!(remaining(&game, actor), 0);
    assert_eq!(quantity(&game, actor, 10), Some(1));
    assert_eq!(round_trip(&game), game);
}

#[test]
fn rejected_replacement_leaves_the_previous_exact_preparation_and_hold_untouched() {
    let (mut game, actor, _) = fixture(None, true, json!({"kind":"self_only"}));
    bandages(&mut game, 10, Some(actor), 1, 1);
    let origin = start(&mut game, actor);
    game.pause_preparation(actor).unwrap();
    let before = game.clone();
    assert!(game
        .admit_intention(actor, explicit_action(actor, 99), IntentionOrigin::Human)
        .is_err());
    assert_eq!(game, before);
    assert_eq!(
        game.preparation(actor).unwrap().origin_intention(),
        Some(origin)
    );
    assert_eq!(round_trip(&game), game);
}

#[test]
fn overflowing_merge_into_a_held_destination_rejects_without_changing_either_stack() {
    let (mut game, holder, carrier) = fixture(None, true, json!({"kind":"self_only"}));
    bandages(&mut game, 10, None, u64::MAX, 2);
    bandages(&mut game, 11, Some(carrier), 1, 2);
    game.admit_intention(holder, explicit_action(holder, 10), IntentionOrigin::Human)
        .unwrap();
    game.execute_next_intention().unwrap().outcome.unwrap();
    assert!(matches!(
        game.preparation(holder).unwrap().item_payment,
        tor_simulation::supplies::ItemPayment::Held { .. }
    ));
    let before = game.clone();
    assert_eq!(
        game.act(
            carrier,
            Action::Drop {
                item: tor_simulation::ItemId(11),
                quantity: None
            }
        ),
        Err(tor_simulation::GameError::InvalidQuantity)
    );
    assert_eq!(game, before);
    assert_eq!(round_trip(&game), game);
}

fn paid_healing_resume(start_cost: u32, minimum_percent: u8) {
    use tor_simulation::resources::Resource;
    let (mut game, actor, _) =
        fixture_custom(Some(1000), 1, json!({"kind":"self_only"}), &[0], |entry| {
            entry["cost"] = json!({"resource":"stamina","start":start_cost,"resolution":1});
            entry["ai"]["minimum_resource_percent"] = json!(minimum_percent);
        });
    bandages(&mut game, 10, Some(actor), 1, 1);
    assert_eq!(
        game.creature(actor)
            .unwrap()
            .costs()
            .resources()
            .maximum(Resource::Stamina),
        2
    );
    let max = game.creature(actor).unwrap().health().maximum();
    game.apply_effects(
        actor,
        &[super::abilities::damage(
            u32::try_from(max / 2 + 1).unwrap(),
            tor_simulation::combat::DamageType::Vital,
        )],
    )
    .unwrap();
    let origin = start(&mut game, actor);
    game.pause_preparation(actor).unwrap();
    game.configure_ai(actor, tor_simulation::ai::AiProfile::general())
        .unwrap();
    while game.next_actor() != Some(actor) {
        game.act(game.next_actor().unwrap(), Action::Wait).unwrap();
    }
    let old = game.preparation(actor).unwrap().clone();
    let costs = game.creature(actor).unwrap().costs().clone();
    assert_eq!(costs.available(Resource::Stamina), 1 - start_cost);
    let tick = game.tick();
    let countdown = remaining(&game, actor);
    assert!(countdown > 0);
    assert_eq!(
        game.next_ai_action(),
        Some((actor, own_action(actor))),
        "a valid paid heal resumes before fresh-only resource gates"
    );
    game.admit_ai_intention(actor).unwrap();
    let execution = game.execute_next_intention().unwrap();
    assert_eq!(execution.action, Some(own_action(actor)));
    execution.outcome.unwrap();
    let resumed = game.preparation(actor).unwrap();
    assert_eq!(resumed.origin_intention(), Some(origin));
    assert_eq!(resumed.hold, old.hold);
    assert_eq!(resumed.item_payment, old.item_payment);
    assert_eq!(game.creature(actor).unwrap().costs(), &costs);
    assert_eq!(remaining(&game, actor), countdown - (game.tick() - tick));
    assert_eq!(quantity(&game, actor, 10), Some(1));
    assert_eq!(round_trip(&game), game);
}
#[test]
fn ai_resumes_paid_healing_without_funding_a_second_start_cost() {
    paid_healing_resume(1, 0);
}
#[test]
fn ai_resumes_paid_healing_before_a_fresh_minimum_resource_gate() {
    paid_healing_resume(0, 100);
}

#[test]
fn held_ground_healing_potion_changes_actual_take_to_wait_without_changing_loot_preview() {
    let (mut game, holder, actor) = fixture(Some(100), true, json!({"kind":"self_only"}));
    let mut spec = tor_simulation::ItemSpec::ordinary("healing bandage draught".into());
    spec.class = tor_simulation::ItemClass::Potion;
    spec.supply = Some(tor_simulation::content_ids::SupplyId::new("bandage").unwrap());
    spec.consumable = Some(tor_simulation::ConsumableSpec {
        descriptors: vec![],
        discovery: tor_simulation::ItemDiscovery::SelfEffect,
        effects: vec![tor_simulation::rules::Effect::Heal {
            amount: tor_simulation::rules::AmountExpr::Fixed { amount: 2 },
        }],
    });
    for item in [10, 11] {
        game.place_item_stack(item, at(2), None, 1, spec.clone())
            .unwrap();
    }
    game.configure_ai(actor, tor_simulation::ai::AiProfile::general())
        .unwrap();
    let mut free = game.clone();
    game.admit_intention(holder, explicit_action(holder, 10), IntentionOrigin::Human)
        .unwrap();
    free.admit_intention(holder, explicit_action(holder, 11), IntentionOrigin::Human)
        .unwrap();
    game.execute_next_intention().unwrap().outcome.unwrap();
    free.execute_next_intention().unwrap().outcome.unwrap();
    assert_eq!(game.observe(actor).unwrap(), free.observe(actor).unwrap());
    let take = Action::Take {
        item: tor_simulation::ItemId(10),
        quantity: Some(1),
    };
    assert_eq!(game.next_ai_action(), Some((actor, take.clone())));
    assert_eq!(free.next_ai_action(), Some((actor, take.clone())));
    game.admit_ai_intention(actor).unwrap();
    let mut restored = round_trip(&game);
    let before_tick = game.tick();
    let execution = game.execute_next_intention().unwrap();
    assert_eq!(execution.action, Some(Action::Wait));
    execution.outcome.unwrap();
    let replay = restored.execute_next_intention().unwrap();
    assert_eq!(replay.action, Some(Action::Wait));
    replay.outcome.unwrap();
    assert_eq!(game, restored);
    assert!(game.tick() > before_tick || game.next_actor() != Some(actor));
    assert_eq!(quantity(&game, actor, 10), Some(1));
    free.admit_ai_intention(actor).unwrap();
    let winner = free.execute_next_intention().unwrap();
    assert_eq!(winner.action, Some(take));
    winner.outcome.unwrap();
    assert!(free
        .observe(actor)
        .unwrap()
        .inventory
        .iter()
        .any(|item| item.id.0 == 10));
}

fn reject_unissued_paid_source(offset: u64) {
    let (mut game, actor, _) = fixture_offsets(None, 1, json!({"kind":"self_only"}), &[0, 10]);
    bandages(&mut game, 10, Some(actor), 1, 1);
    game.admit_intention(actor, explicit_action(actor, 10), IntentionOrigin::Human)
        .unwrap();
    game.execute_next_intention().unwrap().outcome.unwrap();
    while !matches!(
        game.preparation(actor).map(|p| p.progress),
        Some(tor_simulation::combat::PreparationProgress::Ability { next_strike: 1, .. })
    ) {
        game.act(game.next_actor().unwrap(), Action::Wait).unwrap();
    }
    assert_eq!(quantity(&game, actor, 10), None);
    assert!(matches!(
        game.preparation(actor).unwrap().item_payment,
        tor_simulation::supplies::ItemPayment::Settled
    ));
    assert_eq!(
        round_trip(&game),
        game,
        "issued but deleted historical sources remain valid"
    );
    let mut shared = tor_simulation::checkpoint::SharedState::default();
    let snapshot = game.checkpoint(&mut shared);
    let mut value = serde_json::to_value((snapshot, shared)).unwrap();
    let next = value[0]["next_item_id"].as_u64().unwrap();
    let actors = value[0]["actors"].as_u64().unwrap() as usize;
    value[1]["actors"][actors][actor.0.to_string()]["pending"]["plan"]["supplies"]["items"][0]
        ["item"] = json!(next + offset);
    let (snapshot, shared) = serde_json::from_value(value).unwrap();
    assert!(Game::restore_checkpoint(snapshot, &shared, game.content().clone()).is_err());
}
#[test]
fn settled_selection_rejects_the_next_unissued_item_identity() {
    reject_unissued_paid_source(0);
}
#[test]
fn settled_selection_rejects_identity_above_the_item_high_water() {
    reject_unissued_paid_source(100);
}

#[test]
fn direct_drink_without_an_issued_intention_rejects_atomically_then_admitted_drink_succeeds() {
    let (mut game, actor, _) = fixture(None, false, json!({"kind":"self_only"}));
    let mut spec = tor_simulation::ItemSpec::ordinary("ordinary healing draught".into());
    spec.class = tor_simulation::ItemClass::Potion;
    spec.consumable = Some(tor_simulation::ConsumableSpec {
        descriptors: vec![],
        discovery: tor_simulation::ItemDiscovery::SelfEffect,
        effects: vec![tor_simulation::rules::Effect::Heal {
            amount: tor_simulation::rules::AmountExpr::Fixed { amount: 1 },
        }],
    });
    game.place_item_stack(10, at(1), Some(actor), 1, spec)
        .unwrap();
    let action = Action::Drink {
        item: tor_simulation::ItemId(10),
    };
    let before = game.clone();
    assert_eq!(
        game.act(actor, action.clone()),
        Err(tor_simulation::GameError::Schedule(
            tor_simulation::ScheduleError::InvalidIntention
        ))
    );
    assert_eq!(
        game, before,
        "rejection cannot allocate an owner, hold, cost, random draw or tick"
    );
    let origin = game
        .admit_intention(actor, action, IntentionOrigin::Human)
        .unwrap();
    game.execute_next_intention().unwrap().outcome.unwrap();
    assert_eq!(
        game.preparation(actor).unwrap().origin_intention(),
        Some(origin)
    );
    assert!(matches!(
        game.preparation(actor).unwrap().item_payment,
        tor_simulation::supplies::ItemPayment::Held { .. }
    ));
    while game.preparation(actor).is_some() {
        game.act(game.next_actor().unwrap(), Action::Wait).unwrap();
    }
    assert_eq!(quantity(&game, actor, 10), None);
    assert_eq!(round_trip(&game), game);
}

#[test]
fn finite_claim_profile_counts_loaded_authority_without_changing_it() {
    use tor_simulation::diagnostics::FiniteClaimProfile;
    let (mut game, actor, _) = fixture(Some(100), true, json!({"kind":"self_only"}));
    bandages(&mut game, 10, Some(actor), 2, 1);
    assert_eq!(
        game.profile_finite_claims().unwrap(),
        FiniteClaimProfile::default()
    );
    let origin = start(&mut game, actor);
    let expected = FiniteClaimProfile {
        item_holds: 1,
        preparation_refs: 1,
        claimant_actors: 1,
        max_holds_per_item: 1,
        held_units: 1,
    };
    let original = game.clone();
    let view = game.observe(actor).unwrap();
    let before = tor_simulation::diagnostics::work_counts();
    assert_eq!(game.profile_finite_claims().unwrap(), expected);
    assert_eq!(round_trip(&game).profile_finite_claims().unwrap(), expected);
    assert_eq!(game, original);
    assert_eq!(game.observe(actor).unwrap(), view);
    let after = tor_simulation::diagnostics::work_counts();
    assert_eq!(after.ability_plans, before.ability_plans);
    game.cancel_intention(actor, origin).unwrap();
    assert_eq!(
        game.profile_finite_claims().unwrap(),
        FiniteClaimProfile::default()
    );
}

#[test]
fn finite_claim_profile_clears_when_a_last_source_is_settled() {
    let (mut game, actor, _) = fixture(Some(100), true, json!({"kind":"self_only"}));
    bandages(&mut game, 10, Some(actor), 1, 1);
    start(&mut game, actor);
    assert_eq!(game.profile_finite_claims().unwrap().held_units, 1);
    for _ in 0..30 {
        if quantity(&game, actor, 10).is_none() {
            break;
        }
        game.act(game.next_actor().unwrap(), Action::Wait).unwrap();
    }
    assert_eq!(quantity(&game, actor, 10), None);
    assert_eq!(game.profile_finite_claims().unwrap(), Default::default());
}

#[test]
fn phase5_tagged_action_rejects_carried_selection_array() {
    let action = own_action(ActorId(1));
    let mut value = serde_json::to_value(&action).unwrap();
    assert_eq!(
        serde_json::from_value::<Action>(value.clone()).unwrap(),
        action
    );
    value["value"]["supplies"] = json!(["carried"]);
    assert!(serde_json::from_value::<Action>(value).is_err());
}

#[test]
fn phase5_tagged_action_rejects_explicit_selection_array() {
    let action = explicit_action(ActorId(1), 10);
    let mut value = serde_json::to_value(&action).unwrap();
    assert_eq!(
        serde_json::from_value::<Action>(value.clone()).unwrap(),
        action
    );
    let rows = value["value"]["supplies"]["items"].clone();
    assert_eq!(rows.as_array().unwrap().len(), 1);
    value["value"]["supplies"] = json!(["explicit", rows]);
    assert!(serde_json::from_value::<Action>(value).is_err());
}

fn tagged_alias_preparation(supplies: bool) -> (Game, ActorId) {
    let (mut game, actor, _) = fixture_offsets(
        Some(100),
        u64::from(supplies),
        json!({"kind":"self_only"}),
        &[0, 10],
    );
    if supplies {
        bandages(&mut game, 10, Some(actor), 1, 1);
        game.admit_intention(actor, explicit_action(actor, 10), IntentionOrigin::Human)
            .unwrap();
        game.execute_next_intention().unwrap().outcome.unwrap();
    } else {
        start(&mut game, actor);
    }
    assert!(game.preparation(actor).is_some());
    (game, actor)
}

#[test]
fn phase5_tagged_plan_rejects_explicit_selection_array() {
    let (game, actor) = tagged_alias_preparation(true);
    let plan = &game.preparation(actor).unwrap().plan;
    let mut value = serde_json::to_value(plan).unwrap();
    assert_eq!(
        &serde_json::from_value::<tor_simulation::abilities::Plan>(value.clone()).unwrap(),
        plan
    );
    let rows = value["supplies"]["items"].clone();
    assert_eq!(rows.as_array().unwrap().len(), 1);
    value["supplies"] = json!(["explicit", rows]);
    assert!(serde_json::from_value::<tor_simulation::abilities::Plan>(value).is_err());
}

fn checkpoint_payment_alias_is_rejected(game: &Game, actor: ActorId, expected: &str) {
    type Checkpoint = (
        tor_simulation::checkpoint::Snapshot,
        tor_simulation::checkpoint::SharedState,
    );
    let mut shared = tor_simulation::checkpoint::SharedState::default();
    let snapshot = game.checkpoint(&mut shared);
    let mut value = serde_json::to_value((snapshot, shared)).unwrap();
    let (snapshot, shared): Checkpoint = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(
        Game::restore_checkpoint(snapshot, &shared, game.content().clone()).unwrap(),
        *game
    );
    let actors = value[0]["actors"].as_u64().unwrap() as usize;
    let payment = &mut value[1]["actors"][actors][actor.0.to_string()]["pending"]["item_payment"];
    assert_eq!(payment["kind"], expected);
    *payment = if expected == "held" {
        let rows = payment["refs"].clone();
        assert_eq!(rows.as_array().unwrap().len(), 1);
        json!(["held", rows])
    } else {
        json!([expected])
    };
    assert!(serde_json::from_value::<Checkpoint>(value).is_err());
}

#[test]
fn phase5_tagged_checkpoint_rejects_held_payment_array() {
    let (game, actor) = tagged_alias_preparation(true);
    checkpoint_payment_alias_is_rejected(&game, actor, "held");
}

#[test]
fn phase5_tagged_checkpoint_rejects_none_payment_array() {
    let (game, actor) = tagged_alias_preparation(false);
    checkpoint_payment_alias_is_rejected(&game, actor, "none");
}

#[test]
fn phase5_tagged_checkpoint_rejects_settled_payment_array() {
    let (mut game, actor) = tagged_alias_preparation(true);
    for _ in 0..30 {
        if matches!(
            game.preparation(actor).map(|p| &p.item_payment),
            Some(tor_simulation::supplies::ItemPayment::Settled)
        ) {
            break;
        }
        game.act(game.next_actor().unwrap(), Action::Wait).unwrap();
    }
    assert_eq!(quantity(&game, actor, 10), None);
    checkpoint_payment_alias_is_rejected(&game, actor, "settled");
}

fn declared_field_values(value: &Value, fields: &[&str]) -> Vec<Value> {
    let object = value.as_object().unwrap();
    assert_eq!(object.len(), fields.len());
    fields
        .iter()
        .map(|field| object.get(*field).unwrap().clone())
        .collect()
}

fn named_checkpoint_value(game: &Game) -> Value {
    let mut shared = tor_simulation::checkpoint::SharedState::default();
    let snapshot = game.checkpoint(&mut shared);
    let value = serde_json::to_value((snapshot, shared)).unwrap();
    let (snapshot, shared) = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(
        Game::restore_checkpoint(snapshot, &shared, game.content().clone()).unwrap(),
        *game
    );
    value
}

#[test]
fn phase5_container_checkpoint_rejects_positional_preparation() {
    let (game, actor) = tagged_alias_preparation(true);
    let mut value = named_checkpoint_value(&game);
    let actors = value[0]["actors"].as_u64().unwrap() as usize;
    let preparation = &mut value[1]["actors"][actors][actor.0.to_string()]["pending"];
    assert_eq!(preparation["item_payment"]["kind"], "held");
    *preparation = json!(declared_field_values(
        preparation,
        &[
            "intention",
            "hold",
            "origin",
            "plan",
            "progress",
            "item_payment",
            "threats",
            "remaining",
            "started",
            "active",
        ]
    ));
    assert!(serde_json::from_value::<(
        tor_simulation::checkpoint::Snapshot,
        tor_simulation::checkpoint::SharedState
    )>(value)
    .is_err());
}

#[test]
fn phase5_container_checkpoint_rejects_positional_held_item() {
    let (game, _) = tagged_alias_preparation(true);
    let mut value = named_checkpoint_value(&game);
    let items = value[0]["items"].as_u64().unwrap() as usize;
    let item = &mut value[1]["items"][items]["10"];
    assert_eq!(item["holds"].as_array().unwrap().len(), 1);
    *item = json!(declared_field_values(
        item,
        &[
            "corpse",
            "condition",
            "charges",
            "relocation_elapsed",
            "motion",
            "orientation",
            "spec",
            "quantity",
            "holds",
            "location",
        ]
    ));
    assert!(serde_json::from_value::<(
        tor_simulation::checkpoint::Snapshot,
        tor_simulation::checkpoint::SharedState
    )>(value)
    .is_err());
}

fn actual_ability_plan_values() -> Vec<Value> {
    let (game, actor) = tagged_alias_preparation(true);
    let plan = game.preparation(actor).unwrap().plan.as_ability().unwrap();
    let named = serde_json::to_value(plan).unwrap();
    assert_eq!(
        &serde_json::from_value::<tor_simulation::abilities::AbilityPlan>(named.clone()).unwrap(),
        plan
    );
    declared_field_values(
        &named,
        &[
            "ambush",
            "actor",
            "invocation",
            "weapon",
            "step",
            "target",
            "initial_hold",
            "hold_damage_increment",
            "public_aim",
            "ability",
            "preparation",
            "recovery",
            "cost",
            "cooldown",
            "supply_costs",
            "supplies",
            "reach",
            "strikes",
            "tokens",
        ],
    )
}

#[test]
fn phase5_container_ability_plan_rejects_positional_fields() {
    assert!(
        serde_json::from_value::<tor_simulation::abilities::AbilityPlan>(json!(
            actual_ability_plan_values()
        ))
        .is_err()
    );
}

#[test]
fn phase5_container_plan_rejects_positional_ability_payload() {
    let mut fields = vec![json!("ability")];
    fields.extend(actual_ability_plan_values());
    assert!(serde_json::from_value::<tor_simulation::abilities::Plan>(json!(fields)).is_err());
}

#[test]
fn phase5_container_checkpoint_rejects_positional_item_spec() {
    let (game, _) = tagged_alias_preparation(true);
    let mut value = named_checkpoint_value(&game);
    let items = value[0]["items"].as_u64().unwrap() as usize;
    let spec = &mut value[1]["items"][items]["10"]["spec"];
    assert_eq!(spec["supply"], "bandage");
    // Asset alone is legitimately omitted by the serializer.
    spec.as_object_mut()
        .unwrap()
        .entry("asset")
        .or_insert(Value::Null);
    *spec = json!(declared_field_values(
        spec,
        &[
            "magical",
            "currency",
            "equipment",
            "projectile",
            "equipment_generation",
            "consumable",
            "readable",
            "ground_scare",
            "device",
            "edible",
            "supply",
            "class",
            "archetype",
            "identity",
            "name",
            "appearance",
            "concealed",
            "stackable",
            "properties",
            "asset",
        ]
    ));
    assert!(serde_json::from_value::<(
        tor_simulation::checkpoint::Snapshot,
        tor_simulation::checkpoint::SharedState
    )>(value)
    .is_err());
}

#[test]
fn phase5_container_checkpoint_rejects_positional_creature_record() {
    let (game, actor) = tagged_alias_preparation(true);
    let mut value = named_checkpoint_value(&game);
    let actors = value[0]["actors"].as_u64().unwrap() as usize;
    let creature = &mut value[1]["actors"][actors][actor.0.to_string()]["combat"]["creature"];
    assert_eq!(creature["cooldowns"].as_array().unwrap().len(), 1);
    assert!(creature["cooldowns"][0]["remaining"].as_u64().unwrap() > 0);
    *creature = json!(declared_field_values(
        creature,
        &[
            "build",
            "build_epoch",
            "earned_progress",
            "restorable_losses",
            "injury",
            "dead",
            "health_recovery",
            "nutrition",
            "received_health_loss_remaining",
            "pools",
            "holds",
            "conditions",
            "next_condition_generation",
            "cooldowns",
        ]
    ));
    assert!(serde_json::from_value::<(
        tor_simulation::checkpoint::Snapshot,
        tor_simulation::checkpoint::SharedState
    )>(value)
    .is_err());
}

#[test]
fn phase5_container_action_rejects_positional_use_ability() {
    let action = explicit_action(ActorId(1), 10);
    let value = serde_json::to_value(&action).unwrap();
    assert_eq!(
        serde_json::from_value::<Action>(value.clone()).unwrap(),
        action
    );
    assert!(value["value"].is_object());
    assert!(serde_json::from_value::<Action>(json!(["use_ability", value["value"]])).is_err());
}

#[test]
fn phase5_container_work_rejects_positional_use_ability() {
    let (game, actor) = tagged_alias_preparation(true);
    let work = game.preparation(actor).unwrap().plan.work();
    let value = serde_json::to_value(&work).unwrap();
    assert_eq!(
        serde_json::from_value::<tor_simulation::Work>(value.clone()).unwrap(),
        work
    );
    assert_eq!(value["type"], "use_ability");
    assert!(serde_json::from_value::<tor_simulation::Work>(json!([
        "use_ability",
        value["id"],
        value["invocation"],
        value["supplies"],
    ]))
    .is_err());
}

fn use_ability_byte_operands_require_names(type_first: bool) {
    let action = explicit_action(ActorId(1), 10);
    let value = serde_json::to_value(&action).unwrap();
    let named = &value["value"];
    assert!(named.is_object());
    let envelope = |payload: &Value| {
        if type_first {
            format!(r#"{{"type":"use_ability","value":{payload}}}"#)
        } else {
            format!(r#"{{"value":{payload},"type":"use_ability"}}"#)
        }
    };
    assert_eq!(
        serde_json::from_slice::<Action>(envelope(named).as_bytes()).unwrap(),
        action
    );
    let positional = json!([named["id"], named["invocation"], named["supplies"]]);
    assert!(serde_json::from_slice::<Action>(envelope(&positional).as_bytes()).is_err());
}

#[test]
fn phase5_bytes_action_rejects_positional_use_ability_type_first() {
    use_ability_byte_operands_require_names(true);
}

#[test]
fn phase5_bytes_action_rejects_positional_use_ability_value_first() {
    use_ability_byte_operands_require_names(false);
}

#[test]
fn ordinary_free_unit_transfer_preserves_exact_witness_before_own_cancellation() {
    for taking in [false, true] {
        let (mut game, actor, _) = fixture(None, true, json!({"kind":"self_only"}));
        bandages(&mut game, 10, Some(actor), 2, 1);
        if taking {
            bandages(&mut game, 11, None, 1, 1);
        }
        start(&mut game, actor);
        game.pause_preparation(actor).unwrap();
        assert_eq!(game.profile_finite_claims().unwrap().held_units, 1);
        let outcome = game
            .act(
                actor,
                if taking {
                    Action::Take {
                        item: tor_simulation::ItemId(11),
                        quantity: Some(1),
                    }
                } else {
                    Action::Drop {
                        item: tor_simulation::ItemId(10),
                        quantity: Some(1),
                    }
                },
            )
            .unwrap();
        assert!(matches!(
            outcome.kind,
            tor_simulation::OutcomeKind::Taken { quantity: 1, .. }
                | tor_simulation::OutcomeKind::Dropped { quantity: 1, .. }
        ));
        assert!(game.preparation(actor).is_none());
        assert_eq!(game.profile_finite_claims().unwrap().held_units, 0);
        assert_eq!(quantity(&game, actor, 10), Some(if taking { 3 } else { 1 }));
        assert_eq!(round_trip(&game), game);
    }
}
