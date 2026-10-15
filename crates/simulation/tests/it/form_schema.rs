//! Fixed forms share content identity and ordinary ability references.
use super::creature_fixture as fixture;
use serde_json::{json, Value};
use tor_simulation::{
    content_ids::{AbilityId, FormId, TalentId},
    player_advancement::PurchaseChoice,
    rules::{
        FormDef, Modifier, ModifierOp, Requirement, Ruleset, RulesetDocument, Scope, TalentDef,
    },
};
fn form(id: &str) -> FormId {
    FormId::new(id).unwrap()
}
fn definition() -> FormDef {
    FormDef {
        id: form("starter.fire"),
        name: "Fire form".into(),
        ability: AbilityId::new("basic_melee").unwrap(),
    }
}
fn root() -> TalentDef {
    TalentDef {
        id: TalentId::new("starter_root").unwrap(),
        name: "Starter root".into(),
        prerequisites: vec![],
        requirement: Requirement::TotalHd {
            minimum: tor_simulation::HdCount(1),
        },
        modifiers: vec![],
        form_choices: vec![form("starter.fire")],
    }
}
fn document() -> RulesetDocument {
    let mut document = fixture::ruleset().document().clone();
    document.forms.push(definition());
    document.talents.push(root());
    document
}
#[test]
fn fixed_form_byte_decoder_requires_one_named_bounded_record() {
    let expected = definition();
    let valid = serde_json::to_vec(&expected).unwrap();
    assert_eq!(serde_json::from_slice::<FormDef>(&valid).unwrap(), expected);
    for value in [
        json!(["starter.fire", "Fire form", "basic_melee"]),
        json!({"id":"starter.fire","name":"Fire form"}),
        json!({"id":"starter.fire","name":"Fire form","ability":null}),
        json!({"id":"starter.fire","name":"Fire form","ability":"basic_melee","extra":0}),
        json!({"id":"starter.fire","name":"  ","ability":"basic_melee"}),
        json!({"id":"starter.fire","name":"Fire\nform","ability":"basic_melee"}),
        json!({"id":"starter.fire","name":"x".repeat(tor_simulation::limits::DISPLAY_NAME_BYTES + 1),"ability":"basic_melee"}),
        json!({"id":"bad form","name":"Fire form","ability":"basic_melee"}),
        json!({"id":"starter.fire","name":"Fire form","ability":"x".repeat(tor_simulation::limits::ID_BYTES + 1)}),
    ] {
        assert!(
            serde_json::from_slice::<FormDef>(&serde_json::to_vec(&value).unwrap()).is_err(),
            "accepted {value}"
        );
    }
    let duplicate =
        br#"{"id":"starter.fire","name":"Fire form","ability":"basic_melee","ability":"fear"}"#;
    assert!(serde_json::from_slice::<FormDef>(duplicate).is_err());
}
#[test]
fn talent_choice_byte_decoder_enforces_sixteen_and_source_empty_shorthand() {
    let mut value = serde_json::to_value(root()).unwrap();
    value["form_choices"] = json!((0..16)
        .map(|index| format!("starter.f{index}"))
        .collect::<Vec<_>>());
    assert_eq!(
        serde_json::from_slice::<TalentDef>(&serde_json::to_vec(&value).unwrap())
            .unwrap()
            .form_choices
            .len(),
        16
    );
    value["form_choices"]
        .as_array_mut()
        .unwrap()
        .push(json!("starter.f16"));
    assert!(serde_json::from_slice::<TalentDef>(&serde_json::to_vec(&value).unwrap()).is_err());
    value["form_choices"] = Value::Null;
    assert!(serde_json::from_slice::<TalentDef>(&serde_json::to_vec(&value).unwrap()).is_err());
    value.as_object_mut().unwrap().remove("form_choices");
    assert!(
        serde_json::from_slice::<TalentDef>(&serde_json::to_vec(&value).unwrap())
            .unwrap()
            .form_choices
            .is_empty()
    );
    value["unexpected"] = json!(0);
    assert!(serde_json::from_slice::<TalentDef>(&serde_json::to_vec(&value).unwrap()).is_err());
    assert!(serde_json::from_slice::<TalentDef>(
        br#"["starter_root","Starter",[],{"kind":"total_hd","minimum":1},[],[]]"#
    )
    .is_err());
    let duplicate = serde_json::to_string(&root())
        .unwrap()
        .replace("\"form_choices\":", "\"form_choices\":[],\"form_choices\":");
    assert!(serde_json::from_slice::<TalentDef>(duplicate.as_bytes()).is_err());
}
#[test]
fn rules_document_requires_a_bounded_form_catalog_on_actual_bytes() {
    let mut value = serde_json::to_value(document()).unwrap();
    let bytes = serde_json::to_vec(&value).unwrap();
    assert!(Ruleset::validate(serde_json::from_slice(&bytes).unwrap()).is_ok());
    value["forms"] = Value::Null;
    assert!(
        serde_json::from_slice::<RulesetDocument>(&serde_json::to_vec(&value).unwrap()).is_err()
    );
    value.as_object_mut().unwrap().remove("forms");
    assert!(
        serde_json::from_slice::<RulesetDocument>(&serde_json::to_vec(&value).unwrap()).is_err()
    );
    value["forms"] = json!(vec![definition(); 4096]);
    assert_eq!(
        serde_json::from_slice::<RulesetDocument>(&serde_json::to_vec(&value).unwrap())
            .unwrap()
            .forms
            .len(),
        4096
    );
    value["forms"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::to_value(definition()).unwrap());
    assert!(
        serde_json::from_slice::<RulesetDocument>(&serde_json::to_vec(&value).unwrap()).is_err()
    );
    let duplicate = String::from_utf8(bytes)
        .unwrap()
        .replace("\"forms\":", "\"forms\":[],\"forms\":");
    assert!(serde_json::from_slice::<RulesetDocument>(duplicate.as_bytes()).is_err());
}
#[test]
fn form_registry_and_choice_references_are_validated_before_publication() {
    let original = document();
    let rules = Ruleset::validate(original.clone()).unwrap();
    assert_eq!(rules.form(&form("starter.fire")).unwrap(), &definition());
    let mut cases = Vec::new();
    let mut duplicate = original.clone();
    duplicate.forms.push(definition());
    cases.push(duplicate);
    let mut unknown_ability = original.clone();
    unknown_ability.forms[0].ability = AbilityId::new("missing_ability").unwrap();
    cases.push(unknown_ability);
    let mut unknown_form = original.clone();
    unknown_form.talents.last_mut().unwrap().form_choices = vec![form("missing_form")];
    cases.push(unknown_form);
    let mut duplicate_choice = original.clone();
    duplicate_choice
        .talents
        .last_mut()
        .unwrap()
        .form_choices
        .push(form("starter.fire"));
    cases.push(duplicate_choice);
    let mut oversized_choice = original.clone();
    for index in 1..17 {
        let mut def = definition();
        def.id = form(&format!("starter.f{index}"));
        oversized_choice
            .talents
            .last_mut()
            .unwrap()
            .form_choices
            .push(def.id.clone());
        oversized_choice.forms.push(def);
    }
    cases.push(oversized_choice);
    let mut invalid_name = original;
    invalid_name.forms[0].name = "\n".into();
    cases.push(invalid_name);
    for bad in cases {
        assert!(Ruleset::validate(bad).is_err());
    }
}
#[test]
fn form_root_reserves_capacity_for_one_grant_without_restricting_ordinary_roots() {
    let mut document = document();
    let modifier = Modifier {
        scope: Scope::Always,
        op: ModifierOp::GrantAbility {
            ability: AbilityId::new("basic_melee").unwrap(),
        },
    };
    document.talents.last_mut().unwrap().modifiers = vec![modifier.clone(); 127];
    assert!(Ruleset::validate(document.clone()).is_ok());
    document
        .talents
        .last_mut()
        .unwrap()
        .modifiers
        .push(modifier);
    assert!(Ruleset::validate(document.clone()).is_err());
    document.talents.last_mut().unwrap().form_choices.clear();
    assert!(Ruleset::validate(document).is_ok());
}
#[test]
fn owner_choice_byte_decoder_preserves_required_form_and_rejects_ambiguous_operands() {
    let valid = br#"{"kind":"talent_form","talent":"starter_root","form":"starter.fire"}"#;
    let choice: PurchaseChoice = serde_json::from_slice(valid).unwrap();
    assert_eq!(
        choice.selection().talent().unwrap().as_str(),
        "starter_root"
    );
    assert_eq!(choice.selection().form().unwrap().as_str(), "starter.fire");
    assert_eq!(serde_json::to_vec(&choice).unwrap(), valid);
    for bad in [br#"{"kind":"talent_form","talent":"starter_root"}"#.as_slice(),
        br#"{"kind":"talent_form","talent":"starter_root","form":null}"#,
        br#"{"kind":"talent_form","talent":"starter_root","form":"starter.fire","form":"starter.cold"}"#,
        br#"{"kind":"talent_form","talent":"starter_root","form":"starter.fire","extra":0}"#,
        br#"["talent_form","starter_root","starter.fire"]"#,
        br#"{"kind":"talent","talent":"starter_root","form":"starter.fire"}"#,
    ] { assert!(serde_json::from_slice::<PurchaseChoice>(bad).is_err()); }
}
