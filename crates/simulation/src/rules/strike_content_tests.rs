//! These tests decode the real rules document, so the first red checkpoint also
//! verifies that an unsupported source schema fails at the production boundary.
use super::*;
use serde_json::{json, Value};

fn strike(offset: u64) -> Value {
    json!({"offset": offset, "unlock": null, "check": null,
        "on_success": [], "on_failure": [], "descriptors": [], "riders": []})
}
fn document_with(strikes: Vec<Value>) -> Value {
    let document: RulesetDocument = toml::from_str(DEFAULT_RULESET_TOML).unwrap();
    let mut value = serde_json::to_value(document).unwrap();
    // Change only the probe's source. Other production abilities can have
    // invocation-specific payload requirements (for example a selected step).
    {
        let ability = &mut value["abilities"][0];
        let object = ability.as_object_mut().unwrap();
        object.remove("check");
        object.remove("on_success");
        object.remove("on_failure");
        object.remove("descriptors");
        object.insert(
            "strikes".into(),
            json!({"kind": "authored", "strikes": [strike(0)]}),
        );
    }
    value["abilities"][0]["strikes"]["strikes"] = json!(strikes);
    value
}
fn decode(value: Value) -> Result<Arc<Ruleset>, String> {
    let document =
        serde_json::from_value::<RulesetDocument>(value).map_err(|error| error.to_string())?;
    Ruleset::validate(document).map_err(|error| error.to_string())
}

fn equipment_alteration(slot: &str, class: &str, alteration: &str) -> Value {
    json!({"kind":"alter_equipment", "selection":{
        "kind":"slot", "slot":slot, "class":class, "last":false
    }, "alteration":alteration})
}

fn strike_with_equipment_alteration(branch: &str, effect: Value) -> Value {
    let mut row = strike(0);
    if branch == "rider" {
        row["riders"] = json!([{
            "trigger":"success", "check":null, "effects":[effect], "descriptors":[]
        }]);
    } else {
        row[branch] = json!([effect]);
    }
    row
}

#[test]
fn equipment_alteration_slots_are_checked_in_authored_abilities_and_natural_riders() {
    for (class, alteration, compatible, incompatible) in [
        ("armor", "enchant_armor", "body_armor", "weapon"),
        ("armor", "protect_armor", "body_armor", "weapon"),
        ("weapon", "enchant_weapon", "weapon", "body_armor"),
        ("armor", "remove_curse", "body_armor", "weapon"),
    ] {
        for branch in ["on_success", "on_failure", "rider", "additional_rider"] {
            for (slot, valid) in [(compatible, true), (incompatible, false)] {
                let effect = equipment_alteration(slot, class, alteration);
                let mut value = if branch == "additional_rider" {
                    let mut value = document_with(vec![strike(0)]);
                    value["abilities"][0]["strikes"] = json!({
                        "kind":"natural_melee", "additional_riders":[{
                            "trigger":"success", "check":null, "effects":[effect],
                            "descriptors":[]
                        }]
                    });
                    value
                } else {
                    document_with(vec![strike_with_equipment_alteration(branch, effect)])
                };
                value["abilities"][0]["basic_attack"] = json!(false);
                let result = decode(value);
                if valid {
                    assert!(result.is_ok(), "{alteration}/{branch}/{slot}: {result:?}");
                } else {
                    assert!(
                        result.is_err_and(|error| error.contains("equipment_selection")),
                        "{alteration}/{branch}/{slot} must reject at content validation"
                    );
                }
            }
        }
    }
}

#[test]
fn equipment_alteration_slots_are_checked_in_natural_strikes_and_direct_effects() {
    let rules = decode(document_with(vec![strike(0)])).unwrap();
    for (slot, valid) in [("body_armor", true), ("weapon", false)] {
        let effect = equipment_alteration(slot, "armor", "enchant_armor");
        let direct: Effect = serde_json::from_value(effect.clone()).unwrap();
        assert_eq!(rules.validate_effects(&[direct]).is_ok(), valid);
        for branch in ["on_success", "on_failure", "rider"] {
            let strike: StrikeDef =
                serde_json::from_value(strike_with_equipment_alteration(branch, effect.clone()))
                    .unwrap();
            let result = rules.validate_strikes(&[strike]);
            if valid {
                assert!(result.is_ok(), "{branch}/{slot}: {result:?}");
            } else {
                assert!(
                    matches!(result, Err(RulesetError::Bound { field }) if field == "equipment_selection"),
                    "{branch}/{slot} must reject at natural-strike validation"
                );
            }
        }
    }
}

#[test]
fn sequence_rider_condition_is_closed_bounded_and_success_only() {
    let mut row = strike(0);
    row["riders"] = json!([{
        "trigger":"success", "condition":{"kind":"sequence_successful_hits","minimum":2},
        "check":null, "effects":[], "descriptors":[]
    }]);
    let valid = document_with(vec![row]);
    assert!(decode(valid.clone()).is_ok());
    for minimum in [0, 17, u16::MAX] {
        let mut bad = valid.clone();
        bad["abilities"][0]["strikes"]["strikes"][0]["riders"][0]["condition"]["minimum"] =
            json!(minimum);
        assert!(decode(bad).is_err(), "minimum {minimum} must reject");
    }
    for mutation in ["failure", "unknown", "missing", "extra"] {
        let mut bad = valid.clone();
        let rider = &mut bad["abilities"][0]["strikes"]["strikes"][0]["riders"][0];
        match mutation {
            "failure" => rider["trigger"] = json!("failure"),
            "unknown" => rider["condition"]["kind"] = json!("any_effect"),
            "missing" => {
                rider["condition"]
                    .as_object_mut()
                    .unwrap()
                    .remove("minimum");
            }
            "extra" => rider["condition"]["target"] = json!("any"),
            _ => unreachable!(),
        }
        assert!(decode(bad).is_err(), "{mutation} must reject");
    }
    let mut ordinary = valid;
    ordinary["abilities"][0]["strikes"]["strikes"][0]["riders"][0]
        .as_object_mut()
        .unwrap()
        .remove("condition");
    assert!(
        decode(ordinary).is_ok(),
        "current authoring may omit the optional gate"
    );
}
#[test]
fn ordered_strike_content_decodes_authored_and_selected_melee_sources() {
    let mut value = document_with(vec![strike(0)]);
    assert!(
        decode(value.clone()).is_ok(),
        "a one-strike authored source must decode"
    );
    value["abilities"][0]["strikes"] = json!({"kind": "selected_melee"});
    assert!(
        decode(value).is_ok(),
        "the full selected melee source must decode"
    );
}
#[test]
fn ordered_strike_content_accepts_equal_offsets_and_checks_every_boundary() {
    let valid = document_with(vec![strike(0); 16]);
    assert!(
        decode(valid.clone()).is_ok(),
        "sixteen simultaneous strikes are bounded content"
    );
    assert!(decode(document_with(vec![strike(0); 17])).is_err());
    assert!(decode(document_with(vec![])).is_err());
    assert!(decode(document_with(vec![strike(1)])).is_err());
    assert!(decode(document_with(vec![strike(0), strike(2), strike(1)])).is_err());
    assert!(decode(document_with(vec![strike(0), strike(u64::MAX)])).is_err());
    let mut first_locked = valid;
    first_locked["abilities"][0]["strikes"]["strikes"][0]["unlock"] = json!("missing");
    assert!(decode(first_locked).is_err());
}
#[test]
fn ordered_strike_content_bounds_riders_and_combined_effects() {
    let rider = json!({"trigger": "success", "check": null, "effects": [], "descriptors": []});
    let mut first = strike(0);
    first["riders"] = json!(vec![rider.clone(); 8]);
    assert!(
        decode(document_with(vec![first.clone()])).is_ok(),
        "eight riders are valid"
    );
    first["riders"] = json!(vec![rider; 9]);
    assert!(decode(document_with(vec![first])).is_err());
    let effect = json!({"kind": "heal", "amount": {"kind": "fixed", "amount": 1}});
    let mut first = strike(0);
    first["on_success"] = json!(vec![effect.clone(); 128]);
    first["on_failure"] = json!(vec![effect.clone(); 128]);
    assert!(
        decode(document_with(vec![first.clone()])).is_ok(),
        "256 effects across both branches are valid"
    );
    first["riders"] =
        json!([{"trigger": "success", "check": null, "effects": [effect], "descriptors": []}]);
    assert!(
        decode(document_with(vec![first])).is_err(),
        "rider effects count toward the aggregate cap"
    );
}
#[test]
fn ordered_strike_content_rejects_legacy_payloads_and_recursive_riders() {
    let valid = document_with(vec![strike(0)]);
    assert!(decode(valid.clone()).is_ok());
    let mut legacy = valid.clone();
    legacy["abilities"][0]["on_success"] = json!([]);
    assert!(decode(legacy).is_err());
    let mut recursive = valid;
    recursive["abilities"][0]["strikes"]["strikes"][0]["riders"] = json!([{"trigger": "success", "check": null, "effects": [], "descriptors": [], "riders": []}]);
    assert!(decode(recursive).is_err());
}
#[test]
fn ordered_strike_content_counts_descriptors_across_strikes_and_riders() {
    let names = (0..16)
        .map(|index| format!("strike_descriptor_{index}"))
        .collect::<Vec<_>>();
    let mut row = strike(0);
    row["descriptors"] = json!(names);
    let mut value = document_with(vec![row; 16]);
    value["descriptors"].as_array_mut().unwrap().extend(
        names
            .iter()
            .map(|name| json!({"id": name, "name": name, "resistance": null})),
    );
    assert!(
        decode(value.clone()).is_ok(),
        "256 total descriptors are valid"
    );
    value["abilities"][0]["strikes"]["strikes"][0]["riders"] =
        json!([{"trigger": "success", "check": null, "effects": [], "descriptors": [names[0]]}]);
    assert!(
        decode(value).is_err(),
        "rider descriptors share the sequence-wide bound"
    );
}
#[test]
fn ordered_strike_content_validates_unlocks_without_reordering_authored_rows() {
    let mut second = strike(2);
    second["unlock"] = json!("rattling_blow");
    let value = document_with(vec![strike(0), second.clone(), strike(3)]);
    let rules = decode(value).unwrap();
    let serialized = serde_json::to_value(rules.document()).unwrap();
    let mut expected = second.clone();
    expected["cancellable"] = json!(false);
    assert_eq!(serialized["abilities"][0]["strikes"]["strikes"][1], expected);
    let mut unknown = second;
    unknown["unlock"] = json!("missing_talent");
    assert!(decode(document_with(vec![strike(0), unknown])).is_err());
}
#[test]
fn ordered_strike_content_rejects_missing_lists_and_unsupported_rider_rolls_at_decode() {
    let original = document_with(vec![strike(0)]);
    assert!(decode(original.clone()).is_ok());
    for field in ["on_success", "on_failure", "descriptors", "riders"] {
        let mut value = original.clone();
        value["abilities"][0]["strikes"]["strikes"][0]
            .as_object_mut()
            .unwrap()
            .remove(field);
        assert!(
            serde_json::from_value::<RulesetDocument>(value).is_err(),
            "missing {field}"
        );
    }
    let mut value = original;
    value["abilities"][0]["strikes"]["strikes"][0]["riders"] = json!([{"trigger": "success", "check": {"actor": "target", "skill": {"kind": "natural_attack"}, "defense": "physical"}, "effects": [], "descriptors": []}]);
    assert!(serde_json::from_value::<RulesetDocument>(value).is_err());
}

#[test]
fn ordered_strike_content_payload_free_source_and_check_selectors_reject_unknown_fields() {
    let mut source = document_with(vec![strike(0)]);
    source["abilities"][0]["strikes"] = json!({"kind": "selected_melee"});
    assert!(decode(source.clone()).is_ok());
    source["abilities"][0]["strikes"]["future_payload"] = json!([]);
    assert!(serde_json::from_value::<RulesetDocument>(source).is_err());
    for kind in ["selected_weapon", "natural_attack"] {
        let mut source = document_with(vec![strike(0)]);
        source["abilities"][0]["strikes"]["strikes"][0]["check"] = json!({"actor": "user", "skill": {"kind": kind, "private_owner": 99}, "against": {"kind": "defense", "defense": "physical"}});
        assert!(
            serde_json::from_value::<RulesetDocument>(source).is_err(),
            "{kind}"
        );
    }
}

#[test]
fn ordered_strike_content_descriptor_lookup_uses_pinned_index_and_counts_only_requests() {
    let mut document: RulesetDocument = toml::from_str(DEFAULT_RULESET_TOML).unwrap();
    for ordinal in (0..128).rev() {
        document.descriptors.push(NamedDescriptor {
            resistance: None,
            id: DescriptorId::new(format!("inspection_descriptor_{ordinal}")).unwrap(),
            name: format!("Descriptor {ordinal}"),
        });
    }
    let rules = Ruleset::validate(document).unwrap();
    let before = crate::diagnostics::work_counts().descriptor_lookups;
    for ordinal in [0, 63, 127] {
        let id = DescriptorId::new(format!("inspection_descriptor_{ordinal}")).unwrap();
        assert_eq!(
            rules.descriptor(&id).unwrap().name,
            format!("Descriptor {ordinal}")
        );
    }
    assert!(rules
        .descriptor(&DescriptorId::new("missing_descriptor").unwrap())
        .is_none());
    assert_eq!(
        crate::diagnostics::work_counts().descriptor_lookups - before,
        4
    );
}

#[test]
fn ordered_strikes_rider_trigger_is_required_and_failure_needs_a_primary_check() {
    for trigger in ["success", "failure"] {
        let mut row = strike(0);
        row["check"] = json!({"actor":"target","skill":{"kind":"named","skill":"athletics"},
            "against":{"kind":"difficulty","formula":{"base":10,"skill":null,"attribute":null}}});
        row["riders"] = json!([{"trigger":trigger,"check":null,"effects":[],"descriptors":[]}]);
        let valid = document_with(vec![row.clone()]);
        assert!(
            decode(valid.clone()).is_ok(),
            "explicit {trigger} branch must be supported"
        );
        let mut missing = valid.clone();
        missing["abilities"][0]["strikes"]["strikes"][0]["riders"][0]
            .as_object_mut()
            .unwrap()
            .remove("trigger");
        assert!(decode(missing).is_err(), "no implicit rider trigger");
        for invalid in [json!("hit"), json!(null), json!({"success":true})] {
            let mut forged = valid.clone();
            forged["abilities"][0]["strikes"]["strikes"][0]["riders"][0]["trigger"] = invalid;
            assert!(decode(forged).is_err());
        }
        row["check"] = Value::Null;
        assert_eq!(
            decode(document_with(vec![row])).is_ok(),
            trigger == "success",
            "a checkless primary cannot select the failure branch"
        );
    }
}

// Reconstructed natural source contract; the historical implementation is missing.
#[test]
fn natural_melee_source_requires_explicit_bounded_riders() {
    let mut value = document_with(vec![strike(0)]);
    value["abilities"][0]["strikes"] = json!({"kind":"natural_melee", "additional_riders":[]});
    assert!(decode(value.clone()).is_ok(), "natural source must decode");
    value["abilities"][0]["strikes"]
        .as_object_mut()
        .unwrap()
        .remove("additional_riders");
    assert!(decode(value.clone()).is_err());
    let rider = json!({"trigger":"success", "check":null, "effects":[], "descriptors":[]});
    value["abilities"][0]["strikes"]["additional_riders"] = json!(vec![rider.clone(); 8]);
    assert!(decode(value.clone()).is_ok());
    value["abilities"][0]["strikes"]["additional_riders"] = json!(vec![rider; 9]);
    assert!(decode(value).is_err());
}

#[test]
fn natural_melee_source_checks_rider_references_and_unknown_fields() {
    let mut value = document_with(vec![strike(0)]);
    value["abilities"][0]["strikes"] = json!({"kind":"natural_melee", "additional_riders":[{
        "trigger":"success", "check":{"skill":{"kind":"natural_attack"},"defense":"physical"},
        "effects":[{"kind":"apply_condition","condition":"rattled","duration":200}], "descriptors":[]
    }]});
    assert!(decode(value.clone()).is_ok());
    let mut unknown = value.clone();
    unknown["abilities"][0]["strikes"]["additional_riders"][0]["effects"][0]["condition"] =
        json!("missing");
    assert!(decode(unknown).is_err());
    value["abilities"][0]["strikes"]["strikes"] = json!([]);
    assert!(decode(value).is_err());
}

#[test]
fn natural_melee_source_composition_bounds_dormant_rows_and_branch_triggers() {
    let mut row: StrikeDef = serde_json::from_value(strike(0)).unwrap();
    let extra: RiderDef = serde_json::from_value(
        json!({"trigger":"success","check":null,"effects":[],"descriptors":[]}),
    )
    .unwrap();
    row.riders = vec![extra.clone(); 8];
    let mut dormant = row.clone();
    dormant.offset = 10;
    dormant.unlock = Some(TalentId::new("rattling_blow").unwrap());
    assert!(append_natural_riders(
        &[serde_json::from_value(strike(0)).unwrap(), dormant],
        std::slice::from_ref(&extra)
    )
    .is_err());
    assert!(append_natural_riders(&[row], &[]).is_ok());
    let failure = RiderDef {
        condition: None,
        trigger: RiderTrigger::Failure,
        ..extra
    };
    assert!(
        append_natural_riders(&[serde_json::from_value(strike(0)).unwrap()], &[failure]).is_err()
    );
}

#[test]
fn natural_melee_source_strike_and_rider_records_require_names() {
    assert!(serde_json::from_value::<StrikeDef>(json!([0, null, null, [], [], [], []])).is_err());
    assert!(serde_json::from_value::<RiderDef>(json!(["success", null, [], []])).is_err());
    let mut value = document_with(vec![strike(0)]);
    value["abilities"][0]["strikes"] = json!({"kind":"natural_melee", "additional_riders":[]});
    for reach in [
        json!({"kind":"sight","range":2}),
        json!({"kind":"self_only"}),
    ] {
        value["abilities"][0]["reach"] = reach;
        assert!(decode(value.clone()).is_err());
    }
}

#[test]
fn natural_melee_source_repeated_riders_share_the_full_sequence_budget() {
    let row: StrikeDef = serde_json::from_value(strike(0)).unwrap();
    let extra = RiderDef {
        cancellable: false,
        condition: None,
        trigger: RiderTrigger::Success,
        check: None,
        descriptors: vec![],
        effects: vec![
            Effect::Heal {
                amount: AmountExpr::Fixed { amount: 1 }
            };
            128
        ],
    };
    assert!(
        append_natural_riders(&[row.clone(), row.clone()], std::slice::from_ref(&extra)).is_ok()
    );
    assert!(append_natural_riders(&[row.clone(), row.clone(), row], &[extra]).is_err());
}

#[test]
fn adjacent_search_is_only_a_non_hostile_self_ability() {
    let mut row = strike(0);
    row["on_success"] = json!([{"kind":"search_adjacent_guises"}]);
    let mut valid = document_with(vec![row]);
    valid["abilities"][0]["basic_attack"] = json!(false);
    valid["abilities"][0]["interaction"] = json!("non_hostile");
    valid["abilities"][0]["reach"] = json!({"kind":"self_only"});
    assert!(decode(valid.clone()).is_ok());
    for interaction in ["hostile_to_target", "hostile_area"] {
        let mut invalid = valid.clone();
        invalid["abilities"][0]["interaction"] = json!(interaction);
        assert!(decode(invalid).is_err());
    }
    for reach in [
        json!({"kind":"melee"}),
        json!({"kind":"sight","range":4}),
        json!({"kind":"self_or_ally","range":{"kind":"touch"}}),
    ] {
        let mut invalid = valid.clone();
        invalid["abilities"][0]["reach"] = reach;
        assert!(decode(invalid).is_err());
    }
}
