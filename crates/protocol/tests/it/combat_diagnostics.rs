use serde_json::{json, Value};
use tor_protocol::CombatTraceView;

fn attack() -> Value {
    json!({"truncated":false,"steps":[
        {"type":"attack_started","net_edge":"3"},
        {"type":"check","check":{"skill":"heavy_weaponry","binding":"intellect","attribute":"strength","attribute_value":2,"rank":1,"modifier":-1,"threshold":10,"input_edge":"1","edge":"1","unused_edge":"0","rng_before":"18446744073709551615","rng_after":"43","first":10,"second":20,"kept":20,"total":"22","success":true}},
        {"type":"damage_started","net_edge":"2"},
        {"type":"component","component":{"category":"impact","descriptor":null,"expression":{"count":2,"sides":6,"bonus":1},"edge":"2","rng_before":"43","rng_after":"44","rolled":[1,6,4,2],"kept":[6,4],"raw":11,"category_immune":false,"descriptor_immune":false,"after_immunity":11}},
        {"type":"reduction","selector":{"type":"category","category":"impact"},"category":"impact","before":11,"capacity":3,"after":8},
        {"type":"damage_finished","raw":11,"after_immunity":11,"after_descriptors":11,"total":8,"unused_edge":"0"}
    ]})
}
fn parsed(value: Value) -> CombatTraceView {
    serde_json::from_value(value).unwrap()
}

#[test]
fn numerical_trace_round_trips_wide_rng_and_signed_edge_as_decimal_strings() {
    let trace = parsed(attack());
    assert!(trace.validate().is_ok());
    let encoded = serde_json::to_value(&trace).unwrap();
    assert_eq!(
        encoded["steps"][1]["check"]["rng_before"],
        "18446744073709551615"
    );
    assert_eq!(parsed(encoded), trace);
}

#[test]
fn numerical_trace_rejects_wrong_kept_dice_operands_edges_rng_chain_and_protection() {
    for (path, wrong) in [
        ("/steps/1/check/kept", json!(10)),
        ("/steps/1/check/total", json!("23")),
        ("/steps/1/check/attribute", json!("intellect")),
        ("/steps/1/check/unused_edge", json!("1")),
        ("/steps/2/net_edge", json!("3")),
        ("/steps/3/component/kept", json!([6, 6])),
        ("/steps/3/component/raw", json!(12)),
        ("/steps/3/component/rng_before", json!("99")),
        ("/steps/3/component/after_immunity", json!(9)),
        ("/steps/4/after", json!(7)),
        ("/steps/5/total", json!(9)),
        ("/steps/5/unused_edge", json!("1")),
    ] {
        let mut value = attack();
        *value.pointer_mut(path).unwrap() = wrong;
        assert!(parsed(value).validate().is_err(), "accepted invalid {path}");
    }
    let mut value = attack();
    value["steps"].as_array_mut().unwrap().swap(3, 4);
    assert!(parsed(value).validate().is_err());
    let mut value = attack();
    value["steps"].as_array_mut().unwrap().pop();
    assert!(parsed(value).validate().is_err());
    let mut value = attack();
    value["truncated"] = json!(true);
    assert!(
        parsed(value).validate().is_err(),
        "a short trace cannot claim storage truncation"
    );
}

#[test]
fn fear_trace_requires_immunity_or_one_defender_resistance_then_matching_result() {
    let immune = json!({"truncated":false,"steps":[
        {"type":"fear_started","difficulty":16,"duration":"400","net_edge":"-3","difficulty_attribute":2,"difficulty_rank":1,"difficulty_bonus":3,"duration_bonus":"100"},
        {"type":"fear_immunity","fear":false,"mind_affecting":true},
        {"type":"fear_finished","applied":false,"duration":"0"}
    ]});
    assert!(parsed(immune.clone()).validate().is_ok());
    let mut value = immune.clone();
    value["steps"][1]["mind_affecting"] = json!(false);
    assert!(parsed(value).validate().is_err());
    let mut value = immune;
    value["steps"][2]["applied"] = json!(true);
    assert!(parsed(value).validate().is_err());
    let mut resistance = attack()["steps"][1].clone();
    resistance["check"]["skill"] = json!("discipline");
    resistance["check"]["attribute"] = json!("willpower");
    resistance["check"]["input_edge"] = json!("-3");
    resistance["check"]["edge"] = json!("-1");
    resistance["check"]["unused_edge"] = json!("-2");
    resistance["check"]["first"] = json!(2);
    resistance["check"]["second"] = json!(10);
    resistance["check"]["kept"] = json!(2);
    resistance["check"]["total"] = json!("4");
    resistance["check"]["threshold"] = json!(16);
    resistance["check"]["success"] = json!(false);
    let value = json!({"truncated":false,"steps":[
        {"type":"fear_started","difficulty":16,"duration":"400","net_edge":"-3","difficulty_attribute":2,"difficulty_rank":1,"difficulty_bonus":3,"duration_bonus":"100"}, resistance,
        {"type":"fear_finished","applied":true,"duration":"400"}
    ]});
    assert!(parsed(value.clone()).validate().is_ok());
    let mut wrong = value;
    wrong["steps"][2]["duration"] = json!("399");
    assert!(parsed(wrong).validate().is_err());
}

#[test]
fn trace_decoder_rejects_unknown_fields_and_noncanonical_wide_integers() {
    let mut value = attack();
    value["steps"][0]["net_edge"] = json!(3);
    assert!(serde_json::from_value::<CombatTraceView>(value).is_err());
    let mut value = attack();
    value["steps"][0]["net_edge"] = json!("+3");
    assert!(serde_json::from_value::<CombatTraceView>(value).is_err());
    let mut value = attack();
    value["steps"][1]["check"]["extra"] = json!(false);
    assert!(serde_json::from_value::<CombatTraceView>(value).is_err());
}

#[test]
fn trace_misses_leave_damage_budget_unused_and_immunity_keeps_fixed_damage_rng_unchanged() {
    let mut miss = attack();
    miss["steps"][1]["check"]["threshold"] = json!(100);
    miss["steps"][1]["check"]["success"] = json!(false);
    miss["steps"].as_array_mut().unwrap().truncate(2);
    miss["steps"]
        .as_array_mut()
        .unwrap()
        .push(json!({"type":"attack_missed","unused_edge":"2"}));
    assert!(parsed(miss.clone()).validate().is_ok());
    miss["steps"][2]["unused_edge"] = json!("1");
    assert!(parsed(miss).validate().is_err());

    let mut immune = attack();
    immune["steps"][3]["component"] = json!({"category":"impact","descriptor":"fire","expression":null,"edge":"0","rng_before":"43","rng_after":"43","rolled":[],"kept":[],"raw":7,"category_immune":false,"descriptor_immune":true,"after_immunity":0});
    immune["steps"][4] = json!({"type":"reduction","selector":{"type":"descriptor","descriptor":"fire"},"category":"impact","before":0,"capacity":3,"after":0});
    immune["steps"][5] = json!({"type":"reduction","selector":{"type":"category","category":"impact"},"category":"impact","before":0,"capacity":4,"after":0});
    immune["steps"].as_array_mut().unwrap().push(json!({"type":"damage_finished","raw":7,"after_immunity":0,"after_descriptors":0,"total":0,"unused_edge":"2"}));
    assert!(parsed(immune.clone()).validate().is_ok());
    let mut wrong = immune.clone();
    wrong["steps"][3]["component"]["rng_after"] = json!("44");
    assert!(parsed(wrong).validate().is_err());
    let mut wrong = immune.clone();
    wrong["steps"].as_array_mut().unwrap().swap(4, 5);
    assert!(parsed(wrong).validate().is_err());
    let mut wrong = immune;
    wrong["steps"][3]["component"]["descriptor"] = Value::Null;
    assert!(parsed(wrong).validate().is_err());
}

#[test]
fn trace_bound_and_maximum_pool_are_checked_before_retention_arithmetic() {
    let mut value = attack();
    value["steps"][0]["net_edge"] = json!("65");
    value["steps"][2]["net_edge"] = json!("64");
    let pool = &mut value["steps"][3]["component"];
    pool["expression"] = json!({"count":64,"sides":2,"bonus":0});
    pool["edge"] = json!("64");
    pool["rolled"] = json!([vec![1; 64], vec![2; 64]].concat());
    pool["kept"] = json!(vec![2; 64]);
    pool["raw"] = json!(128);
    pool["after_immunity"] = json!(128);
    value["steps"][4]["before"] = json!(128);
    value["steps"][4]["after"] = json!(125);
    value["steps"][5]["raw"] = json!(128);
    value["steps"][5]["after_immunity"] = json!(128);
    value["steps"][5]["after_descriptors"] = json!(128);
    value["steps"][5]["total"] = json!(125);
    assert!(parsed(value.clone()).validate().is_ok());
    let mut wrong = value.clone();
    wrong["steps"][3]["component"]["rolled"]
        .as_array_mut()
        .unwrap()
        .push(json!(2));
    assert!(parsed(wrong).validate().is_err());
    let mut wrong = value;
    let repeated = wrong["steps"][0].clone();
    wrong["steps"].as_array_mut().unwrap().resize(129, repeated);
    assert!(parsed(wrong).validate().is_err());
    let mut wrong = attack();
    wrong["steps"][0]["net_edge"] = json!("-9223372036854775808");
    assert!(parsed(wrong).validate().is_err());
}

#[test]
fn trace_standalone_checks_use_the_declared_mana_binding_and_disadvantage_keeps_lowest_pool() {
    for (binding, attribute) in [
        ("intellect", "intellect"),
        ("willpower", "willpower"),
        ("awareness", "awareness"),
        ("presence", "presence"),
    ] {
        let mut check = attack()["steps"][1].clone();
        check["check"]["skill"] = json!("spellcasting");
        check["check"]["binding"] = json!(binding);
        check["check"]["attribute"] = json!(attribute);
        assert!(parsed(json!({"truncated":false,"steps":[check]}))
            .validate()
            .is_ok());
    }
    let mut value = attack();
    value["steps"][0]["net_edge"] = json!("-3");
    let check = &mut value["steps"][1]["check"];
    check["input_edge"] = json!("-1");
    check["edge"] = json!("-1");
    check["kept"] = json!(10);
    check["total"] = json!("12");
    value["steps"][2]["net_edge"] = json!("-2");
    let pool = &mut value["steps"][3]["component"];
    pool["edge"] = json!("-2");
    pool["kept"] = json!([1, 2]);
    pool["raw"] = json!(4);
    pool["after_immunity"] = json!(4);
    value["steps"][4]["before"] = json!(4);
    value["steps"][4]["after"] = json!(1);
    value["steps"][5]["raw"] = json!(4);
    value["steps"][5]["after_immunity"] = json!(4);
    value["steps"][5]["after_descriptors"] = json!(4);
    value["steps"][5]["total"] = json!(1);
    assert!(parsed(value).validate().is_ok());
}

#[test]
fn fear_difficulty_and_duration_require_the_actual_caster_operands() {
    let value = json!({"truncated":false,"steps":[
        {"type":"fear_started","difficulty":16,"duration":"400","net_edge":"0","difficulty_attribute":2,"difficulty_rank":1,"difficulty_bonus":3,"duration_bonus":"100"},
        {"type":"fear_immunity","fear":true,"mind_affecting":false},
        {"type":"fear_finished","applied":false,"duration":"0"}
    ]});
    assert!(parsed(value.clone()).validate().is_ok());
    for (field, wrong) in [
        ("difficulty_attribute", json!(3)),
        ("difficulty_rank", json!(2)),
        ("difficulty_bonus", json!(4)),
        ("duration_bonus", json!("101")),
    ] {
        let mut malformed = value.clone();
        malformed["steps"][0][field] = wrong;
        assert!(
            parsed(malformed).validate().is_err(),
            "accepted wrong {field}"
        );
    }
}

fn combat_report() -> Value {
    let state = json!({"health":20,"maximum_health":20,"injury":0,"dead":false,"resources":[
        {"resource":"stamina","maximum":2,"balance":2,"available":2,"recovery_elapsed":"0"},
        {"resource":"focus","maximum":2,"balance":2,"available":2,"recovery_elapsed":"0"},
        {"resource":"mana","maximum":2,"balance":2,"available":2,"recovery_elapsed":"0"}
    ],"fear":[]});
    let mut after = state.clone();
    after["health"] = json!(12);
    after["injury"] = json!(8);
    json!({"enabled":true,"tick":"100","captured":"1","dropped":"0","retained":1,"through":"1","records":[{
        "sequence":"1","tick":"100","actor":"1","target":"2","ability":"basic_melee","intention":"3","origin_intention":"3",
        "preparation":"20","started":"80","remaining":"20","recovery":"20","charge":null,
        "actor_before":state,"actor_after":state,"target_before":state,"target_after":after,
        "applied":true,"damage":8,"trace":attack()
    }]})
}

#[test]
fn combat_report_checks_capture_window_health_costs_timing_and_resolution_agreement() {
    use tor_protocol::CombatDiagnosticsView;
    let value = combat_report();
    let report: CombatDiagnosticsView = serde_json::from_value(value.clone()).unwrap();
    assert!(report.validate().is_ok());
    for (path, wrong) in [
        ("/retained", json!(65)),
        ("/captured", json!("2")),
        ("/dropped", json!("1")),
        ("/records/0/sequence", json!("2")),
        ("/records/0/started", json!("101")),
        ("/records/0/damage", json!(7)),
        ("/records/0/applied", json!(false)),
        ("/records/0/target_after/injury", json!(7)),
        ("/records/0/target_after/dead", json!(true)),
        ("/records/0/actor_after/resources/0/balance", json!(1)),
        (
            "/records/0/actor_after/resources/0/recovery_elapsed",
            json!("100"),
        ),
        (
            "/records/0/charge",
            json!({"resource":"stamina","start":1,"resolution":1}),
        ),
    ] {
        let mut malformed = value.clone();
        *malformed.pointer_mut(path).unwrap() = wrong;
        let parsed: CombatDiagnosticsView = serde_json::from_value(malformed).unwrap();
        assert!(parsed.validate().is_err(), "accepted invalid {path}");
    }
}

#[test]
fn combat_report_distinguishes_overkill_from_actual_injury_and_preserves_large_window_indices() {
    use tor_protocol::CombatDiagnosticsView;
    let mut value = combat_report();
    value["records"][0]["target_before"]["health"] = json!(3);
    value["records"][0]["target_before"]["injury"] = json!(17);
    value["records"][0]["target_after"]["health"] = json!(0);
    value["records"][0]["target_after"]["injury"] = json!(20);
    value["records"][0]["target_after"]["dead"] = json!(true);
    value["records"][0]["damage"] = json!(3);
    let report: CombatDiagnosticsView = serde_json::from_value(value.clone()).unwrap();
    assert!(report.validate().is_ok());
    let mut wrong = value.clone();
    wrong["records"][0]["damage"] = json!(8);
    assert!(serde_json::from_value::<CombatDiagnosticsView>(wrong)
        .unwrap()
        .validate()
        .is_err());
    value["captured"] = json!(u64::MAX.to_string());
    value["through"] = json!(u64::MAX.to_string());
    value["dropped"] = json!((u64::MAX - 64).to_string());
    value["retained"] = json!(64);
    let mut records = vec![value["records"][0].clone(); 8];
    for (index, record) in records.iter_mut().enumerate() {
        record["sequence"] = json!((u64::MAX - 7 + index as u64).to_string());
    }
    value["records"] = json!(records);
    let report: CombatDiagnosticsView = serde_json::from_value(value).unwrap();
    assert!(report.validate().is_ok());
    let encoded = serde_json::to_value(&report).unwrap();
    assert_eq!(encoded["captured"], u64::MAX.to_string());
    assert_eq!(
        serde_json::from_value::<CombatDiagnosticsView>(encoded).unwrap(),
        report
    );
}

#[test]
fn paid_combat_report_requires_original_owner_and_the_actual_split_payment() {
    use tor_protocol::CombatDiagnosticsView;
    let mut value = combat_report();
    let record = &mut value["records"][0];
    record["ability"] = json!("magic_bolt");
    record["charge"] = json!({"resource":"mana","start":1,"resolution":1});
    record["trace"]["steps"][1]["check"]["skill"] = json!("spellcasting");
    record["trace"]["steps"][1]["check"]["attribute"] = json!("intellect");
    record["actor_before"]["resources"][2]["available"] = json!(1);
    record["actor_after"]["resources"][2]["balance"] = json!(1);
    record["actor_after"]["resources"][2]["available"] = json!(1);
    assert!(
        serde_json::from_value::<CombatDiagnosticsView>(value.clone())
            .unwrap()
            .validate()
            .is_ok()
    );
    for (path, wrong) in [
        ("/records/0/origin_intention", Value::Null),
        ("/records/0/charge/resolution", json!(2)),
        ("/records/0/actor_after/resources/2/balance", json!(2)),
        ("/records/0/ability", json!("power_strike")),
    ] {
        let mut malformed = value.clone();
        *malformed.pointer_mut(path).unwrap() = wrong;
        assert!(serde_json::from_value::<CombatDiagnosticsView>(malformed)
            .unwrap()
            .validate()
            .is_err());
    }
}

#[test]
fn report_pages_reject_expired_and_duplicate_sequences_and_disabled_capture_history() {
    use tor_protocol::CombatDiagnosticsView;
    let mut value = combat_report();
    value["captured"] = json!("70");
    value["dropped"] = json!("6");
    value["retained"] = json!(64);
    value["through"] = json!("14");
    let mut records = vec![value["records"][0].clone(); 8];
    for (index, record) in records.iter_mut().enumerate() {
        record["sequence"] = json!((7 + index).to_string());
    }
    value["records"] = json!(records);
    assert!(
        serde_json::from_value::<CombatDiagnosticsView>(value.clone())
            .unwrap()
            .validate()
            .is_ok()
    );
    let mut wrong = value.clone();
    wrong["records"][1]["sequence"] = json!("7");
    assert!(serde_json::from_value::<CombatDiagnosticsView>(wrong)
        .unwrap()
        .validate()
        .is_err());
    let mut wrong = value.clone();
    wrong["through"] = json!("5");
    assert!(serde_json::from_value::<CombatDiagnosticsView>(wrong)
        .unwrap()
        .validate()
        .is_err());
    let mut wrong = value;
    wrong["enabled"] = json!(false);
    assert!(serde_json::from_value::<CombatDiagnosticsView>(wrong)
        .unwrap()
        .validate()
        .is_err());
    let empty = json!({"enabled":false,"tick":"100","captured":"0","dropped":"0","retained":0,"through":"0","records":[]});
    assert!(serde_json::from_value::<CombatDiagnosticsView>(empty)
        .unwrap()
        .validate()
        .is_ok());
    let mut wrong = combat_report();
    wrong["records"][0]
        .as_object_mut()
        .unwrap()
        .remove("origin_intention");
    assert!(serde_json::from_value::<CombatDiagnosticsView>(wrong).is_err());
}

#[test]
fn fear_report_links_paid_focus_to_the_actual_refreshed_causer_relation() {
    use tor_protocol::CombatDiagnosticsView;
    let mut value = combat_report();
    let record = &mut value["records"][0];
    record["ability"] = json!("fear");
    record["damage"] = json!(0);
    record["charge"] = json!({"resource":"focus","start":1,"resolution":1});
    record["actor_before"]["resources"][1]["available"] = json!(1);
    record["actor_after"]["resources"][1]["balance"] = json!(1);
    record["actor_after"]["resources"][1]["available"] = json!(1);
    record["target_before"]["fear"] = json!([{"causer":"1","remaining_ticks":"1000"}]);
    record["target_after"] = record["target_before"].clone();
    record["trace"] = json!({"truncated":false,"steps":[
        {"type":"fear_started","difficulty":16,"duration":"400","net_edge":"0","difficulty_attribute":2,"difficulty_rank":1,"difficulty_bonus":3,"duration_bonus":"100"},
        {"type":"check","check":{"skill":"discipline","binding":"intellect","attribute":"willpower","attribute_value":2,"rank":1,"modifier":0,"threshold":16,"input_edge":"0","edge":"0","unused_edge":"0","rng_before":"42","rng_after":"43","first":2,"second":null,"kept":2,"total":"5","success":false}},
        {"type":"fear_finished","applied":true,"duration":"400"}
    ]});
    assert!(
        serde_json::from_value::<CombatDiagnosticsView>(value.clone())
            .unwrap()
            .validate()
            .is_ok()
    );
    let mut wrong = value.clone();
    wrong["records"][0]["target_after"]["fear"][0]["remaining_ticks"] = json!("400");
    assert!(
        serde_json::from_value::<CombatDiagnosticsView>(wrong)
            .unwrap()
            .validate()
            .is_err(),
        "refresh cannot shorten a stronger relation"
    );
    let mut wrong = value;
    wrong["records"][0]["target_after"]["fear"][0]["causer"] = json!("2");
    assert!(serde_json::from_value::<CombatDiagnosticsView>(wrong)
        .unwrap()
        .validate()
        .is_err());
}
