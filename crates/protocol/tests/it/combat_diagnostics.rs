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
