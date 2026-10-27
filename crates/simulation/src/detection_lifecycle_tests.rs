//! Paid native lifecycle and independent strict operand/receipt contracts.
use super::*;
use crate::abilities::{AttemptedParticipation, OutcomeStatus};
use crate::content_ids::{ConditionFamilyId, ConditionId};
use crate::perception::contract_fixtures::{OWNER, SUBJECT};
use crate::rules::*;
use crate::{Action, ItemDiscovery, ItemId, Work};
use std::num::NonZeroU64;
use tor_world::RegionId;

pub(crate) fn condition() -> ConditionId {
    ConditionId::new("fixture_monster_chart").unwrap()
}
fn definition() -> ConditionDef {
    ConditionDef {
        detecting_monsters: true,
        hallucinating: false,
        id: condition(),
        family: ConditionFamilyId::new("fixture_monster_chart").unwrap(),
        name: "Monster sense".into(),
        lifecycle: ConditionLifecycle::Timed,
        modifiers: vec![],
        stacking: Stacking::Refresh,
        source_scoped: false,
        descriptors: vec![],
        capacity: Eviction::Reject,
        timing: ConditionTiming::expiring(),
        restrictions: vec![],
        preparation: ConditionPreparation::Preserve,
    }
}
pub(crate) fn fixture() -> Game {
    crate::perception::contract_fixtures::fixture_with(&[], &[], |document, _, _| {
        document.conditions.push(definition());
    })
}
pub(crate) fn potion(game: &mut Game, discovery: ItemDiscovery, effect: Effect) -> ItemId {
    let mut spec = crate::ItemSpec::ordinary("unmarked dose".into());
    spec.identity = "fixture.monster_chart".into();
    spec.concealed = true;
    spec.class = crate::ItemClass::Potion;
    spec.stackable = true;
    spec.consumable = Some(crate::ConsumableSpec {
        descriptors: vec![],
        discovery,
        effects: vec![effect],
    });
    game.place_item_stack(70, game.actors[&OWNER].location, Some(OWNER), 3, spec)
        .unwrap()
}
pub(crate) fn dose() -> Effect {
    Effect::DetectMonsters {
        condition: condition(),
    }
}
pub(crate) fn start(game: &mut Game, item: ItemId) -> u64 {
    crate::hallucination_clock::integration_tests::start(game, item)
}
pub(crate) fn finish(game: &mut Game, due: u64) {
    crate::hallucination_clock::integration_tests::finish(game, due);
}
fn remaining(game: &Game) -> Option<(u64, u64)> {
    let actor = &game.actors[&OWNER];
    let elapsed = game.actor_clock(OWNER).saturating_sub(actor.active_since);
    actor
        .combat
        .as_ref()
        .unwrap()
        .creature()
        .conditions()
        .instances()
        .find(|instance| instance.condition == condition())
        .map(|instance| {
            (
                instance.generation,
                instance.remaining.unwrap().saturating_sub(elapsed),
            )
        })
}
fn advance_to(game: &mut Game, until: u64) {
    while game.tick() < until {
        let Some(actor) = game.next_actor() else {
            assert!(game.actors.keys().all(|id| game.actor_frozen(*id)));
            assert_eq!(game.advance_physics(until).unwrap(), until);
            break;
        };
        assert!(actor == OWNER || actor == SUBJECT);
        assert_eq!(game.actors[&actor].ready_at, game.tick());
        if actor == SUBJECT {
            game.actors.get_mut(&actor).unwrap().turn_ticks =
                NonZeroU64::new(until - game.tick()).unwrap();
        }
        game.act(actor, Action::Wait).unwrap();
        assert!(game.tick() <= until);
    }
}
fn roundtrip(game: &Game) -> Game {
    crate::hallucination_clock::integration_tests::roundtrip(game)
}

#[test]
fn marker_is_unique_pure_refresh_and_author_omission_is_false() {
    let baseline = definition();
    validate_conditions(std::slice::from_ref(&baseline)).unwrap();
    let mut document = crate::test_creatures::ruleset().document().clone();
    document.conditions.push(baseline.clone());
    Ruleset::validate(document.clone()).unwrap();
    let mut absent = serde_json::to_value(&baseline).unwrap();
    absent.as_object_mut().unwrap().remove("detecting_monsters");
    assert!(
        !serde_json::from_value::<ConditionDef>(absent)
            .unwrap()
            .detecting_monsters
    );
    for value in [
        serde_json::Value::Null,
        serde_json::json!("true"),
        serde_json::json!(1),
    ] {
        let mut invalid = serde_json::to_value(&baseline).unwrap();
        invalid["detecting_monsters"] = value;
        assert!(serde_json::from_value::<ConditionDef>(invalid).is_err());
    }
    let mut duplicate = baseline.clone();
    duplicate.id = ConditionId::new("other_monster_chart").unwrap();
    assert!(validate_conditions(&[baseline.clone(), duplicate]).is_err());
    let mut variants = Vec::new();
    for stacking in [
        Stacking::Keep,
        Stacking::Extend,
        Stacking::Longest,
        Stacking::Stack { maximum: 2 },
    ] {
        let mut row = baseline.clone();
        row.stacking = stacking;
        variants.push(row);
    }
    for timing in [
        ConditionTiming::UntilRemoved,
        ConditionTiming::ActorActiveOrUntilRemoved,
        ConditionTiming::ActorActive {
            on_expiry: ConditionExpiry::Protect {
                duration: ConditionTicks::new(20).unwrap(),
            },
        },
    ] {
        let mut row = baseline.clone();
        row.timing = timing;
        variants.push(row);
    }
    for lifecycle in [
        ConditionLifecycle::NextCheck,
        ConditionLifecycle::NextCheckAgainst,
    ] {
        let mut row = baseline.clone();
        row.lifecycle = lifecycle;
        variants.push(row);
    }
    let mut row = baseline.clone();
    row.source_scoped = true;
    variants.push(row);
    let mut row = baseline.clone();
    row.hallucinating = true;
    variants.push(row);
    let mut row = baseline.clone();
    row.preparation = ConditionPreparation::Pause;
    variants.push(row);
    let mut row = baseline.clone();
    row.restrictions
        .push(ConditionRestriction::VoluntaryActions);
    variants.push(row);
    let mut row = baseline.clone();
    row.descriptors
        .push(crate::content_ids::DescriptorId::new("fire").unwrap());
    variants.push(row);
    let mut row = baseline;
    row.modifiers.push(Modifier {
        cancellable: false,
        scope: Scope::Always,
        op: ModifierOp::Perception {
            value: crate::perception::PerceptionValue::SeeInvisible { enabled: true },
        },
    });
    variants.push(row);
    for row in variants {
        assert!(validate_conditions(&[row]).is_err());
    }
}

fn frozen_plan(effects: Vec<PlannedEffect>) -> crate::abilities::Plan {
    crate::abilities::Plan::Drink {
        actor: OWNER,
        item: ItemId(70),
        preparation: 100,
        descriptors: vec![],
        discovery: ItemDiscovery::SelfEffect,
        effects,
    }
}

#[test]
fn source_and_frozen_operands_are_exact_and_twenty_base_turns_are_bounded() {
    let game = fixture();
    for effect in [dose(), Effect::DetectFood, Effect::DetectMagic] {
        game.content
            .rules()
            .validate_effects(std::slice::from_ref(&effect))
            .unwrap();
        let encoded = serde_json::to_value(&effect).unwrap();
        assert_eq!(
            serde_json::from_value::<Effect>(encoded.clone()).unwrap(),
            effect
        );
        let mut extra = encoded;
        extra["duration"] = serde_json::json!(20);
        assert!(serde_json::from_value::<Effect>(extra).is_err());
        assert!(game
            .content
            .rules()
            .validate_effects(&[effect, Effect::DetectFood])
            .is_err());
    }
    for raw in [
        r#"{"kind":"detect_monsters"}"#,
        r#"{"kind":"detect_monsters","condition":null}"#,
        r#"{"kind":"detect_monsters","condition":"fixture_monster_chart","condition":"fixture_monster_chart"}"#,
        r#"["detect_food"]"#,
    ] {
        assert!(serde_json::from_str::<Effect>(raw).is_err());
    }
    assert_eq!(duration(100), Some(2000));
    assert_eq!(duration(0), None);
    let maximum = crate::limits::NUMERIC as u64 / 20;
    assert_eq!(duration(maximum), Some(maximum * 20));
    for ticks in [maximum + 1, u64::MAX] {
        assert_eq!(duration(ticks), None);
    }
    let valid =
        serde_json::json!({"kind":"detect_monsters","id":"fixture_monster_chart","turn_ticks":100});
    let effect: PlannedEffect = serde_json::from_value(valid.clone()).unwrap();
    assert!(frozen_plan(vec![effect]).valid(game.content.rules()));
    for value in [
        serde_json::json!(0),
        serde_json::json!(maximum + 1),
        serde_json::Value::Null,
    ] {
        let mut invalid = valid.clone();
        invalid["turn_ticks"] = value;
        assert!(serde_json::from_value::<PlannedEffect>(invalid).is_err());
    }
    let mut missing = valid;
    missing.as_object_mut().unwrap().remove("turn_ticks");
    assert!(serde_json::from_value::<PlannedEffect>(missing).is_err());
    let wrong = PlannedEffect::DetectMonsters {
        id: ConditionId::new("fixture_blind").unwrap(),
        turn_ticks: 100,
    };
    assert!(!frozen_plan(vec![wrong]).valid(game.content.rules()));
    for ticks in [0, 1, 19, 21, u64::MAX] {
        assert!(serde_json::from_value::<ResolvedEffect>(serde_json::json!({
            "kind":"detect_monsters","id":"fixture_monster_chart","duration":ticks,"changed":true
        }))
        .is_err());
    }
}

fn chart(positive: bool) -> crate::chart::Candidate {
    crate::chart::Candidate::from_complete(
        crate::chart::Kind::Food,
        vec![crate::chart::Component {
            alignment: crate::scope_projection::Alignment::Aligned,
        }],
        if positive {
            vec![crate::chart::Row {
                component: 0,
                position: Some([0, 0, 0]),
                mark: crate::chart::Mark::Food,
            }]
        } else {
            vec![]
        },
        crate::chart::Budget {
            components: 1,
            rows: 1,
            json_bytes: 4096,
        },
    )
    .unwrap()
}
fn outcome(effect: ResolvedEffect) -> Outcome {
    let mut outcome = Outcome {
        actor: OWNER,
        target: OWNER,
        ability: None,
        intention: None,
        result: OutcomeStatus::Unaffected,
        check: None,
        resistance: None,
        participation: if effect.recipient() == crate::abilities::EffectRecipient::Source {
            AttemptedParticipation::source()
        } else {
            AttemptedParticipation::target()
        },
        target_rider_participated: false,
        effects: vec![effect],
    };
    outcome.result = outcome.canonical_result();
    outcome
}
#[test]
fn final_receipt_controls_identification_independently_of_mutation_truth() {
    for candidate in [
        chart(false),
        chart(true),
        crate::chart::Candidate::unavailable(crate::chart::Unavailable::Budget),
    ] {
        let receipt =
            CompletionReceipt::from_admitted(crate::chart::Kind::Food, &candidate).unwrap();
        for changed in [false, true] {
            let result = ItemCompletion::new(
                outcome(ResolvedEffect::DetectFood { changed }),
                Some(receipt),
            )
            .unwrap();
            assert_eq!(result.outcome.result == OutcomeStatus::Applied, changed);
            assert_eq!(
                result.identifies(),
                candidate
                    .chart()
                    .is_some_and(|chart| !chart.rows().is_empty())
            );
        }
        assert!(ItemCompletion::new(
            outcome(ResolvedEffect::DetectMagic { changed: true }),
            Some(receipt)
        )
        .is_err());
    }
    assert!(
        ItemCompletion::new(outcome(ResolvedEffect::DetectFood { changed: true }), None).is_err()
    );
    assert!(CompletionReceipt::from_admitted(crate::chart::Kind::Monsters, &chart(true)).is_err());
    let ordinary = outcome(ResolvedEffect::Heal { amount: 1 });
    assert!(ItemCompletion::new(ordinary, None).unwrap().identifies());
}

#[test]
fn paid_unavailable_chart_starts_full_lifetime_but_does_not_identify() {
    let mut game = fixture();
    assert!(
        game.spatial_registry.scopes().next().is_none(),
        "deliberately unavailable scope, not an empty population"
    );
    let item = potion(&mut game, ItemDiscovery::SelfEffect, dose());
    let plan = game.plan_work(OWNER, Work::Drink { item }).unwrap();
    assert!(matches!(
        plan.on_success(),
        [PlannedEffect::DetectMonsters {
            turn_ticks: 100,
            ..
        }]
    ));
    let rng = game.rules_rng;
    let due = start(&mut game, item);
    assert!(!game.monster_detection_active(OWNER));
    finish(&mut game, due);
    assert_eq!(remaining(&game), Some((0, 2000)));
    assert!(game.monster_detection_active(OWNER));
    assert_eq!(game.items[&item].quantity, 2);
    assert_eq!(game.rules_rng, rng);
    assert!(!game.actors[&OWNER]
        .knowledge
        .contains("fixture.monster_chart"));
    assert!(game.combat_events().iter().any(|event| matches!(event,
        crate::combat::CombatEvent::Resolved { outcome, .. } if outcome.result == OutcomeStatus::Applied
        && matches!(outcome.effects.as_slice(), [ResolvedEffect::DetectMonsters { duration: 2000, changed: true, .. }]))));
    assert_eq!(roundtrip(&game), game);
}

#[test]
fn detection_receipt_does_not_change_on_use_never_or_ordinary_self_effect() {
    for (discovery, known) in [(ItemDiscovery::OnUse, true), (ItemDiscovery::Never, false)] {
        let mut game = fixture();
        let item = potion(&mut game, discovery, dose());
        let due = start(&mut game, item);
        finish(&mut game, due);
        assert_eq!(
            game.actors[&OWNER]
                .knowledge
                .contains("fixture.monster_chart"),
            known
        );
        assert_eq!(remaining(&game), Some((0, 2000)));
    }
    let mut game = fixture();
    game.apply_condition(OWNER, &condition(), OWNER, 1000)
        .unwrap();
    let item = potion(
        &mut game,
        ItemDiscovery::SelfEffect,
        Effect::RemoveCondition {
            condition: condition(),
        },
    );
    let due = start(&mut game, item);
    finish(&mut game, due);
    assert!(!game.monster_detection_active(OWNER));
    assert!(game.actors[&OWNER]
        .knowledge
        .contains("fixture.monster_chart"));
}

#[test]
fn fractional_interval_refresh_reuses_generation_and_replaces_not_extends_lifetime() {
    let mut game = fixture();
    advance_to(&mut game, 50);
    game.apply_condition(OWNER, &condition(), SUBJECT, 1000)
        .unwrap();
    advance_to(&mut game, 100);
    assert_eq!(remaining(&game), Some((0, 950)));
    let item = potion(&mut game, ItemDiscovery::SelfEffect, dose());
    let due = start(&mut game, item);
    assert_eq!(due, 200);
    finish(&mut game, due);
    assert_eq!(
        remaining(&game),
        Some((0, 2000)),
        "settle the old fractional interval, then exact Refresh, never Extend or a second clock"
    );
    assert_eq!(game.next_creature_event_tick(), Some(2200));
    assert_eq!(roundtrip(&game), game);
}

#[test]
fn ordinary_peer_drives_exact_expiry_and_frozen_detached_time_pauses() {
    use crate::streaming::{MemoryRecords, RecordStore, RegionRecordData, RegionTransition};
    for detached in [false, true] {
        let mut game = fixture();
        game.apply_condition(OWNER, &condition(), SUBJECT, 200)
            .unwrap();
        advance_to(&mut game, 50);
        let mut records = MemoryRecords::default();
        game.apply_region_transition(
            &RegionTransition {
                active: Default::default(),
                loaded: if detached {
                    Default::default()
                } else {
                    [RegionId(1)].into()
                },
            },
            &mut records,
        )
        .unwrap();
        let mut reopened_records = MemoryRecords::default();
        for id in records.ids().collect::<Vec<_>>() {
            let encoded =
                serde_json::to_vec(&RegionRecordData::capture(&records.get(id).unwrap())).unwrap();
            let decoded: RegionRecordData = serde_json::from_slice(&encoded).unwrap();
            reopened_records.put(
                id,
                tor_world::Shared::new(decoded.restore(game.content().clone()).unwrap()),
            );
        }
        let mut game = roundtrip(&game);
        advance_to(&mut game, 500);
        game.apply_region_transition(
            &RegionTransition {
                active: [RegionId(1)].into(),
                loaded: [RegionId(1)].into(),
            },
            &mut reopened_records,
        )
        .unwrap();
        assert_eq!(remaining(&game), Some((0, 150)));
        assert_eq!(game.next_creature_event_tick(), Some(650));
        advance_to(&mut game, 649);
        assert!(game.monster_detection_active(OWNER));
        advance_to(&mut game, 650);
        assert!(!game.monster_detection_active(OWNER));
        assert_eq!(remaining(&game), None);
        assert_eq!(roundtrip(&game), game);
    }
}

#[test]
fn detection_source_rejects_checked_targeted_mixed_and_rider_contexts() {
    let game = fixture();
    let mut baseline = game.content.rules().document().clone();
    let row = baseline
        .abilities
        .iter_mut()
        .find(|row| row.id.as_str() == "fixture_search")
        .unwrap();
    let StrikeSource::Authored { strikes } = &mut row.strikes else {
        unreachable!()
    };
    strikes[0].on_success = vec![dose()];
    Ruleset::validate(baseline.clone()).unwrap();
    for mutation in 0..5 {
        let mut invalid = baseline.clone();
        let row = invalid
            .abilities
            .iter_mut()
            .find(|row| row.id.as_str() == "fixture_search")
            .unwrap();
        if mutation == 0 {
            row.reach = Reach::Sight { range: 5 };
        }
        let StrikeSource::Authored { strikes } = &mut row.strikes else {
            unreachable!()
        };
        match mutation {
            1 => {
                let StrikeSource::Authored { strikes: checked } =
                    crate::test_creatures::paid_melee().strikes
                else {
                    unreachable!()
                };
                assert!(checked[0].check.is_some());
                strikes[0].check = checked[0].check.clone();
            }
            2 => strikes[0].on_success.push(Effect::DetectFood),
            3 => {
                strikes[0].on_success.clear();
                strikes[0].riders.push(RiderDef {
                    cancellable: false,
                    trigger: RiderTrigger::Success,
                    condition: None,
                    check: None,
                    effects: vec![dose()],
                    descriptors: vec![],
                });
            }
            4 => {
                strikes[0].on_success.clear();
                strikes[0].on_failure.push(dose());
            }
            _ => {}
        }
        assert!(Ruleset::validate(invalid).is_err(), "mutation {mutation}");
    }
}

#[test]
fn pending_current_and_retained_actor_roots_reject_wrong_marker_and_mixed_payload() {
    let mut game = fixture();
    let item = potion(&mut game, ItemDiscovery::SelfEffect, dose());
    let due = start(&mut game, item);
    let mut shared = crate::checkpoint::SharedState::default();
    let history = serde_json::to_value(game.checkpoint(&mut shared)).unwrap();
    advance_to(&mut game, 50);
    assert!(game.tick() < due);
    let current = serde_json::to_value(game.checkpoint(&mut shared)).unwrap();
    let baseline = serde_json::to_value(&shared).unwrap();
    for snapshot in [history, current] {
        let restore = |stored: serde_json::Value| {
            Game::restore_checkpoint(
                serde_json::from_value(snapshot.clone()).unwrap(),
                &serde_json::from_value(stored).unwrap(),
                game.content().clone(),
            )
        };
        restore(baseline.clone()).unwrap();
        let root = snapshot["actors"].as_u64().unwrap() as usize;
        for mutation in 0..2 {
            let mut invalid = baseline.clone();
            let effects = &mut invalid["actors"][root]["1"]["pending"]["plan"]["effects"];
            assert_eq!(effects[0]["kind"], serde_json::json!("detect_monsters"));
            if mutation == 0 {
                effects[0]["id"] = serde_json::json!("fixture_blind");
            } else {
                effects
                    .as_array_mut()
                    .unwrap()
                    .push(serde_json::json!({"kind":"detect_food"}));
            }
            assert!(restore(invalid).is_err());
        }
        let mut missing = baseline.clone();
        missing["actors"][root]["1"]["pending"]["plan"]["effects"][0]
            .as_object_mut()
            .unwrap()
            .remove("turn_ticks");
        assert!(serde_json::from_value::<crate::checkpoint::SharedState>(missing).is_err());
    }
}

#[test]
fn pending_and_active_monster_pins_use_region_owned_scope_without_thawing() {
    use crate::spatial_scope::registry::{Budget, Scope};
    let mut game = fixture();
    let item = potion(&mut game, ItemDiscovery::SelfEffect, dose());
    let due = start(&mut game, item);
    game.register_spatial_scope(
        Scope {
            id: "fixture.chart.scope".into(),
            members: vec![RegionId(1), RegionId(2)],
            primary: vec![],
            rooms: vec![],
            mapping: crate::spatial_scope::mapping::MappingCertification::Uncertified,
        },
        &Budget {
            scopes: 1,
            members: 2,
            rooms: 0,
            passages: 0,
            runs: 0,
            expanded_refs: 0,
            label_bytes: 64,
        },
    )
    .unwrap();
    assert!(game
        .spatial_registry
        .lookup("fixture.chart.scope", game.actors[&OWNER].location)
        .is_none());
    let active: std::collections::BTreeSet<RegionId> = [RegionId(1)].into();
    let expected = [RegionId(1), RegionId(2)].into();
    assert_eq!(game.detection_required_regions(OWNER).unwrap(), expected);
    let before = game.clone();
    let mut loaded = active.clone();
    game.relocation_loaded_closure(&active, &mut loaded);
    assert_eq!(
        loaded, expected,
        "pending dose pins full region-owned scope"
    );
    assert_eq!(
        game, before,
        "planning may not load, thaw, enter floors, or draw randomness"
    );
    finish(&mut game, due);
    assert!(game.preparation(OWNER).is_none());
    assert!(game.monster_detection_active(OWNER));
    let needed = game.region_requirements(&active, &active);
    assert_eq!(
        needed.active, active,
        "monster capability is a load-only pin"
    );
    assert_eq!(needed.loaded, expected);
}

#[test]
fn retained_resolved_detection_rejects_nonself_mixed_and_impossible_duration() {
    let baseline = outcome(ResolvedEffect::DetectMonsters {
        id: condition(),
        duration: 2000,
        changed: true,
    });
    assert!(baseline.valid());
    for mutation in 0..4 {
        let mut invalid = baseline.clone();
        match mutation {
            0 => invalid.target = SUBJECT,
            1 => invalid
                .effects
                .push(ResolvedEffect::DetectFood { changed: true }),
            2 => {
                invalid.effects[0] = ResolvedEffect::DetectMonsters {
                    id: condition(),
                    duration: 21,
                    changed: true,
                }
            }
            3 => invalid.participation = AttemptedParticipation::target(),
            _ => unreachable!(),
        }
        assert!(!invalid.valid(), "mutation {mutation}");
        assert!(serde_json::from_value::<Outcome>(serde_json::to_value(invalid).unwrap()).is_err());
    }
}

#[test]
fn haste_and_slow_change_paid_preparation_not_captured_twenty_base_turns() {
    for (speed, preparation) in [
        (crate::action_duration::ActionDuration::Haste, 50),
        (crate::action_duration::ActionDuration::Slow, 200),
    ] {
        let mut game =
            crate::perception::contract_fixtures::fixture_with(&[], &[], |document, _, _| {
                document.conditions.push(definition());
                let mut timing = definition();
                timing.detecting_monsters = false;
                timing.id = ConditionId::new("fixture_timing").unwrap();
                timing.family = ConditionFamilyId::new("fixture_timing").unwrap();
                timing.modifiers = vec![Modifier {
                    cancellable: false,
                    scope: Scope::Always,
                    op: ModifierOp::ActionDuration { value: speed },
                }];
                document.conditions.push(timing);
            });
        game.apply_condition(
            OWNER,
            &ConditionId::new("fixture_timing").unwrap(),
            OWNER,
            1000,
        )
        .unwrap();
        let item = potion(&mut game, ItemDiscovery::SelfEffect, dose());
        let plan = game.plan_work(OWNER, Work::Drink { item }).unwrap();
        assert_eq!(plan.preparation(), preparation);
        assert!(matches!(
            plan.on_success(),
            [PlannedEffect::DetectMonsters {
                turn_ticks: 100,
                ..
            }]
        ));
        let due = start(&mut game, item);
        finish(&mut game, due);
        assert_eq!(remaining(&game).map(|(_, ticks)| ticks), Some(2000));
    }
}
