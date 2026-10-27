//! Generic self-defense decisions use disclosed pressure and ordinary paid work.
use super::*;
use crate::{
    content_ids::{AbilityId, ConditionId},
    rules::{Modifier, ModifierOp, Ruleset, Scope},
    test_creatures,
};
use serde_json::{json, Value};
use std::num::NonZeroU64;
use tor_world::{Position, RegionId};

const OWNER: ActorId = ActorId(2);
const FOE: ActorId = ActorId(1);
fn id(value: &str) -> AbilityId {
    AbilityId::new(value).unwrap()
}
fn at(region: u64, x: i32, y: i32) -> Location {
    Location {
        region: RegionId(region),
        position: Position { x, y, z: 0 },
    }
}
fn action(name: &str) -> Action {
    Action::UseAbility {
        id: id(name),
        invocation: crate::abilities::AbilityInvocation::actor_target(OWNER),
        supplies: Default::default(),
    }
}
fn definition(name: &str, condition: &str) -> Value {
    json!({
        "id": name, "name": name, "tags": [], "basic_attack": false,
        "invocation": "actor_target", "cost": {"resource":"stamina","start":1,"resolution":1},
        "timing": {"source":{"kind":"fixed","prepare":100,"recover":100},"scaled_by":null},
        "reach":{"kind":"self_only"},
        "strikes":{"kind":"authored","strikes":[{"offset":0,"unlock":null,"check":null,
            "on_success":[{"kind":"apply_condition","condition":condition,"duration":400}],
            "on_failure":[],"descriptors":[],"riders":[]}]},
        "interaction": "non_hostile", "ai":{"priority":30,"hostile":false,"minimum_resource_percent":0,"range_preference":"any"}
    })
}
fn condition(name: &str, family: &str, op: Value) -> Value {
    json!({"id":name,"family":family,"name":name,"lifecycle":"timed",
        "timing":{"kind":"actor_active","on_expiry":{"kind":"remove"}},
        "restrictions":[],"preparation":"preserve",
        "modifiers":[{"scope":"always","op":op}],"stacking":{"kind":"longest"},
        "source_scoped":false,"descriptors":[],"capacity":"reject"})
}
fn fixture(change: impl FnOnce(&mut Value, &mut crate::creatures::Species)) -> Game {
    let mut document = serde_json::to_value(test_creatures::ruleset().document()).unwrap();
    let incoming =
        json!({"kind":"add","stat":{"kind":"incoming_edge","defense":"physical"},"value":-1});
    document["conditions"].as_array_mut().unwrap().extend([
        condition("probe_guard", "probe_family", incoming.clone()),
        condition("probe_equal", "probe_family", incoming),
        condition(
            "probe_shell",
            "probe_shell",
            json!({"kind":"talent_reduction","category":"impact","amount":2}),
        ),
        condition(
            "probe_strong",
            "probe_strong",
            json!({"kind":"talent_reduction","category":"impact","amount":4}),
        ),
        condition(
            "probe_immunity",
            "probe_immunity",
            json!({"kind":"immunity","selector":{"kind":"category","category":"impact"}}),
        ),
    ]);
    document["abilities"]
        .as_array_mut()
        .unwrap()
        .push(definition("probe_defense", "probe_guard"));
    let mut owner = test_creatures::species();
    owner.modifiers.push(Modifier {
        cancellable: false,
        scope: Scope::Always,
        op: ModifierOp::GrantAbility {
            ability: id("probe_defense"),
        },
    });
    change(&mut document, &mut owner);
    super::tests::configured_game(
        test_creatures::species(),
        owner,
        Ruleset::validate(serde_json::from_value(document).unwrap()).unwrap(),
    )
}
fn ability(document: &mut Value) -> &mut Value {
    document["abilities"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|entry| entry["id"] == "probe_defense")
        .unwrap()
}
fn apply(game: &mut Game, name: &str, duration: u64) {
    game.apply_effects(
        OWNER,
        &[crate::rules::Effect::ApplyCondition {
            condition: ConditionId::new(name).unwrap(),
            duration,
        }],
    )
    .unwrap();
}
fn prepare(game: &mut Game) -> crate::IntentionId {
    game.spawn_actor(at(1, 0, 0), NonZeroU64::new(100).unwrap())
        .unwrap();
    game.act(FOE, Action::Wait).unwrap();
    let identity = game.admit_ai_intention(OWNER).unwrap();
    let execution = game.execute_next_intention().unwrap();
    assert_eq!(execution.action, Some(action("probe_defense")));
    execution.outcome.unwrap();
    identity
}

#[test]
fn self_defense_sensed_threat_selects_generic_owned_condition_without_spending() {
    let game = fixture(|_, _| {});
    let before = game.clone();
    let choice = game.choose_ai(OWNER).unwrap();
    assert_eq!(
        choice.action,
        action("probe_defense"),
        "sensed hostile must prompt the owned generic defensive condition"
    );
    assert!(
        choice.plan.is_some(),
        "commit must own the exact immutable plan"
    );
    assert_eq!(
        game, before,
        "scanning cannot pay, reserve, advance time or RNG"
    );
}

fn add_unconnected_region(game: &mut Game) {
    game.add_region(tor_world::Region {
        id: RegionId(77),
        name: "Undisclosed remote region".into(),
        bounds: tor_world::Extent::new(8, 8, 3).unwrap(),
    })
    .unwrap();
}

#[test]
fn self_defense_hidden_threat_and_remembered_threat_do_not_prompt_buffing() {
    let mut game = fixture(|_, _| {});
    let seen = game.choose_ai(OWNER).unwrap();
    game.actors.get_mut(&OWNER).unwrap().behavior = seen.behavior;
    game.combat.ai.insert(OWNER, seen.ai);
    add_unconnected_region(&mut game);
    game.teleport(FOE, at(77, 4, 2)).unwrap();
    let hidden = game.observe(OWNER).unwrap();
    assert!(hidden
        .visible_cells
        .iter()
        .all(|cell| cell.location.region != RegionId(77)));
    assert!(hidden.visible_actors.is_empty());
    assert_ne!(
        game.choose_ai(OWNER).unwrap().action,
        action("probe_defense")
    );
    game.actors.get_mut(&OWNER).unwrap().behavior.engagement = None;
    assert_ne!(
        game.choose_ai(OWNER).unwrap().action,
        action("probe_defense")
    );
}

#[test]
fn self_defense_hidden_world_and_enemy_immunity_twins_choose_identically() {
    let mut a = fixture(|_, _| {});
    add_unconnected_region(&mut a);
    let mut b = a.clone();
    assert!(std::sync::Arc::ptr_eq(a.content(), b.content()));
    b.set_wall(at(77, 4, 2), true).unwrap();
    b.actors
        .get_mut(&FOE)
        .unwrap()
        .combat
        .as_mut()
        .unwrap()
        .apply_condition(
            &ConditionId::new("probe_immunity").unwrap(),
            FOE,
            1000,
            None,
        )
        .unwrap();
    let a_view = a.observe(OWNER).unwrap();
    let b_view = b.observe(OWNER).unwrap();
    for view in [&a_view, &b_view] {
        assert!(
            view.visible_cells
                .iter()
                .all(|cell| cell.location.region != RegionId(77)),
            "remote geometry must actually be undisclosed before comparing decisions"
        );
        assert!(view.visible_actors.iter().any(|actor| actor.id == FOE));
    }
    assert!(
        a_view == b_view,
        "entire disclosed observations must match despite remote geometry and enemy immunity"
    );
    assert_eq!(a.choose_ai(OWNER).unwrap().action, action("probe_defense"));
    assert_eq!(
        a.choose_ai(OWNER).unwrap().action,
        b.choose_ai(OWNER).unwrap().action
    );
}

#[test]
fn self_defense_active_equal_family_and_last_tick_are_not_maintained() {
    for condition in ["probe_guard", "probe_equal"] {
        let mut game = fixture(|_, _| {});
        apply(&mut game, condition, 1);
        assert_ne!(
            game.choose_ai(OWNER).unwrap().action,
            action("probe_defense")
        );
        game.actors
            .get_mut(&OWNER)
            .unwrap()
            .combat
            .as_mut()
            .unwrap()
            .advance_active(1);
        assert_eq!(
            game.choose_ai(OWNER).unwrap().action,
            action("probe_defense")
        );
    }
}

#[test]
fn self_defense_category_uses_strongest_talent_bucket_and_keeps_ordinary_reduction() {
    for (existing, expected) in [("probe_shell", false), ("probe_strong", false)] {
        let mut game = fixture(|doc, _| {
            ability(doc)["strikes"]["strikes"][0]["on_success"][0]["condition"] =
                json!("probe_shell");
        });
        apply(&mut game, existing, 1);
        assert_eq!(
            game.choose_ai(OWNER).unwrap().action == action("probe_defense"),
            expected
        );
    }
    let game = fixture(|doc, owner| {
        ability(doc)["strikes"]["strikes"][0]["on_success"][0]["condition"] = json!("probe_shell");
        owner.modifiers.push(
            serde_json::from_value(json!({"scope":"always","op":{
            "kind":"reduction","selector":{"kind":"category","category":"impact"},"amount":10}}))
            .unwrap(),
        );
    });
    assert_eq!(
        game.choose_ai(OWNER).unwrap().action,
        action("probe_defense"),
        "ordinary reduction stacks with talent protection and must not mask its benefit"
    );
}

#[test]
fn self_defense_harmful_or_conditional_payloads_are_never_safe_candidates() {
    for shape in [
        "harm",
        "failure",
        "rider",
        "check",
        "mixed_condition",
        "source_scoped",
        "ally",
        "hostile",
    ] {
        let game = fixture(|doc, _| {
            match shape {
                "harm" => ability(doc)["strikes"]["strikes"][0]["on_success"].as_array_mut().unwrap().push(
                    json!({"kind":"damage","damage":{"kind":"components","components":[{"category":"vital","amount":{"kind":"fixed","amount":1}}]}})),
                "failure" => ability(doc)["strikes"]["strikes"][0]["on_failure"] = json!([{"kind":"heal","amount":{"kind":"fixed","amount":1}}]),
                "rider" => ability(doc)["strikes"]["strikes"][0]["riders"] = json!([{"trigger":"success","check":null,"effects":[{"kind":"heal","amount":{"kind":"fixed","amount":1}}],"descriptors":[]}]),
                "check" => ability(doc)["strikes"]["strikes"][0]["check"] = json!({"actor":"user","skill":{"kind":"named","skill":"athletics"},"against":{"kind":"difficulty","formula":{"base":10,"skill":null,"attribute":null}}}),
                "mixed_condition" => doc["conditions"].as_array_mut().unwrap().iter_mut().find(|c| c["id"] == "probe_guard").unwrap()["modifiers"].as_array_mut().unwrap().push(json!({"scope":"always","op":{"kind":"add","stat":{"kind":"health"},"value":-1}})),
                "source_scoped" => doc["conditions"].as_array_mut().unwrap().iter_mut().find(|c| c["id"] == "probe_guard").unwrap()["source_scoped"] = json!(true),
                "ally" => ability(doc)["reach"] = json!({"kind":"self_or_ally","range":{"kind":"touch"}}),
                "hostile" => ability(doc)["ai"]["hostile"] = json!(true),
                _ => unreachable!(),
            }
            if matches!(shape, "harm" | "mixed_condition") {
                ability(doc)["interaction"] = json!("hostile_to_target");
            }
        });
        assert_ne!(
            game.choose_ai(OWNER).unwrap().action,
            action("probe_defense"),
            "{shape}"
        );
    }
}

#[test]
fn self_defense_unfunded_supplies_and_authored_resource_threshold_refuse_fresh_work() {
    for reason in ["unfunded", "supplies", "minimum"] {
        let game = fixture(|doc, _| match reason {
            "unfunded" => ability(doc)["cost"]["start"] = json!(100),
            "supplies" => {
                ability(doc)["supply_costs"] = json!([{"supply":"bandage","quantity":1,"access":"carried_or_ground_touch","spend_at":"first_resolution"}])
            }
            "minimum" => {
                ability(doc)["cost"] = json!({"resource":"focus","start":1,"resolution":1});
                ability(doc)["ai"]["minimum_resource_percent"] = json!(100);
            }
            _ => unreachable!(),
        });
        let mut game = game;
        if reason == "minimum" {
            game.actors
                .get_mut(&OWNER)
                .unwrap()
                .combat
                .as_mut()
                .unwrap()
                .start_preparation_cost(
                    crate::IntentionId(900),
                    crate::costs::CostBundle::single(crate::costs::ResourceCost {
                        resource: crate::resources::Resource::Focus,
                        start: 1,
                        resolution: 0,
                    })
                    .unwrap(),
                )
                .unwrap();
        }
        assert_ne!(
            game.choose_ai(OWNER).unwrap().action,
            action("probe_defense"),
            "{reason}"
        );
    }
}

#[test]
fn self_defense_paid_pause_resumes_exact_plan_despite_cooldown_and_depleted_threshold() {
    let mut game = fixture(|doc, _| {
        ability(doc)["cooldown"] = json!(1000);
        ability(doc)["ai"]["minimum_resource_percent"] = json!(100);
    });
    let origin = prepare(&mut game);
    let frozen = game.preparation(OWNER).unwrap().plan.clone();
    game.pause_preparation(OWNER).unwrap();
    let costs = game.creature(OWNER).unwrap().costs().clone();
    assert_eq!(
        game.choose_ai(OWNER).unwrap().action,
        action("probe_defense")
    );
    let latest = game.admit_ai_intention(OWNER).unwrap();
    game.execute_next_intention().unwrap().outcome.unwrap();
    let pending = game.preparation(OWNER).unwrap();
    assert_eq!(pending.origin_intention(), Some(origin));
    assert_eq!(pending.intention, Some(latest));
    assert_eq!(pending.plan, frozen);
    assert_eq!(game.creature(OWNER).unwrap().costs(), &costs);
}

#[test]
fn self_defense_cancelled_cooldown_refuses_fresh_reapplication() {
    let mut game = fixture(|doc, _| {
        ability(doc)["cooldown"] = json!(1000);
    });
    let origin = prepare(&mut game);
    game.cancel_intention(OWNER, origin).unwrap();
    assert_ne!(
        game.choose_ai(OWNER).unwrap().action,
        action("probe_defense")
    );
}

#[test]
fn self_defense_reordering_uses_authored_priority_then_stable_identity() {
    for reverse in [false, true] {
        let game = fixture(|doc, owner| {
            for (name, priority) in [("aaa_guard", 40), ("zzz_guard", 40)] {
                let mut candidate = definition(name, "probe_guard");
                candidate["ai"]["priority"] = json!(priority);
                doc["abilities"].as_array_mut().unwrap().push(candidate);
                owner.modifiers.push(Modifier {
                    cancellable: false,
                    scope: Scope::Always,
                    op: ModifierOp::GrantAbility { ability: id(name) },
                });
            }
            if reverse {
                doc["abilities"].as_array_mut().unwrap().reverse();
                owner.modifiers.reverse();
            }
        });
        assert_eq!(game.choose_ai(OWNER).unwrap().action, action("aaa_guard"));
    }
}

#[test]
fn self_defense_large_catalog_reuses_one_scene_without_rebuild_or_per_candidate_plans() {
    let game = fixture(|doc, _| {
        for index in 0..256 {
            let name = format!("defense_{index:03}");
            let mut candidate = definition(&name, "probe_guard");
            candidate["ai"]["priority"] = json!(31);
            candidate["cost"]["start"] = json!(100);
            doc["abilities"].as_array_mut().unwrap().push(candidate);
            doc["constants"]["default_abilities"]
                .as_array_mut()
                .unwrap()
                .push(json!(name));
        }
    });
    let before = crate::diagnostics::work_counts();
    let choice = game.choose_ai(OWNER).unwrap();
    let after = crate::diagnostics::work_counts();
    assert_eq!(choice.action, action("probe_defense"));
    assert_eq!(after.scenes - before.scenes, 1);
    assert_eq!(after.decision_scenes - before.decision_scenes, 1);
    assert_eq!(after.ability_plans - before.ability_plans, 1);
    assert_eq!(
        after.ai_defense_candidates - before.ai_defense_candidates,
        257
    );
    assert_eq!(
        after.ai_defense_source_scans - before.ai_defense_source_scans,
        4
    );
    assert_eq!(after.health_derivations - before.health_derivations, 0);
    assert_eq!(after.game_clones - before.game_clones, 0);
}

#[test]
fn self_defense_reserved_incoming_token_is_not_reapplied_after_expiry() {
    let mut game = fixture(|doc, _| {
        let guard = doc["conditions"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|c| c["id"] == "probe_guard")
            .unwrap();
        guard["lifecycle"] = json!("next_check_against");
        guard["family"] = json!("probe_token_family");
        guard["modifiers"][0]["op"]["stat"]["defense"] = Value::Null;
    });
    apply(&mut game, "probe_guard", 1);
    let tokens = game
        .creature(OWNER)
        .unwrap()
        .conditions()
        .qualifying_tokens(game.content().rules(), OWNER, true);
    assert_eq!(tokens.len(), 1);
    game.actors
        .get_mut(&OWNER)
        .unwrap()
        .combat
        .as_mut()
        .unwrap()
        .reserve_tokens(&tokens, FOE)
        .unwrap();
    game.actors
        .get_mut(&OWNER)
        .unwrap()
        .combat
        .as_mut()
        .unwrap()
        .advance_active(2);
    assert_ne!(
        game.choose_ai(OWNER).unwrap().action,
        action("probe_defense")
    );
    assert_eq!(
        game.creature(OWNER)
            .unwrap()
            .conditions()
            .instances()
            .next()
            .unwrap()
            .reserved_by,
        Some(FOE)
    );
}

#[test]
fn self_defense_healing_and_disclosed_self_step_retreat_keep_priority() {
    for fleeing in [false, true] {
        let mut game = fixture(|doc, owner| {
            let name = if fleeing {
                "probe_retreat"
            } else {
                "probe_heal"
            };
            let mut other = definition(name, "probe_guard");
            other["invocation"] = json!(if fleeing { "self_step" } else { "actor_target" });
            other["strikes"]["strikes"][0]["on_success"] = if fleeing {
                json!([{"kind":"step_self_one_cell"}])
            } else {
                json!([{"kind":"heal","amount":{"kind":"fixed","amount":20}}])
            };
            doc["abilities"].as_array_mut().unwrap().push(other);
            owner.modifiers.push(Modifier {
                cancellable: false,
                scope: Scope::Always,
                op: ModifierOp::GrantAbility { ability: id(name) },
            });
        });
        if fleeing {
            game.combat.ai.get_mut(&OWNER).unwrap().profile.flee_percent = 100;
            assert!(matches!(
                game.choose_ai(OWNER).unwrap().action,
                Action::UseAbility {
                    invocation: crate::abilities::AbilityInvocation {
                        aim: crate::abilities::AbilityAim::SelfStep { .. },
                        ..
                    },
                    ..
                }
            ));
        } else {
            game.apply_effects(
                OWNER,
                &[crate::rules::Effect::Damage {
                    damage: crate::rules::DamageExpr::Components {
                        components: vec![crate::rules::DamageComponentExpr {
                            category: crate::combat::DamageType::Vital,
                            descriptor: None,
                            amount: crate::rules::AmountExpr::Fixed { amount: 15 },
                        }],
                    },
                }],
            )
            .unwrap();
            assert_eq!(game.choose_ai(OWNER).unwrap().action, action("probe_heal"));
        }
    }
}

#[test]
fn self_defense_never_replaces_a_paid_strike_prefix() {
    paid_prefix_retains_priority(false);
}

#[test]
fn erratic_movement_never_draws_or_replaces_a_paid_strike_prefix() {
    paid_prefix_retains_priority(true);
}

fn paid_prefix_retains_priority(erratic: bool) {
    let mut game = fixture(|doc, owner| {
        if erratic {
            owner.modifiers.push(Modifier {
                cancellable: false,
                scope: Scope::Always,
                op: ModifierOp::Locomotion {
                    value: crate::locomotion::LocomotionValue::Erratic {
                        chance: Some(crate::locomotion::MovementChance {
                            numerator: 1,
                            denominator: 1,
                        }),
                    },
                },
            });
        }
        let mut other = definition("probe_sequence", "probe_guard");
        other["reach"] = json!({"kind":"melee"});
        other["ai"]["hostile"] = json!(true);
        other["interaction"] = json!("hostile_to_target");
        other["strikes"]["strikes"][0]["on_success"] = json!([{
            "kind":"damage","damage":{"kind":"components","components":[{
                "category":"vital","amount":{"kind":"fixed","amount":1}}]}}]);
        let mut second = other["strikes"]["strikes"][0].clone();
        second["offset"] = json!(300);
        other["strikes"]["strikes"]
            .as_array_mut()
            .unwrap()
            .push(second);
        doc["abilities"].as_array_mut().unwrap().push(other);
        owner.modifiers.push(Modifier {
            cancellable: false,
            scope: Scope::Always,
            op: ModifierOp::GrantAbility {
                ability: id("probe_sequence"),
            },
        });
    });
    // As in the ordinary paid-prefix lifecycle tests, the opening action is
    // manually selected before autonomous control begins. An already-autonomous
    // actor correctly rejects a recorded action that differs from its decision.
    if erratic {
        assert!(matches!(
            game.choose_ai(OWNER).unwrap().action,
            Action::Move(_)
        ));
    } else {
        assert_eq!(
            game.choose_ai(OWNER).unwrap().action,
            action("probe_defense")
        );
    }
    let profile = game.combat.ai.remove(&OWNER).unwrap().profile;
    let initial_stamina = game
        .creature(OWNER)
        .unwrap()
        .costs()
        .resources()
        .balance(crate::resources::Resource::Stamina);
    let initial_health = game.health(FOE).unwrap().0;
    game.spawn_actor(at(1, 0, 0), NonZeroU64::new(100).unwrap())
        .unwrap();
    game.act(FOE, Action::Wait).unwrap();
    let attack = Action::UseAbility {
        id: id("probe_sequence"),
        invocation: crate::abilities::AbilityInvocation::actor_target(FOE),
        supplies: Default::default(),
    };
    let origin = game
        .admit_intention(OWNER, attack.clone(), crate::IntentionOrigin::Human)
        .unwrap();
    game.execute_next_intention().unwrap().outcome.unwrap();
    assert_eq!(
        game.preparation(OWNER).unwrap().origin_intention(),
        Some(origin)
    );
    let after_start = game.creature(OWNER).unwrap().costs().clone();
    assert_eq!(
        after_start
            .resources()
            .balance(crate::resources::Resource::Stamina),
        initial_stamina - 1
    );
    assert_eq!(after_start.reservations().len(), 1);
    let (&hold, &cost) = after_start.reservations().iter().next().unwrap();
    assert_eq!(
        cost,
        crate::costs::CostBundle::single(crate::costs::ResourceCost {
            resource: crate::resources::Resource::Stamina,
            start: 1,
            resolution: 1,
        })
        .unwrap()
    );
    // Preparation earns ordinary recovery. Compare the complete paid ledger
    // against its independent clock/cost kernel, not a net-balance subtraction
    // that incorrectly ignores that earned recovery.
    let mut expected_paid = after_start.clone();
    expected_paid.advance_active(100 - game.tick);
    assert_eq!(expected_paid.finish(hold).unwrap(), cost);
    game.tick = 100;
    game.resolve_preparations().unwrap();
    assert!(game.preparation(OWNER).unwrap().has_prefix());
    assert_eq!(game.health(FOE).unwrap().0, initial_health - 1);
    assert_eq!(game.creature(OWNER).unwrap().costs(), &expected_paid);
    assert!(game
        .creature(OWNER)
        .unwrap()
        .costs()
        .reservations()
        .is_empty());
    game.tick = 150;
    game.pause_preparation(OWNER).unwrap();
    let pending = game.preparation(OWNER).unwrap();
    let frozen = pending.plan.clone();
    assert_eq!(pending.remaining, 250);
    assert_eq!(pending.origin_intention(), Some(origin));
    game.configure_ai(OWNER, profile).unwrap();
    let before = game.clone();
    let chosen = game.choose_ai(OWNER).unwrap();
    assert_eq!(chosen.action, attack);
    assert_eq!(chosen.plan, Some(frozen));
    assert_eq!(chosen.behavior.rng, before.actors[&OWNER].behavior.rng);
    assert_eq!(game.preparation(OWNER).unwrap().remaining, 250);
    assert_eq!(
        game, before,
        "selecting the committed tail cannot charge, reserve or replace it"
    );
}

#[test]
fn self_defense_paid_checkpoint_replays_identical_decision_and_settlement() {
    let mut game = fixture(|doc, _| {
        ability(doc)["cooldown"] = json!(1000);
    });
    let origin = prepare(&mut game);
    game.pause_preparation(OWNER).unwrap();
    let mut shared = crate::checkpoint::SharedState::default();
    let snapshot =
        serde_json::from_value(serde_json::to_value(game.checkpoint(&mut shared)).unwrap())
            .unwrap();
    let shared = serde_json::from_value(serde_json::to_value(shared).unwrap()).unwrap();
    let mut restored = Game::restore_checkpoint(snapshot, &shared, game.content().clone()).unwrap();
    assert_eq!(game, restored);
    assert_eq!(
        restored.preparation(OWNER).unwrap().origin_intention(),
        Some(origin)
    );
    for candidate in [&mut game, &mut restored] {
        assert_eq!(
            candidate.choose_ai(OWNER).unwrap().action,
            action("probe_defense")
        );
        candidate.admit_ai_intention(OWNER).unwrap();
        candidate.execute_next_intention().unwrap().outcome.unwrap();
        for _ in 0..10 {
            if candidate.preparation(OWNER).is_none() {
                break;
            }
            candidate
                .act(candidate.next_actor().unwrap(), Action::Wait)
                .unwrap();
        }
        assert!(candidate.preparation(OWNER).is_none());
        assert!(candidate
            .creature(OWNER)
            .unwrap()
            .conditions()
            .instances()
            .any(|entry| entry.condition.as_str() == "probe_guard"));
    }
    assert_eq!(game, restored);
}

#[test]
fn self_defense_equal_family_catalog_uses_one_baseline_for_every_funded_candidate() {
    let mut game = fixture(|doc, _| {
        ability(doc)["strikes"]["strikes"][0]["on_success"][0]["condition"] = json!("probe_shell");
        for index in 0..256 {
            let name = format!("equal_{index:03}");
            let mut candidate = definition(&name, "probe_equal");
            candidate["ai"]["priority"] = json!(31);
            doc["abilities"].as_array_mut().unwrap().push(candidate);
            doc["constants"]["default_abilities"]
                .as_array_mut()
                .unwrap()
                .push(json!(name));
        }
    });
    apply(&mut game, "probe_guard", 1);
    let before = crate::diagnostics::work_counts();
    assert_eq!(
        game.choose_ai(OWNER).unwrap().action,
        action("probe_defense")
    );
    let after = crate::diagnostics::work_counts();
    assert_eq!(
        after.ai_defense_candidates - before.ai_defense_candidates,
        257
    );
    assert_eq!(
        after.ai_defense_source_scans - before.ai_defense_source_scans,
        4
    );
    assert_eq!(after.ability_plans - before.ability_plans, 1);
    assert_eq!(after.scenes - before.scenes, 1);
    assert_eq!(after.health_derivations - before.health_derivations, 0);
    assert_eq!(after.game_clones - before.game_clones, 0);
}

#[test]
fn self_defense_same_family_cancellation_compares_effective_sign() {
    for (harm, useful) in [(1, true), (2, false)] {
        let mut game = fixture(|doc, _| {
            let equal = doc["conditions"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .find(|entry| entry["id"] == "probe_equal")
                .unwrap();
            equal["modifiers"][0]["op"]["value"] = json!(harm);
        });
        apply(&mut game, "probe_equal", 1000);
        assert_eq!(
            game.choose_ai(OWNER).unwrap().action == action("probe_defense"),
            useful
        );
    }
}

#[test]
fn self_defense_own_immunity_and_full_condition_capacity_reject_wasted_protection() {
    let mut immune = fixture(|doc, _| {
        ability(doc)["strikes"]["strikes"][0]["on_success"][0]["condition"] = json!("probe_shell");
    });
    apply(&mut immune, "probe_immunity", 1000);
    assert_ne!(
        immune.choose_ai(OWNER).unwrap().action,
        action("probe_defense")
    );
    let mut full = fixture(|doc, _| {
        for index in 0..crate::limits::CONDITIONS {
            doc["conditions"].as_array_mut().unwrap().push(condition(
                &format!("filler_{index}"),
                &format!("filler_{index}"),
                json!({"kind":"add","stat":{"kind":"health"},"value":1}),
            ));
        }
    });
    for index in 0..crate::limits::CONDITIONS {
        apply(&mut full, &format!("filler_{index}"), 1000);
    }
    let before = full.clone();
    assert_ne!(
        full.choose_ai(OWNER).unwrap().action,
        action("probe_defense")
    );
    assert_eq!(full, before);
}

#[test]
fn self_defense_paid_supply_claims_survive_resume_and_spend_exactly_once() {
    let mut game = fixture(|doc, _| {
        ability(doc)["supply_costs"] = json!([{"supply":"bandage","quantity":1,"access":"carried_or_ground_touch","spend_at":"first_resolution"}]);
    });
    let mut supply = crate::ItemSpec::ordinary("plain bandage".into());
    supply.supply = Some(crate::content_ids::SupplyId::new("bandage").unwrap());
    game.place_item_stack(20, at(1, 2, 1), Some(OWNER), 1, supply)
        .unwrap();
    prepare(&mut game);
    game.pause_preparation(OWNER).unwrap();
    let before = game.clone();
    assert_eq!(
        game.choose_ai(OWNER).unwrap().action,
        action("probe_defense")
    );
    assert_eq!(game, before);
    game.admit_ai_intention(OWNER).unwrap();
    game.execute_next_intention().unwrap().outcome.unwrap();
    for _ in 0..10 {
        if game.preparation(OWNER).is_none() {
            break;
        }
        game.act(game.next_actor().unwrap(), Action::Wait).unwrap();
    }
    assert!(game.preparation(OWNER).is_none());
    assert!(!game.items.contains_key(&crate::ItemId(20)));
    assert!(game
        .creature(OWNER)
        .unwrap()
        .conditions()
        .instances()
        .any(|entry| entry.condition.as_str() == "probe_guard"));
}

#[test]
fn self_defense_reserved_token_blocks_an_equivalent_fresh_ability() {
    let mut game = fixture(|doc, owner| {
        let token = doc["conditions"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|entry| entry["id"] == "probe_guard")
            .unwrap();
        token["lifecycle"] = json!("next_check_against");
        token["family"] = json!("probe_token_family");
        token["modifiers"][0]["op"]["stat"]["defense"] = Value::Null;
        let alias = definition("probe_alias", "probe_guard");
        doc["abilities"].as_array_mut().unwrap().push(alias);
        owner.modifiers.push(Modifier {
            cancellable: false,
            scope: Scope::Always,
            op: ModifierOp::GrantAbility {
                ability: id("probe_alias"),
            },
        });
    });
    apply(&mut game, "probe_guard", 1);
    let tokens = game
        .creature(OWNER)
        .unwrap()
        .conditions()
        .qualifying_tokens(game.content().rules(), OWNER, true);
    assert_eq!(tokens.len(), 1);
    game.actors
        .get_mut(&OWNER)
        .unwrap()
        .combat
        .as_mut()
        .unwrap()
        .reserve_tokens(&tokens, FOE)
        .unwrap();
    game.actors
        .get_mut(&OWNER)
        .unwrap()
        .combat
        .as_mut()
        .unwrap()
        .advance_active(2);
    let before = game.clone();
    let choice = game.choose_ai(OWNER).unwrap();
    assert_ne!(choice.action, action("probe_defense"));
    assert_ne!(choice.action, action("probe_alias"));
    assert_eq!(
        game.creature(OWNER)
            .unwrap()
            .conditions()
            .instances()
            .next()
            .unwrap()
            .reserved_by,
        Some(FOE)
    );
    assert_eq!(game, before);
}

#[test]
fn self_defense_token_families_reject_distinct_condition_aliases() {
    let mut document = serde_json::to_value(test_creatures::ruleset().document()).unwrap();
    let op = json!({"kind":"add","stat":{"kind":"incoming_edge","defense":null},"value":-1});
    let mut token = condition("probe_token", "unique_token_family", op.clone());
    token["lifecycle"] = json!("next_check_against");
    let sibling = condition("probe_sibling", "unique_token_family", op);
    document["conditions"]
        .as_array_mut()
        .unwrap()
        .extend([token, sibling]);
    assert!(matches!(Ruleset::validate(serde_json::from_value(document).unwrap()),
        Err(crate::rules::RulesetError::Bound { field }) if field == "token_family"),
        "distinct conditions cannot share a reserved one-use family; equivalent abilities must use its single condition");
}

#[test]
fn self_defense_stronger_category_candidate_remains_useful_over_a_weaker_condition() {
    let mut game = fixture(|doc, _| {
        ability(doc)["strikes"]["strikes"][0]["on_success"][0]["condition"] = json!("probe_strong");
    });
    apply(&mut game, "probe_shell", 1000);
    assert_eq!(
        game.choose_ai(OWNER).unwrap().action,
        action("probe_defense")
    );
}
