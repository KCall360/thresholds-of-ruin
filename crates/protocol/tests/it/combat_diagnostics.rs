use super::creature::content;
use tor_protocol::*;
fn trace() -> CombatTraceView {
    CombatTraceView {
        truncated: false,
        steps: vec![CombatTraceStep {
            id: "fixture.step".into(),
            name: "A data-defined step".into(),
            fields: vec![
                DiagnosticField {
                    id: "rng".into(),
                    name: "Random state".into(),
                    value: DiagnosticValue::Unsigned { value: u64::MAX },
                },
                DiagnosticField {
                    id: "edge".into(),
                    name: "Edge".into(),
                    value: DiagnosticValue::Signed { value: i64::MIN },
                },
                DiagnosticField {
                    id: "content".into(),
                    name: "Content".into(),
                    value: DiagnosticValue::Content {
                        value: content("fixture_condition"),
                    },
                },
            ],
        }],
    }
}
fn combatant() -> DiagnosticCombatant {
    DiagnosticCombatant {
        health: 10,
        maximum_health: 10,
        injury: Some(0),
        dead: false,
        resources: vec![],
        conditions: vec![],
    }
}
fn report() -> CombatDiagnosticsView {
    CombatDiagnosticsView {
        enabled: true,
        tick: 10,
        captured: 1,
        dropped: 0,
        retained: 1,
        through: 1,
        records: vec![CombatDiagnosticRecord {
            sequence: 1,
            tick: 10,
            actor: ActorId(1),
            target: ActorId(2),
            ability: Some(content("fixture_ability")),
            intention: Some(1),
            origin_intention: Some(1),
            preparation: 5,
            started: 5,
            remaining: 5,
            recovery: 2,
            charge: None,
            actor_before: combatant(),
            target_before: combatant(),
            actor_after: combatant(),
            target_after: combatant(),
            outcome: DiagnosticOutcome {
                result: AbilityOutcome::Unaffected,
                effects: vec![],
            },
            trace: trace(),
        }],
    }
}
#[test]
fn generic_trace_roundtrips_unknown_steps_content_and_wide_numbers() {
    let trace = trace();
    trace.validate().unwrap();
    let value = serde_json::to_value(&trace).unwrap();
    assert_eq!(
        value["steps"][0]["fields"][0]["value"]["value"],
        u64::MAX.to_string()
    );
    assert_eq!(
        value["steps"][0]["fields"][1]["value"]["value"],
        i64::MIN.to_string()
    );
    assert_eq!(
        serde_json::from_value::<CombatTraceView>(value).unwrap(),
        trace
    );
}
#[test]
fn generic_trace_rejects_unbounded_fields_duplicate_keys_and_unsafe_numbers() {
    for case in 0..5 {
        let mut value = trace();
        match case {
            0 => value.steps[0].id = "bad id".into(),
            1 => {
                let duplicate = value.steps[0].fields[0].clone();
                value.steps[0].fields.push(duplicate)
            }
            2 => {
                value.steps[0].fields[0].value = DiagnosticValue::Text {
                    value: "x".repeat(4097),
                }
            }
            3 => {
                value.steps[0].fields[0].value = DiagnosticValue::Dice {
                    values: vec![6; 129],
                }
            }
            4 => value.steps = vec![value.steps[0].clone(); 129],
            _ => unreachable!(),
        }
        assert!(value.validate().is_err(), "case {case}");
    }
    let mut value = serde_json::to_value(trace()).unwrap();
    value["steps"][0]["fields"][0]["value"]["value"] = u64::MAX.into();
    assert!(serde_json::from_value::<CombatTraceView>(value).is_err());
}
#[test]
fn report_bounds_pagination_and_outcomes_are_validated_independently_of_trace_completeness() {
    report().validate().unwrap();
    for case in 0..8 {
        let mut value = report();
        match case {
            0 => value.records[0].sequence = 2,
            1 => value.through = 2,
            2 => value.retained = 65,
            3 => value.captured = 2,
            4 => value.records[0].tick = 11,
            5 => value.records[0].remaining = 6,
            6 => value.records[0].actor_before.health = 11,
            7 => {
                value.records[0].outcome.effects = vec![DiagnosticEffect::Damage {
                    target: ActorId(0),
                    amount: 1,
                }]
            }
            _ => unreachable!(),
        }
        assert!(value.validate().is_err(), "case {case}");
    }
    let mut value = report();
    value.records[0].trace.truncated = true;
    value.records[0].trace.steps.clear();
    value.validate().unwrap();
    value.records[0].ability = None;
    value.validate().unwrap();
    let encoded = serde_json::to_value(&value).unwrap();
    assert_eq!(
        serde_json::from_value::<CombatDiagnosticsView>(encoded).unwrap(),
        value
    );
}
#[test]
fn public_plan_diagnostic_observation_recipient_is_the_recorded_source_actor() {
    let mut value = report();
    value.records[0].outcome.effects = vec![DiagnosticEffect::Observation {
        target: value.records[0].actor,
    }];
    value
        .validate()
        .expect("the source owns its paid observation");
    value.records[0].outcome.effects = vec![DiagnosticEffect::Observation {
        target: value.records[0].target,
    }];
    assert!(
        value.validate().is_err(),
        "selected target is not the observation recipient"
    );
    value.records[0].target = value.records[0].actor;
    value.records[0].outcome.effects = vec![DiagnosticEffect::Observation {
        target: value.records[0].actor,
    }];
    value
        .validate()
        .expect("same-actor aliases retain one canonical recipient");
}

// Append to protocol/tests/it/combat_diagnostics.rs after canonical reader merge.
#[test]
fn translation_diagnostics_are_closed_named_and_target_qualified_without_locations() {
    for mode in ["voluntary", "forced"] {
        for result in ["moved", "blocked", "ineligible", "stale", "ambiguous"] {
            let value =
                serde_json::json!({"type":"translate","target":"2","mode":mode,"result":result});
            let effect: DiagnosticEffect =
                serde_json::from_slice(&serde_json::to_vec(&value).unwrap()).unwrap();
            assert_eq!(serde_json::to_value(&effect).unwrap(), value);
            let mut view = report();
            view.records[0].outcome.effects = vec![effect.clone()];
            assert!(view.validate().is_ok());
            view.records[0].target = ActorId(3);
            assert!(view.validate().is_err());
            view.records[0].actor = ActorId(2);
            view.records[0].target = ActorId(2);
            assert!(view.validate().is_ok(), "self roles canonicalize");
        }
    }
    for raw in [
        r#"["translate",{"target":"2","mode":"forced","result":"moved"}]"#,
        r#"{"type":"translate","target":"2","mode":"forced"}"#,
        r#"{"type":"translate","target":"2","mode":"forced","result":"unknown"}"#,
        r#"{"type":"translate","target":"2","mode":"forced","result":"moved","from":{"region":1}}"#,
        r#"{"type":"translate","target":"2","mode":"forced","result":"moved","target":"3"}"#,
        r#"{"type":"translate","target":"2","mode":"forced","mode":"voluntary","result":"moved"}"#,
    ] {
        assert!(
            serde_json::from_slice::<DiagnosticEffect>(raw.as_bytes()).is_err(),
            "{raw}"
        );
    }
    let reverse = r#"{"result":"blocked","mode":"forced","target":"2","type":"translate"}"#;
    assert!(serde_json::from_slice::<DiagnosticEffect>(reverse.as_bytes()).is_ok());
}

#[test]
fn restorable_loss_diagnostics_are_closed_bounded_and_target_qualified() {
    for kind in [
        "lose_attribute",
        "restore_attribute",
        "lose_maximum_health",
        "restore_maximum_health",
    ] {
        let mut value = serde_json::json!({"type":kind,"target":"2","amount":2});
        if kind.ends_with("attribute") {
            value["attribute"] = serde_json::json!({"id":"strength","name":"Strength"});
        }
        let effect: DiagnosticEffect =
            serde_json::from_value(value.clone()).expect("typed loss result");
        assert_eq!(serde_json::to_value(&effect).unwrap(), value);
        let mut view = report();
        view.records[0].outcome.effects = vec![effect];
        view.validate().unwrap();
        view.records[0].target = ActorId(3);
        assert!(
            view.validate().is_err(),
            "loss recipient must match the resolved target"
        );
        for field in ["amount", "target"] {
            let mut missing = value.clone();
            missing.as_object_mut().unwrap().remove(field);
            assert!(serde_json::from_value::<DiagnosticEffect>(missing).is_err());
        }
        let mut leaked = value.clone();
        leaked["causer_seed"] = 42.into();
        assert!(serde_json::from_value::<DiagnosticEffect>(leaked).is_err());
        let maximum = if kind.ends_with("attribute") {
            1000
        } else {
            1_000_000
        };
        view.records[0].target = ActorId(2);
        for amount in [0, maximum] {
            value["amount"] = amount.into();
            view.records[0].outcome.effects = vec![serde_json::from_value(value.clone()).unwrap()];
            assert!(
                view.validate().is_ok(),
                "{kind} accepts actual applied amount {amount}"
            );
        }
        value["amount"] = (maximum + 1).into();
        view.records[0].outcome.effects = vec![serde_json::from_value(value).unwrap()];
        assert!(
            view.validate().is_err(),
            "{kind} rejects an impossible actual applied amount"
        );
    }
}

#[test]
fn hit_die_effect_diagnostics_are_closed_single_ordinal_and_target_qualified() {
    for kind in ["drain_latest_hit_die", "restore_hit_die"] {
        for amount in [0, 1] {
            let value = serde_json::json!({"type":kind,"target":"2","amount":amount});
            let effect: DiagnosticEffect = serde_json::from_value(value.clone()).unwrap();
            assert_eq!(serde_json::to_value(&effect).unwrap(), value);
            let mut view = report();
            view.records[0].outcome.effects = vec![effect];
            view.validate().unwrap();
            view.records[0].target = ActorId(3);
            assert!(view.validate().is_err());
            for field in ["amount", "target"] {
                let mut invalid = value.clone();
                invalid.as_object_mut().unwrap().remove(field);
                assert!(serde_json::from_value::<DiagnosticEffect>(invalid).is_err());
            }
            let mut invalid = value;
            invalid["health_seed"] = 42.into();
            assert!(serde_json::from_value::<DiagnosticEffect>(invalid).is_err());
        }
        let mut view = report();
        view.records[0].outcome.effects =
            vec![
                serde_json::from_value(serde_json::json!({"type":kind,"target":"2","amount":2}))
                    .unwrap(),
            ];
        assert!(
            view.validate().is_err(),
            "one effect cannot change two ordinals"
        );
        let duplicate = format!(r#"{{"type":"{kind}","target":"2","amount":1,"amount":0}}"#);
        assert!(serde_json::from_str::<DiagnosticEffect>(&duplicate).is_err());
    }
}

#[test]
fn restorable_loss_diagnostics_reject_positional_duplicate_and_missing_attribute_operands() {
    for raw in [
        r#"["lose_maximum_health", "2", 1]"#,
        r#"{"type":"lose_maximum_health","target":"2","amount":1,"amount":2}"#,
        r#"{"type":"restore_attribute","target":"2","amount":1}"#,
        r#"{"type":"lose_attribute","target":"2","attribute":["strength","Strength"],"amount":1}"#,
        r#"{"type":"restore_attribute","target":"2","attribute":null,"amount":1}"#,
    ] {
        assert!(
            serde_json::from_str::<DiagnosticEffect>(raw).is_err(),
            "{raw}"
        );
    }
    let mut view = report();
    view.records[0].outcome.effects = vec![DiagnosticEffect::LoseAttribute {
        target: ActorId(2),
        attribute: content("health"),
        amount: 1,
    }];
    assert!(view.validate().is_err());
}

#[test]
fn corrosion_diagnostics_are_closed_bounded_and_roundtrip_actual_loss() {
    for amount in [0, 1, 1_000_000] {
        let value = serde_json::json!({"type":"corrode_armor","target":"2","amount":amount});
        let effect: DiagnosticEffect = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(serde_json::to_value(&effect).unwrap(), value);
        let mut view = report();
        view.records[0].outcome.effects = vec![effect];
        view.validate().unwrap();
        let encoded = serde_json::to_string(&view).unwrap();
        assert_eq!(
            serde_json::from_str::<CombatDiagnosticsView>(&encoded).unwrap(),
            view
        );
        for field in ["target", "amount"] {
            let mut missing = value.clone();
            missing.as_object_mut().unwrap().remove(field);
            assert!(serde_json::from_value::<DiagnosticEffect>(missing).is_err());
        }
        let mut leaked = value;
        leaked["item"] = "17".into();
        assert!(serde_json::from_value::<DiagnosticEffect>(leaked).is_err());
    }
    for (target, amount) in [(0, 1), (2, 1_000_001), (2, u32::MAX)] {
        let mut view = report();
        view.records[0].outcome.effects = vec![DiagnosticEffect::CorrodeArmor {
            target: ActorId(target),
            amount,
        }];
        assert!(view.validate().is_err());
    }
    for raw in [
        r#"["corrode_armor","2",1]"#,
        r#"{"type":"corrode_armor","target":"2","amount":1,"amount":2}"#,
        r#"{"type":"corrode_armor","target":"2","target":"3","amount":1}"#,
        r#"{"type":"corrode_armor","type":"damage","target":"2","amount":1}"#,
        r#"{"type":"corrode_armor","target":"2","amount":-1}"#,
        r#"{"type":"corrode_armor","target":"2","amount":4294967296}"#,
    ] {
        assert!(
            serde_json::from_str::<DiagnosticEffect>(raw).is_err(),
            "{raw}"
        );
    }
}

#[test]
fn condition_removal_is_named_closed_and_bound_to_report_target() {
    let effect = serde_json::json!({"type":"condition_removed","target":"2","condition":{"id":"haste","name":"Haste"}});
    let decoded: DiagnosticEffect = serde_json::from_value(effect.clone()).unwrap();
    assert_eq!(serde_json::to_value(&decoded).unwrap(), effect);
    for bad in [
        serde_json::json!(["condition_removed", "2", {"id":"haste","name":"Haste"}]),
        serde_json::json!({"type":"condition_removed","target":"2","condition":["haste","Haste"]}),
        serde_json::json!({"type":"condition_removed","target":"2","condition":{"id":"haste","name":"Haste"},"duration":"1"}),
    ] {
        assert!(serde_json::from_value::<DiagnosticEffect>(bad).is_err());
    }
    let mut value = report();
    value.records[0].outcome.effects.push(decoded);
    assert!(value.validate().is_ok());
    if let DiagnosticEffect::ConditionRemoved { target, .. } =
        &mut value.records[0].outcome.effects[0]
    {
        *target = ActorId(3);
    }
    assert!(value.validate().is_err());
}

#[test]
fn healing_improvement_diagnostic_discloses_no_owned_seed_or_selection_claim() {
    let effect = serde_json::json!({"type":"heal_and_improve_hit_die","target":"2","amount":8});
    let decoded: DiagnosticEffect = serde_json::from_value(effect.clone()).unwrap();
    assert_eq!(serde_json::to_value(&decoded).unwrap(), effect);
    for field in [
        "source",
        "seed",
        "health_seed",
        "ordinal",
        "improvement",
        "origin",
    ] {
        let mut bad = effect.clone();
        bad[field] = serde_json::json!(1);
        assert!(
            serde_json::from_value::<DiagnosticEffect>(bad).is_err(),
            "{field}"
        );
    }
    let mut value = report();
    value.records[0].outcome.effects = vec![decoded];
    assert!(value.validate().is_ok());
    value.records[0].outcome.effects = vec![DiagnosticEffect::HealAndImproveHitDie {
        target: ActorId(0),
        amount: 8,
    }];
    assert!(value.validate().is_err());
}
