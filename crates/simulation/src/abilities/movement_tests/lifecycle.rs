//! Ordinary action lifecycle controls for shared one-cell translation.
use super::*;
// Builds variants through the same authored definition path as the main fixture.
pub(super) fn push_variant(
    source_first: bool,
    customize: impl FnOnce(&mut serde_json::Value),
) -> (crate::Game, crate::ActorId, crate::ActorId) {
    let facts = profile(CreatureSize::Medium, false);
    let original = push_content(facts, facts, true);
    let mut document = serde_json::to_value(original.rules().document()).unwrap();
    customize(&mut document);
    let rules = crate::rules::Ruleset::validate(serde_json::from_value(document).unwrap()).unwrap();
    let content = crate::creatures::Content::new(rules, original.catalog().clone()).unwrap();
    let mut game = crate::Game::new(
        tor_world::World::new(
            vec![tor_world::Region {
                id: tor_world::RegionId(1),
                name: "push lifecycle".into(),
                bounds: tor_world::Extent::new(9, 9, 9).unwrap(),
            }],
            vec![],
        )
        .unwrap(),
        42,
        content,
    );
    let rows = if source_first {
        [
            (cell(3, 3, 3), "pushing_source"),
            (cell(4, 3, 3), "pushed_target"),
        ]
    } else {
        [
            (cell(4, 3, 3), "pushed_target"),
            (cell(3, 3, 3), "pushing_source"),
        ]
    };
    for (at, name) in rows {
        let actor = game
            .spawn_actor(at, std::num::NonZeroU64::new(100).unwrap())
            .unwrap();
        configure_push_subject(&mut game, actor, name);
    }
    (
        game,
        crate::ActorId(if source_first { 1 } else { 2 }),
        crate::ActorId(if source_first { 2 } else { 1 }),
    )
}
pub(super) fn ability_action(id: &str, target: crate::ActorId) -> crate::Action {
    crate::Action::UseAbility {
        id: crate::content_ids::AbilityId::new(id).unwrap(),
        invocation: crate::abilities::AbilityInvocation::actor_target(target),
        supplies: Default::default(),
    }
}

#[test]
fn second_push_rider_reuses_the_primary_contact_and_cannot_select_a_new_hit() {
    let (mut game, source, target) = push_variant(true, |document| {
        let ability = document["abilities"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|ability| ability["id"] == "push_probe")
            .unwrap();
        let rider = ability["strikes"]["strikes"][0]["riders"][0].clone();
        ability["strikes"]["strikes"][0]["riders"]
            .as_array_mut()
            .unwrap()
            .push(rider);
    });
    start_push(&mut game);
    finish_push(&mut game);
    assert_eq!(game.actors[&target].location, cell(5, 3, 3));
    assert_eq!(game.health(target), Some((26, 30)));
    assert_eq!(game.actors[&source].ready_at, 150);
    let translated: Vec<_> = game
        .combat_events()
        .iter()
        .filter_map(|event| match event {
            crate::combat::CombatEvent::Resolved { outcome, .. } => Some(&outcome.effects),
            _ => None,
        })
        .flatten()
        .filter(|effect| matches!(effect, super::super::ResolvedEffect::Translate { .. }))
        .collect();
    assert_eq!(translated.len(), 2);
    assert!(matches!(
        translated[0],
        super::super::ResolvedEffect::Translate {
            result: super::super::MovementResult::Moved { .. },
            ..
        }
    ));
    assert!(matches!(
        translated[1],
        super::super::ResolvedEffect::Translate {
            result: super::super::MovementResult::Stale,
            ..
        }
    ));
}

#[test]
fn primary_miss_cannot_run_the_successful_push_rider() {
    let (mut game, source, target) = push_variant(true, |document| {
        let ability = document["abilities"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|ability| ability["id"] == "push_probe")
            .unwrap();
        ability["strikes"]["strikes"][0]["check"] = json!({
            "actor":"user","skill":{"kind":"selected_weapon"},
            "against":{"kind":"difficulty","formula":{"base":100000,"skill":null,"attribute":null}}
        });
    });
    start_push(&mut game);
    finish_push(&mut game);
    assert_eq!(game.actors[&target].location, cell(4, 3, 3));
    assert_eq!(game.health(target), Some((30, 30)));
    assert_eq!(game.actors[&source].ready_at, 150);
}

#[test]
fn whole_primary_immunity_skips_push_but_zero_actual_damage_still_pushes() {
    use crate::rules::{Modifier, ModifierOp, Scope, Selector};
    for whole in [false, true] {
        let (mut game, source, target) = push_variant(true, |document| {
            let ability = document["abilities"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .find(|ability| ability["id"] == "push_probe")
                .unwrap();
            ability["strikes"]["strikes"][0]["descriptors"] = json!(["fire"]);
            let mut condition = document["conditions"]
                .as_array()
                .unwrap()
                .iter()
                .find(|condition| condition["id"] == "braced")
                .unwrap()
                .clone();
            condition["id"] = json!("push_immunity");
            condition["family"] = json!("push_immunity");
            condition["modifiers"] = serde_json::to_value(vec![Modifier {
                cancellable: false,
                scope: Scope::Always,
                op: ModifierOp::Immunity {
                    selector: if whole {
                        Selector::Descriptor {
                            descriptor: crate::content_ids::DescriptorId::new("fire").unwrap(),
                        }
                    } else {
                        Selector::Category {
                            category: crate::combat::DamageType::Impact,
                        }
                    },
                },
            }])
            .unwrap();
            document["conditions"]
                .as_array_mut()
                .unwrap()
                .push(condition);
        });
        game.apply_condition(
            target,
            &crate::content_ids::ConditionId::new("push_immunity").unwrap(),
            target,
            1000,
        )
        .unwrap();
        let rng = game.rules_rng;
        start_push(&mut game);
        finish_push(&mut game);
        assert_eq!(game.health(target), Some((30, 30)));
        assert_eq!(
            game.actors[&target].location,
            if whole { cell(4, 3, 3) } else { cell(5, 3, 3) }
        );
        assert_eq!(game.rules_rng == rng, whole);
        assert_eq!(game.actors[&source].ready_at, 150);
        assert_eq!(
            game.creature(source)
                .unwrap()
                .costs()
                .resources()
                .balance(crate::resources::Resource::Stamina),
            18
        );
    }
}

#[test]
fn lethal_primary_finishes_without_displacing_the_dead_target() {
    let (mut game, source, target) = push_variant(true, |document| {
        let ability = document["abilities"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|ability| ability["id"] == "push_probe")
            .unwrap();
        ability["strikes"]["strikes"][0]["on_success"] = json!([{
            "kind":"damage","damage":{"kind":"components","components":[{
                "category":"impact","descriptor":null,"amount":{"kind":"fixed","amount":30}
            }]}
        }]);
    });
    start_push(&mut game);
    finish_push(&mut game);
    assert_eq!(game.health(target), Some((0, 30)));
    assert_eq!(game.actors[&target].location, cell(4, 3, 3));
    assert_eq!(game.actors[&source].ready_at, 150);
    assert_eq!(
        game.creature(source)
            .unwrap()
            .costs()
            .resources()
            .balance(crate::resources::Resource::Stamina),
        18
    );
    assert!(!game.combat_events().iter().any(|event| matches!(event,
        crate::combat::CombatEvent::Resolved {outcome,..} if outcome.effects.iter()
            .any(|effect|matches!(effect,super::super::ResolvedEffect::Translate {..})))));
}

#[test]
fn pushed_carer_retires_original_claim_once_in_both_actor_orders_and_when_paused() {
    for source_first in [false, true] {
        for paused in [false, true] {
            for carried in [false, true] {
                let (mut game, source, target) = push_variant(source_first, |document| {
                    let care = document["abilities"]
                        .as_array_mut()
                        .unwrap()
                        .iter_mut()
                        .find(|ability| ability["id"] == "bind_wounds")
                        .unwrap();
                    care["cost"] = json!({"resource":"stamina","start":1,"resolution":1});
                });
                game.set_combat_diagnostics(true);
                let idle = game
                    .spawn_actor(cell(7, 7, 3), std::num::NonZeroU64::new(100).unwrap())
                    .unwrap();
                let mut bandage = crate::ItemSpec::ordinary("claimed bandage".into());
                bandage.stackable = true;
                bandage.supply = Some(crate::content_ids::SupplyId::new("bandage").unwrap());
                let item = game
                    .place_item_stack(10, cell(4, 3, 3), carried.then_some(target), 2, bandage)
                    .unwrap();
                let care = crate::Action::UseAbility {
                    id: crate::content_ids::AbilityId::new("bind_wounds").unwrap(),
                    invocation: crate::abilities::AbilityInvocation::actor_target(target),
                    supplies: crate::supplies::SupplySelection::Explicit {
                        items: vec![crate::supplies::SelectedSupply {
                            cost_ordinal: 0,
                            item,
                        }],
                    },
                };
                let care_id = game
                    .admit_intention(target, care, crate::IntentionOrigin::Human)
                    .unwrap();
                game.admit_intention(
                    source,
                    ability_action("push_probe", target),
                    crate::IntentionOrigin::Human,
                )
                .unwrap();
                for _ in 0..2 {
                    game.execute_next_intention().unwrap().outcome.unwrap();
                }
                assert_eq!(game.tick(), 0, "the idle actor still owns an ordinary turn");
                assert_eq!(game.actor_clock(target), 0);
                let care_ability = crate::content_ids::AbilityId::new("bind_wounds").unwrap();
                assert_eq!(
                    game.creature(target)
                        .unwrap()
                        .cooldowns()
                        .remaining(&care_ability),
                    1200
                );
                let cooldown_deadline = 1200;
                assert_eq!(game.next_actor(), Some(idle));
                assert_eq!(game.profile_finite_claims().unwrap().held_units, 1);
                assert_eq!(game.items[&item].quantity, 2);
                assert_eq!(game.preparation(target).unwrap().intention, Some(care_id));
                if paused {
                    game.pause_preparation(target).unwrap();
                }
                for _ in 0..12 {
                    if game.preparation(source).is_none() {
                        break;
                    }
                    let actor = game.next_actor().unwrap();
                    assert_ne!(actor, source);
                    game.act(actor, crate::Action::Wait).unwrap();
                }
                assert!(game.preparation(source).is_none());
                assert!(game.preparation(target).is_none());
                let impact = game.combat_diagnostics().unwrap().records().back().unwrap();
                assert_eq!(
                    (impact.actor, impact.target, impact.started, impact.tick),
                    (source, target, 0, 110)
                );
                // A paused caregiver keeps its ordinary Wait recovery to 200; after
                // impact at 110 the source is the next decision, at 150. An active
                // caregiver is interrupted to the impact boundary itself, at 110.
                let returned = if paused { 150 } else { 110 };
                assert_eq!(
                    game.tick(),
                    returned,
                    "source_first={source_first}, paused={paused}, carried={carried}"
                );
                assert_eq!(game.actor_clock(target), returned);
                assert_eq!(game.profile_finite_claims().unwrap().held_units, 0);
                assert_eq!(game.profile_finite_claims().unwrap().item_holds, 0);
                assert_eq!(game.items[&item].quantity, 2);
                assert_eq!(
                    game.creature(target)
                        .unwrap()
                        .costs()
                        .resources()
                        .balance(crate::resources::Resource::Stamina),
                    20
                );
                let remaining = game
                    .creature(target)
                    .unwrap()
                    .cooldowns()
                    .remaining(&care_ability);
                assert_eq!(remaining, if paused { 1050 } else { 1090 });
                assert_eq!(
                    game.actor_clock(target) + remaining,
                    cooldown_deadline,
                    "the original cooldown is neither refunded nor restarted"
                );
                assert_eq!(game.actors[&target].location, cell(5, 3, 3));
                assert_eq!(game.actors[&source].ready_at, 150);
                assert_eq!(game.combat_events().iter().filter(|event|matches!(event,
            crate::combat::CombatEvent::SequenceEnded {actor,intention:Some(id),reason:crate::combat::SequenceEnd::Abandoned}
                if *actor==target && *id==care_id)).count(),1);
                assert!(
                    game.cancel_intention(target, care_id).is_err(),
                    "a retired owner cannot cancel or refund twice"
                );
                assert_eq!(game.items[&item].quantity, 2);
            }
        }
    }
}

#[test]
fn pushed_entry_allocates_stock_before_later_lethal_effect_in_the_same_batch() {
    use crate::floor_activation::{
        DepthBand, DepthInterval, EntryCause, PlacementDecision, PopulationContent,
        PopulationEntity, PopulationPlan, PopulationProfile, PopulationStage, PreparedItemChoice,
        PreparedScope, ScenarioCapabilities, StageResult,
    };
    use std::collections::{BTreeMap, BTreeSet};
    use tor_world::{Extent, Location, Passage, Region, RegionId};

    let (mut game, source, protagonist) = push_variant(true, |document| {
        // Isolate payment/retirement from the ordinary 100-tick Stamina
        // recovery that would refill the start payment during preparation.
        let stamina = document["resources"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|resource| resource["resource"] == "stamina")
            .unwrap();
        stamina["recovery_period"] = json!(1000);
        let ability = document["abilities"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|ability| ability["id"] == "push_probe")
            .unwrap();
        ability["strikes"]["strikes"][0]["on_success"] = json!([
            {"kind":"push_target_one_cell","direction":"away_from_source"},
            {"kind":"damage","damage":{"kind":"selected_weapon","impact_bonus":0}}
        ]);
        ability["strikes"]["strikes"][0]["riders"] = json!([]);
    });
    game.configure_scenario_policy(ScenarioCapabilities::default(), Some(protagonist))
        .unwrap();
    game.add_region(Region {
        id: RegionId(2),
        name: "entered floor".into(),
        bounds: Extent::new(9, 9, 9).unwrap(),
    })
    .unwrap();
    game.teleport(source, cell(7, 3, 3)).unwrap();
    game.teleport(protagonist, cell(8, 3, 3)).unwrap();
    let arrival = Location {
        region: RegionId(2),
        ..cell(0, 3, 3)
    };
    let stock_at = Location {
        region: RegionId(2),
        ..cell(3, 3, 3)
    };
    game.connect_portal_area(
        Passage {
            from: cell(8, 3, 3),
            direction: tor_world::Direction::East,
            to: arrival,
        },
        0,
        1,
        1,
    )
    .unwrap();
    game.configure_population(PopulationPlan {
        carried_item_pools: BTreeMap::new(),
        protagonist,
        profiles: BTreeMap::from([(
            "baseline".into(),
            PopulationProfile {
                version: 1,
                stages: vec![PopulationStage {
                    id: "stock".into(),
                    version: 1,
                    count: [1, 1],
                    population: PopulationContent::Items {
                        bands: vec![DepthBand {
                            depth: DepthInterval::Open { minimum: 2 },
                            choices: vec![PreparedItemChoice {
                                content: "entry marker".into(),
                                weight: 1,
                                quantity: crate::quantity::QuantityExpr::Fixed { quantity: 1 },
                                spec: crate::ItemSpec::ordinary("entry marker".into()),
                            }],
                        }],
                    },
                }],
            },
        )]),
    })
    .unwrap();
    for (depth, stock) in [(1, cell(6, 6, 3)), (2, stock_at)] {
        game.register_activation_scope(PreparedScope {
            rooms: vec![],
            scope: format!("dungeon/{depth}"),
            depth,
            profile: "baseline".into(),
            members: BTreeSet::from([stock.region]),
            cells: vec![stock],
            protected: BTreeSet::new(),
        })
        .unwrap();
    }
    game.activate_initial_floor().unwrap();
    crate::test_creatures::injure(&mut game, protagonist, 29).unwrap();
    assert_eq!(game.health(protagonist), Some((1, 30)));
    let starting_item_id = game.next_item_id;
    let stamina = |game: &crate::Game, actor| {
        game.creature(actor)
            .unwrap()
            .costs()
            .resources()
            .balance(crate::resources::Resource::Stamina)
    };
    let source_stamina = stamina(&game, source);
    let target_stamina = stamina(&game, protagonist);
    // Hold the scheduler at tick zero long enough to prove that the pushed
    // character owns paid pending work which the spatial prefix must retire.
    game.spawn_actor(cell(1, 1, 3), std::num::NonZeroU64::new(100).unwrap())
        .unwrap();
    start_push(&mut game);
    let target_work = game
        .admit_intention(
            protagonist,
            ability_action("push_probe", source),
            crate::IntentionOrigin::Human,
        )
        .unwrap();
    game.execute_next_intention().unwrap().outcome.unwrap();
    assert_eq!(
        game.preparation(protagonist).unwrap().intention,
        Some(target_work)
    );
    finish_push(&mut game);
    assert_eq!(game.actors[&protagonist].location, arrival);
    assert!(
        !game.alive(protagonist),
        "the later ordinary damage remains lethal"
    );
    let records = game.activations_since(0).unwrap();
    assert_eq!(
        records.len(),
        2,
        "initial and pushed floors activate exactly once"
    );
    assert_eq!(
        (records[0].ordinal, records[0].scope.as_str()),
        (0, "dungeon/1")
    );
    let entry = &records[1];
    assert_eq!(
        (
            entry.ordinal,
            entry.scope.as_str(),
            entry.entrant,
            entry.tick,
            entry.cause
        ),
        (1, "dungeon/2", protagonist, 110, EntryCause::Physics)
    );
    let StageResult::Evaluated { attempts, .. } = &entry.decisions[0].result else {
        panic!("entered floor must evaluate stock");
    };
    let PlacementDecision::Placed {
        entity: PopulationEntity::Item { id: stock, .. },
        location,
        ..
    } = &attempts[0]
    else {
        panic!("entered floor must place stock");
    };
    assert_eq!(*location, stock_at);
    let corpse = game
        .items
        .iter()
        .find_map(|(id, item)| (item.spec.class == crate::ItemClass::Corpse).then_some(*id))
        .expect("ordinary death must allocate one corpse");
    assert_eq!(
        (stock.0, corpse.0),
        (starting_item_id, starting_item_id + 1),
        "first red: accepted push entry must allocate its stock before a later lethal effect allocates the corpse"
    );
    let resolved = game
        .combat_events()
        .iter()
        .find_map(|event| match event {
            crate::combat::CombatEvent::Resolved { outcome, .. }
                if outcome.actor == source
                    && outcome
                        .ability
                        .as_ref()
                        .is_some_and(|id| id.as_str() == "push_probe") =>
            {
                Some(outcome)
            }
            _ => None,
        })
        .unwrap();
    assert!(matches!(resolved.effects.as_slice(), [
        crate::abilities::ResolvedEffect::Translate { result, .. },
        crate::abilities::ResolvedEffect::Damage { amount: 1 },
    ] if result.changed()));
    let resolution_indices: Vec<_> = game.combat_events().iter().enumerate().filter_map(|(index, event)| {
        matches!(event, crate::combat::CombatEvent::Resolved { outcome, .. }
            if outcome.actor == source && outcome.ability.as_ref().is_some_and(|id| id.as_str() == "push_probe"))
            .then_some(index)
    }).collect();
    assert_eq!(
        resolution_indices.len(),
        1,
        "the original batch is published exactly once"
    );
    let consequences: Vec<_> = game
        .combat_events()
        .iter()
        .enumerate()
        .filter_map(|(index, event)| {
            matches!(event,
                crate::combat::CombatEvent::Interrupted { actor, .. }
                | crate::combat::CombatEvent::SequenceEnded { actor, .. }
                | crate::combat::CombatEvent::Died { actor, .. } if *actor == protagonist
            )
            .then_some(index)
        })
        .collect();
    assert!(!consequences.is_empty());
    assert!(
        consequences
            .iter()
            .all(|index| *index > resolution_indices[0]),
        "prefix interruption and death stay after the single original Resolved event"
    );
    assert!(game.preparation(protagonist).is_none());
    assert!(game
        .creature(protagonist)
        .unwrap()
        .costs()
        .reservations()
        .is_empty());
    assert!(
        game.tick() < 1000,
        "the payment probe must finish before resource recovery"
    );
    assert_eq!(
        stamina(&game, source),
        source_stamina - 3,
        "no replayed payment"
    );
    assert_eq!(
        stamina(&game, protagonist),
        target_stamina - 1,
        "paid start is not refunded"
    );
    game.activate_initial_floor().unwrap();
    assert_eq!(game.activations_since(0).unwrap(), records);
    assert_eq!(game.next_item_id, starting_item_id + 2);
}
