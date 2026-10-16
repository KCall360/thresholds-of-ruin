//! Paid public-plan observation uses ordinary effects and immutable aim operands.
use serde_json::{json, Value};
use tor_simulation::abilities::AbilityPlan;
use tor_simulation::rules::Effect;
use tor_simulation::Action;

fn observation_duel(
    known: bool,
) -> (
    tor_simulation::Game,
    tor_simulation::ActorId,
    tor_simulation::ActorId,
) {
    use tor_simulation::content_ids::{AbilityTagId, ConditionId, TemplateId};
    use tor_simulation::rules::{
        Cost, Modifier, ModifierOp, Scope, StrikeDef, StrikeSource, Timing, TimingSource,
    };
    let base = super::abilities::content();
    let mut document = base.rules().document().clone();
    let mut observation = document
        .abilities
        .iter()
        .find(|ability| ability.id.as_str() == "fixture_focus_fear")
        .unwrap()
        .clone();
    observation.id = super::abilities::id("fixture_public_plan_read");
    observation.name = "Read the Rhythm".into();
    observation.timing = Timing {
        source: TimingSource::Fixed {
            prepare: 100,
            recover: 100,
        },
        scaled_by: None,
    };
    observation.cost = Some(Cost {
        resource: tor_simulation::resources::Resource::Focus,
        start: 1,
        resolution: 1,
    });
    observation.strikes = StrikeSource::Authored {
        strikes: vec![StrikeDef {
            offset: 0,
            unlock: None,
            check: None,
            on_success: vec![
                Effect::ObservePublicPlan,
                Effect::ApplyQualifiedConditionToSource {
                    condition: ConditionId::new("readied").unwrap(),
                    duration: 300,
                    ability_tag: AbilityTagId::new("weapon_attack").unwrap(),
                },
            ],
            on_failure: vec![],
            descriptors: vec![],
            riders: vec![],
        }],
    };
    let mut slow = observation.clone();
    slow.id = super::abilities::id("slow_plan");
    slow.name = "Slow Plan".into();
    slow.cost = None;
    slow.timing.source = TimingSource::Fixed {
        prepare: 300,
        recover: 100,
    };
    slow.strikes = StrikeSource::Authored {
        strikes: vec![StrikeDef {
            offset: 0,
            unlock: None,
            check: None,
            on_success: vec![Effect::Heal {
                amount: tor_simulation::rules::AmountExpr::Fixed { amount: 0 },
            }],
            on_failure: vec![],
            descriptors: vec![],
            riders: vec![],
        }],
    };
    document
        .constants
        .default_abilities
        .push(observation.id.clone());
    document.abilities.extend([observation, slow]);
    let rules = tor_simulation::rules::Ruleset::validate(document).unwrap();
    let mut catalog = base.catalog().clone();
    catalog
        .templates
        .get_mut(&TemplateId::new("techniques").unwrap())
        .unwrap()
        .modifiers
        .push(Modifier {
            scope: Scope::Always,
            op: ModifierOp::GrantAbility {
                ability: super::abilities::id("slow_plan"),
            },
        });
    let content = tor_simulation::creatures::Content::new(rules, catalog).unwrap();
    let mut game = tor_simulation::Game::two_room_in_stone(42, content.clone());
    let mut actors = vec![];
    for x in [1, 2] {
        let actor = game
            .spawn_actor(
                tor_world::Location {
                    region: tor_world::RegionId(1),
                    position: tor_world::Position { x, y: 1, z: 0 },
                },
                std::num::NonZeroU64::new(100).unwrap(),
            )
            .unwrap();
        game.configure_creature(
            actor,
            tor_simulation::CreatureIdentity {
                name: format!("subject{x}"),
                faction: "neutral".into(),
            },
            super::abilities::build(content.clone(), known || x == 2),
        )
        .unwrap();
        actors.push(actor);
    }
    (game, actors[0], actors[1])
}

fn observe_preparing(
    game: &mut tor_simulation::Game,
    observer: tor_simulation::ActorId,
    subject: tor_simulation::ActorId,
) {
    game.admit_intention(
        observer,
        Action::UseAbility {
            id: super::abilities::id("fixture_public_plan_read"),
            invocation: tor_simulation::abilities::AbilityInvocation::ActorTarget {
                target: subject,
            },
            supplies: Default::default(),
        },
        tor_simulation::IntentionOrigin::Human,
    )
    .unwrap();
    game.execute_next_intention().unwrap().outcome.unwrap();
    game.act(
        subject,
        Action::UseAbility {
            id: super::abilities::id("slow_plan"),
            invocation: tor_simulation::abilities::AbilityInvocation::ActorTarget {
                target: observer,
            },
            supplies: Default::default(),
        },
    )
    .unwrap();
}

#[test]
fn paid_public_plan_observation_freezes_known_intent_and_survives_occlusion_and_checkpoint() {
    use tor_simulation::public_plan::{ObservedAim, PublicPlanState};
    let (mut game, observer, subject) = observation_duel(true);
    observe_preparing(&mut game, observer, subject);
    let frozen = game
        .public_plan_observation(observer)
        .expect("paid effect records a result")
        .clone();
    assert_eq!(frozen.observed_at, 100);
    assert_eq!(frozen.subject_name, "subject2");
    let PublicPlanState::Preparing {
        ability,
        aim,
        remaining_work,
    } = &frozen.state
    else {
        panic!("preparing")
    };
    assert_eq!(*remaining_work, 200);
    assert_eq!(ability.as_ref().unwrap().id.as_str(), "slow_plan");
    assert!(
        matches!(aim, ObservedAim::Disclosed { occurrences, .. } if occurrences == &[tor_world::Position { x: 0, y: 0, z: 0 }])
    );
    assert!(game.public_plan_observation(subject).is_none());
    let before = game.clone();
    assert_eq!(game.public_plan_observation(observer), Some(&frozen));
    assert_eq!(game, before);
    game.pause_preparation(subject).unwrap();
    let mut away = game.observe(subject).unwrap().location;
    away.position.x += 2;
    game.teleport(subject, away).unwrap();
    assert_eq!(game.public_plan_observation(observer), Some(&frozen));
    let mut shared = tor_simulation::checkpoint::SharedState::default();
    let snapshot = game.checkpoint(&mut shared);
    let (snapshot, shared) =
        serde_json::from_slice(&serde_json::to_vec(&(snapshot, shared)).unwrap()).unwrap();
    let restored =
        tor_simulation::Game::restore_checkpoint(snapshot, &shared, game.content().clone())
            .unwrap();
    assert_eq!(restored.public_plan_observation(observer), Some(&frozen));
}

#[test]
fn unknown_public_ability_stays_redacted_after_later_recognition() {
    use tor_simulation::public_plan::PublicPlanState;
    let (mut game, observer, subject) = observation_duel(false);
    observe_preparing(&mut game, observer, subject);
    let frozen = game.public_plan_observation(observer).unwrap().clone();
    assert!(matches!(
        &frozen.state,
        PublicPlanState::Preparing { ability: None, .. }
    ));
    game.act(observer, Action::Wait).unwrap();
    let view = game.observe(observer).unwrap();
    assert!(view.combat.as_ref().unwrap().events.iter().any(|event| matches!(event,
        tor_simulation::combat::DisclosedCombatEvent::Ability { id, .. } if id.as_str() == "slow_plan")));
    game.record_observation_knowledge(&view).unwrap();
    assert_eq!(game.public_plan_observation(observer), Some(&frozen));
}

#[test]
fn outgoing_token_qualification_is_named_and_cannot_broaden_incoming_tokens() {
    use tor_simulation::conditions::{ConditionInstance, ConditionState};
    let build = super::creature_state::build();
    let value = json!({
        "condition":"readied", "source":1, "remaining":300, "stacks":1,
        "order":0, "reserved_by":null, "phase":"active",
        "qualification":{"opponent":2,"ability_tag":"weapon_attack"}
    });
    let entry: ConditionInstance = serde_json::from_slice(&serde_json::to_vec(&value).unwrap())
        .expect("an existing outgoing token can carry an explicit bounded qualification");
    assert!(ConditionState::from_recorded(vec![entry], build.ruleset()).is_ok());
    for condition in ["exposed", "fear"] {
        let mut invalid = value.clone();
        invalid["condition"] = json!(condition);
        let entry: ConditionInstance =
            serde_json::from_slice(&serde_json::to_vec(&invalid).unwrap()).unwrap();
        assert!(ConditionState::from_recorded(vec![entry], build.ruleset()).is_err());
    }
    for invalid in [
        json!({"opponent":0,"ability_tag":"weapon_attack"}),
        json!({"opponent":2,"ability_tag":"undiscovered_tag"}),
    ] {
        let mut malformed = value.clone();
        malformed["qualification"] = invalid;
        let entry: ConditionInstance =
            serde_json::from_slice(&serde_json::to_vec(&malformed).unwrap()).unwrap();
        assert!(ConditionState::from_recorded(vec![entry], build.ruleset()).is_err());
    }
    for qualifier in [
        r#"[2,"weapon_attack"]"#,
        r#"{"opponent":2,"ability_tag":"weapon_attack","private":1}"#,
        r#"{"private":1,"ability_tag":"weapon_attack","opponent":2}"#,
        r#"{"opponent":2,"opponent":3,"ability_tag":"weapon_attack"}"#,
    ] {
        let encoded = format!(
            r#"{{"condition":"readied","source":1,"remaining":300,"stacks":1,"order":0,"reserved_by":null,"phase":"active","qualification":{qualifier}}}"#
        );
        assert!(
            serde_json::from_slice::<ConditionInstance>(encoded.as_bytes()).is_err(),
            "{encoded}"
        );
    }
}

#[test]
fn public_plan_effect_is_a_strict_named_ordinary_effect() {
    let effect: Effect = serde_json::from_slice(br#"{"kind":"observe_public_plan"}"#)
        .expect("public-plan observation is an ordinary authored effect");
    assert_eq!(
        serde_json::to_value(&effect).unwrap(),
        json!({"kind":"observe_public_plan"})
    );
    assert!(super::creature_state::build()
        .ruleset()
        .validate_effects(&[effect])
        .is_ok());
    for malformed in [
        r#"{"kind":"observe_public_plan","private_actor":42}"#,
        r#"{"private_actor":42,"kind":"observe_public_plan"}"#,
        r#"{"kind":"observe_public_plan","kind":"observe_public_plan"}"#,
        r#"["observe_public_plan"]"#,
    ] {
        assert!(
            serde_json::from_slice::<Effect>(malformed.as_bytes()).is_err(),
            "{malformed}"
        );
    }
}

#[test]
fn ability_preparation_captures_a_visible_public_aim_once() {
    let (mut game, actor, target) = super::abilities::duel(42);
    let at = game.observe(target).unwrap().location;
    game.act(
        actor,
        Action::UseAbility {
            id: super::abilities::id("basic_melee"),
            invocation: tor_simulation::abilities::AbilityInvocation::ActorTarget { target },
            supplies: Default::default(),
        },
    )
    .unwrap();
    let frozen = game.preparation(actor).unwrap().plan.clone();
    let value = serde_json::to_value(frozen.as_ability().unwrap()).unwrap();
    assert_eq!(
        value.get("public_aim"),
        Some(&json!({"kind":"cell", "location": at}))
    );

    let mut moved = at;
    moved.position.x += 1;
    game.teleport(target, moved).unwrap();
    assert_ne!(game.observe(target).unwrap().location, at);
    assert_eq!(game.preparation(actor).unwrap().plan, frozen);
}

#[test]
fn captured_public_aim_has_a_required_strict_named_byte_encoding() {
    let (game, actor, target) = super::abilities::duel(42);
    let plan = game
        .ability_plan(actor, &super::abilities::id("basic_melee"), target)
        .unwrap();
    let mut value = serde_json::to_value(plan.as_ability().unwrap()).unwrap();
    value["public_aim"] = json!({"kind":"cell", "location":game.observe(target).unwrap().location});
    let encoded = serde_json::to_vec(&value).unwrap();
    let decoded: AbilityPlan = serde_json::from_slice(&encoded)
        .expect("current plan format includes a required captured public aim");
    assert_eq!(serde_json::to_value(decoded).unwrap(), value);
    let object = value.as_object_mut().unwrap();
    let aim = object.remove("public_aim").unwrap();
    assert!(serde_json::from_slice::<AbilityPlan>(&serde_json::to_vec(&value).unwrap()).is_err());
    for malformed in [
        Value::Null,
        json!(["cell", game.observe(target).unwrap().location]),
        json!({"kind":"cell"}),
        json!({"kind":"cell", "location":game.observe(target).unwrap().location, "target_actor":target}),
        json!({"kind":"unavailable", "target_actor":target}),
    ] {
        value["public_aim"] = malformed;
        let bytes = serde_json::to_vec(&value).unwrap();
        assert!(
            serde_json::from_slice::<AbilityPlan>(&bytes).is_err(),
            "{}",
            String::from_utf8(bytes).unwrap()
        );
        assert!(serde_json::from_value::<AbilityPlan>(value.clone()).is_err());
    }
    value["public_aim"] = aim;
    let prefix = serde_json::to_string(&value).unwrap();
    let duplicate = format!(
        "{},\"public_aim\":{{\"kind\":\"unavailable\"}}}}",
        &prefix[..prefix.len() - 1]
    );
    assert!(serde_json::from_slice::<AbilityPlan>(duplicate.as_bytes()).is_err());
}

#[test]
fn visible_neutral_opponent_uses_ordinary_sight_targeting() {
    let (mut game, observer, subject) = observation_duel(false);
    assert!(!game.hostile(observer, subject));
    assert!(!game.hostile(subject, observer));
    observe_preparing(&mut game, observer, subject);
    assert_eq!(
        game.public_plan_observation(observer).unwrap().subject_name,
        "subject2"
    );
    assert!(game.public_plan_observation(subject).is_none());
}
// Append to simulation integration public_plan_observation.
fn rejects_effect_bytes<T: serde::de::DeserializeOwned>(raw: &str) {
    assert!(
        serde_json::from_slice::<T>(raw.as_bytes()).is_err(),
        "accepted bytes: {raw}"
    );
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(raw) {
        assert!(
            serde_json::from_value::<T>(value).is_err(),
            "accepted Value: {raw}"
        );
    }
}
#[test]
fn public_plan_planned_effect_requires_named_payload_from_bytes() {
    use tor_simulation::abilities::PlannedEffect;
    let planned = r#"{"kind":"apply_qualified_condition_to_source","id":"readied","duration":300,"ability_tag":"weapon_attack"}"#;
    serde_json::from_slice::<PlannedEffect>(planned.as_bytes()).unwrap();
    for raw in [
        r#"{"kind":"apply_qualified_condition_to_source","id":"readied","duration":300,"ability_tag":"weapon_attack","extra":1}"#,
        r#"{"kind":"observe_public_plan","extra":1}"#,
        r#"{"extra":1,"kind":"observe_public_plan"}"#,
        r#"["apply_qualified_condition_to_source","readied",300,"weapon_attack"]"#,
    ] {
        rejects_effect_bytes::<PlannedEffect>(raw);
    }
}
#[test]
fn public_plan_resolved_effect_requires_named_payload_from_bytes() {
    use tor_simulation::abilities::ResolvedEffect;
    let resolved = r#"{"kind":"observe_public_plan","observation":{"observed_at":0,"subject_name":"foe","state":{"kind":"no_public_plan"}}}"#;
    serde_json::from_slice::<ResolvedEffect>(resolved.as_bytes()).unwrap();
    for raw in [
        r#"["observe_public_plan",{"observed_at":0,"subject_name":"foe","state":{"kind":"no_public_plan"}}]"#,
        r#"{"kind":"observe_public_plan","observation":{"observed_at":0,"subject_name":"foe","state":{"kind":"no_public_plan"}},"extra":1}"#,
        r#"{"observation":{"observed_at":0,"subject_name":"foe","state":{"kind":"no_public_plan"}},"extra":1,"kind":"observe_public_plan"}"#,
    ] {
        rejects_effect_bytes::<ResolvedEffect>(raw);
    }
}
#[test]
fn public_plan_outcome_wrapper_requires_named_bounded_fields_from_bytes() {
    use tor_simulation::abilities::Outcome;
    let resolved = r#"{"kind":"observe_public_plan","observation":{"observed_at":0,"subject_name":"foe","state":{"kind":"no_public_plan"}}}"#;
    let outcome = format!(
        r#"{{"actor":1,"target":2,"ability":null,"intention":null,"result":"applied","check":null,"target_rider_participated":false,"participation":{{"check":"none","payload":{{"kind":"source"}}}},"effects":[{resolved}]}}"#
    );
    serde_json::from_slice::<Outcome>(outcome.as_bytes()).unwrap();
    rejects_effect_bytes::<Outcome>(&format!(r#"[1,2,null,null,"applied",null,[{resolved}]]"#));
    for field in ["ability", "intention", "check"] {
        let omitted = outcome.replace(&format!(r#""{field}":null,"#), "");
        rejects_effect_bytes::<Outcome>(&omitted);
        let duplicate = outcome.replace(
            &format!(r#""{field}":null"#),
            &format!(r#""{field}":null,"{field}":null"#),
        );
        assert!(serde_json::from_slice::<Outcome>(duplicate.as_bytes()).is_err());
    }
    let mut too_many: serde_json::Value = serde_json::from_str(&outcome).unwrap();
    too_many["effects"] =
        serde_json::json!(vec![
            serde_json::from_str::<serde_json::Value>(resolved)
                .unwrap();
            129
        ]);
    assert!(serde_json::from_slice::<Outcome>(&serde_json::to_vec(&too_many).unwrap()).is_err());
}

fn qualified_source_document(
    lifecycle: tor_simulation::rules::ConditionLifecycle,
    source_scoped: bool,
) -> tor_simulation::rules::RulesetDocument {
    use tor_simulation::rules::{ConditionLifecycle, ModifierOp, Stat};
    let mut document = super::abilities::content().rules().document().clone();
    let mut condition = document
        .conditions
        .iter()
        .find(|value| value.id.as_str() == "readied")
        .unwrap()
        .clone();
    condition.id =
        tor_simulation::content_ids::ConditionId::new("fixture_qualified_token").unwrap();
    condition.family =
        tor_simulation::content_ids::ConditionFamilyId::new("fixture_qualified_token").unwrap();
    condition.lifecycle = lifecycle;
    condition.source_scoped = source_scoped;
    if lifecycle == ConditionLifecycle::NextCheckAgainst {
        condition.modifiers[0].op = ModifierOp::Add {
            stat: Stat::IncomingEdge { defense: None },
            value: 1,
        };
    }
    document.conditions.push(condition);
    document
}

fn qualified_source_effect() -> Effect {
    Effect::ApplyQualifiedConditionToSource {
        condition: tor_simulation::content_ids::ConditionId::new("fixture_qualified_token")
            .unwrap(),
        duration: 300,
        ability_tag: tor_simulation::content_ids::AbilityTagId::new("weapon_attack").unwrap(),
    }
}

#[test]
fn public_plan_review_source_effect_validation_rejects_ineligible_condition_matrix() {
    use tor_simulation::rules::{ConditionLifecycle, Ruleset};
    let mut mismatches = Vec::new();
    for lifecycle in [
        ConditionLifecycle::Timed,
        ConditionLifecycle::NextCheck,
        ConditionLifecycle::NextCheckAgainst,
    ] {
        for source_scoped in [false, true] {
            let document = qualified_source_document(lifecycle, source_scoped);
            let definition_valid = lifecycle == ConditionLifecycle::Timed || !source_scoped;
            let rules = Ruleset::validate(document);
            assert_eq!(
                rules.is_ok(),
                definition_valid,
                "definition control {lifecycle:?}/{source_scoped}"
            );
            let Some(rules) = rules.ok() else {
                continue;
            };
            assert!(
                rules
                    .validate_effects(&[Effect::ApplyCondition {
                        condition: tor_simulation::content_ids::ConditionId::new(
                            "fixture_qualified_token"
                        )
                        .unwrap(),
                        duration: 300,
                    }])
                    .is_ok(),
                "ordinary condition application remains valid"
            );
            let actual = rules.validate_effects(&[qualified_source_effect()]).is_ok();
            let expected = lifecycle == ConditionLifecycle::NextCheck && !source_scoped;
            if actual != expected {
                mismatches.push(format!("{lifecycle:?}/source_scoped={source_scoped}: accepted={actual}, expected={expected}"));
            }
        }
    }
    assert!(
        mismatches.is_empty(),
        "qualified source acceptance mismatch: {mismatches:?}"
    );
}

#[test]
fn public_plan_review_ability_source_validation_rejects_ineligible_payload_matrix() {
    use tor_simulation::rules::{
        ConditionLifecycle, RiderDef, RiderTrigger, Ruleset, StrikeSource,
    };
    let mut mismatches = Vec::new();
    for lifecycle in [
        ConditionLifecycle::Timed,
        ConditionLifecycle::NextCheck,
        ConditionLifecycle::NextCheckAgainst,
    ] {
        for source_scoped in [false, true] {
            for branch in ["success", "failure", "rider"] {
                let mut document = qualified_source_document(lifecycle, source_scoped);
                let ability = document
                    .abilities
                    .iter_mut()
                    .find(|value| value.id.as_str() == "fixture_focus_fear")
                    .unwrap();
                let StrikeSource::Authored { strikes } = &mut ability.strikes else {
                    panic!("authored fear fixture")
                };
                let strike = &mut strikes[0];
                strike.on_success.clear();
                strike.on_failure.clear();
                strike.riders.clear();
                match branch {
                    "success" => strike.on_success.push(qualified_source_effect()),
                    "failure" => strike.on_failure.push(qualified_source_effect()),
                    "rider" => strike.riders.push(RiderDef {
                        condition: None,
                        trigger: RiderTrigger::Success,
                        check: None,
                        effects: vec![qualified_source_effect()],
                        descriptors: vec![],
                    }),
                    _ => unreachable!(),
                }
                let actual = Ruleset::validate(document).is_ok();
                let expected = lifecycle == ConditionLifecycle::NextCheck && !source_scoped;
                if actual != expected {
                    mismatches.push(format!("{lifecycle:?}/source_scoped={source_scoped}/{branch}: accepted={actual}, expected={expected}"));
                }
            }
        }
    }
    assert!(
        mismatches.is_empty(),
        "ability authoring must reject before admission: {mismatches:?}"
    );
}

#[test]
fn public_plan_review_accepted_outgoing_source_compiles_and_executes() {
    let (mut game, observer, subject) = observation_duel(false);
    assert!(game
        .ability_plan(
            observer,
            &super::abilities::id("fixture_public_plan_read"),
            subject
        )
        .is_ok());
    observe_preparing(&mut game, observer, subject);
    assert!(game.public_plan_observation(observer).is_some());
    assert!(game
        .creature(observer)
        .unwrap()
        .conditions()
        .instances()
        .any(|entry| {
            entry.condition.as_str() == "readied"
                && entry
                    .qualification
                    .as_ref()
                    .is_some_and(|scope| scope.opponent == subject)
        }));
}
fn qualified_named_cases() -> Vec<(&'static str, Effect, bool)> {
    use tor_simulation::content_ids::{AbilityTagId, ConditionId};
    let mut cases = Vec::new();
    for condition in [
        "readied",
        "distracted",
        "braced",
        "fear",
        "exposed",
        "missing_condition",
    ] {
        cases.push((
            condition,
            Effect::ApplyQualifiedConditionToSource {
                condition: ConditionId::new(condition).unwrap(),
                duration: 300,
                ability_tag: AbilityTagId::new("weapon_attack").unwrap(),
            },
            matches!(condition, "readied" | "distracted"),
        ));
    }
    for (label, tag, duration) in [
        ("unknown_tag", "missing_tag", 300),
        ("zero_duration", "weapon_attack", 0),
        ("overflow_duration", "weapon_attack", u64::MAX),
    ] {
        cases.push((
            label,
            Effect::ApplyQualifiedConditionToSource {
                condition: ConditionId::new("readied").unwrap(),
                duration,
                ability_tag: AbilityTagId::new(tag).unwrap(),
            },
            false,
        ));
    }
    cases
}
#[test]
fn public_plan_review_named_condition_effect_matrix_uses_one_source_contract() {
    let content = super::abilities::content();
    let mut mismatches = Vec::new();
    for (label, effect, expected) in qualified_named_cases() {
        let actual = content.rules().validate_effects(&[effect]).is_ok();
        if expected {
            assert!(actual, "eligible named effect control {label}");
        }
        if actual != expected {
            mismatches.push((label, actual, expected));
        }
    }
    assert!(
        mismatches.is_empty(),
        "named condition eligibility mismatch: {mismatches:?}"
    );
}
#[test]
fn public_plan_review_natural_source_matrix_rejects_ineligible_qualified_effects() {
    use tor_simulation::{
        attacks::NaturalAttack, attributes::Skill, creatures::Content, rules::StrikeDef,
    };
    let base = super::abilities::content();
    let mut mismatches = Vec::new();
    for (label, effect, expected) in qualified_named_cases() {
        let strike = StrikeDef {
            offset: 0,
            unlock: None,
            check: None,
            on_success: vec![effect],
            on_failure: vec![],
            descriptors: vec![],
            riders: vec![],
        };
        let natural = NaturalAttack::new(Skill::HeavyWeaponry, 0, 100, 100, vec![strike]);
        let actual = natural.ok().is_some_and(|natural| {
            let mut catalog = base.catalog().clone();
            catalog.species.values_mut().next().unwrap().melee = natural;
            Content::new(base.rules().clone(), catalog).is_ok()
        });
        if expected {
            assert!(actual, "eligible natural source control {label}");
        }
        if actual != expected {
            mismatches.push((label, actual, expected));
        }
    }
    assert!(
        mismatches.is_empty(),
        "natural source eligibility mismatch: {mismatches:?}"
    );
}
#[test]
fn public_plan_review_item_source_matrix_rejects_ineligible_qualified_effects() {
    let (base, actor, _) = observation_duel(false);
    let mut mismatches = Vec::new();
    for (label, effect, expected) in qualified_named_cases() {
        let mut game = base.clone();
        let mut item = tor_simulation::ItemSpec::ordinary("qualification vial".into());
        item.class = tor_simulation::ItemClass::Potion;
        item.consumable = Some(tor_simulation::ConsumableSpec {
            effects: vec![effect],
        });
        let actual = game
            .place_item_stack(
                999,
                tor_world::Location {
                    region: tor_world::RegionId(1),
                    position: tor_world::Position { x: 1, y: 1, z: 0 },
                },
                Some(actor),
                1,
                item,
            )
            .is_ok();
        if expected {
            assert!(actual, "eligible item source control {label}");
        }
        if actual != expected {
            mismatches.push((label, actual, expected));
        }
    }
    assert!(
        mismatches.is_empty(),
        "item source eligibility mismatch: {mismatches:?}"
    );
}

#[test]
fn public_plan_review_frozen_check_has_required_source_validated_participation() {
    let (game, actor, target) = super::abilities::duel(42);
    let plan = game
        .ability_plan(actor, &super::abilities::id("fixture_mana_ray"), target)
        .unwrap();
    let value = serde_json::to_value(plan.as_ability().unwrap()).unwrap();
    assert_eq!(
        value["strikes"][0]["check"]["target_participates"],
        json!(true),
        "frozen target participation must be explicit"
    );
    let canonical: AbilityPlan =
        serde_json::from_slice(&serde_json::to_vec(&value).unwrap()).unwrap();
    assert!(canonical.valid(game.content().rules()));
    let mut forged = value.clone();
    forged["strikes"][0]["check"]["target_participates"] = json!(false);
    let forged: AbilityPlan =
        serde_json::from_slice(&serde_json::to_vec(&forged).unwrap()).unwrap();
    assert!(
        !forged.valid(game.content().rules()),
        "source validation rejects a forged involvement bit"
    );
    let mut missing = value;
    missing["strikes"][0]["check"]
        .as_object_mut()
        .unwrap()
        .remove("target_participates");
    assert!(serde_json::from_slice::<AbilityPlan>(&serde_json::to_vec(&missing).unwrap()).is_err());
}
#[test]
fn public_plan_review_outcome_has_required_consistent_participation_evidence() {
    use tor_simulation::abilities::{resolve, Outcome, Snapshot};
    let (game, actor, target) = super::abilities::duel(42);
    let plan = game
        .ability_plan(actor, &super::abilities::id("basic_melee"), target)
        .unwrap();
    let mut rng = 42;
    let outcome = resolve(
        plan.as_ability().unwrap(),
        &Snapshot::capture(&game.creature(target).unwrap()),
        &mut rng,
        &mut (),
    )
    .unwrap();
    let value = serde_json::to_value(outcome).unwrap();
    assert!(
        value.get("participation").is_some(),
        "outcome requires bounded attempted participation evidence"
    );
    assert!(serde_json::from_slice::<Outcome>(&serde_json::to_vec(&value).unwrap()).is_ok());
    let mut missing = value.clone();
    missing.as_object_mut().unwrap().remove("participation");
    assert!(serde_json::from_slice::<Outcome>(&serde_json::to_vec(&missing).unwrap()).is_err());
    let mut contradictory = value;
    contradictory["participation"]["check"] = json!("none");
    assert!(
        serde_json::from_slice::<Outcome>(&serde_json::to_vec(&contradictory).unwrap()).is_err(),
        "retained check cannot have absent attempted check evidence"
    );
}

#[test]
fn public_plan_review_forged_pending_participation_fails_authored_and_natural_restore() {
    use tor_simulation::checkpoint::{SharedState, Snapshot};
    for ability in ["fixture_mana_ray", "basic_melee"] {
        let (mut game, actor, target) = super::abilities::duel(42);
        game.admit_intention(
            actor,
            Action::UseAbility {
                id: super::abilities::id(ability),
                invocation: tor_simulation::abilities::AbilityInvocation::ActorTarget { target },
                supplies: Default::default(),
            },
            tor_simulation::IntentionOrigin::Human,
        )
        .unwrap();
        game.execute_next_intention().unwrap().outcome.unwrap();
        assert!(
            game.preparation(actor).is_some(),
            "a real pending {ability} plan"
        );
        let mut shared = SharedState::default();
        let checkpoint = game.checkpoint(&mut shared);
        let canonical = serde_json::to_value((checkpoint, shared)).unwrap();
        let (checkpoint, shared): (Snapshot, SharedState) =
            serde_json::from_value(canonical.clone()).unwrap();
        assert_eq!(
            tor_simulation::Game::restore_checkpoint(checkpoint, &shared, game.content().clone())
                .unwrap(),
            game
        );
        let mut forged = canonical;
        let table = forged[0]["actors"].as_u64().unwrap() as usize;
        let check = &mut forged[1]["actors"][table][actor.0.to_string()]["pending"]["plan"]
            ["strikes"][0]["check"];
        assert_eq!(check["actor"], json!("user"));
        assert_eq!(check["threshold"]["kind"], json!("fixed"));
        assert_eq!(check["target_participates"], json!(true));
        check["target_participates"] = json!(false);
        let (checkpoint, shared): (Snapshot, SharedState) =
            serde_json::from_slice(&serde_json::to_vec(&forged).unwrap()).unwrap();
        assert!(
            tor_simulation::Game::restore_checkpoint(checkpoint, &shared, game.content().clone())
                .is_err(),
            "{ability} source rejects the forged personal-DC claim"
        );
    }
}
