use super::*;
use crate::rules::{Modifier, ModifierOp, Scope};

fn modifier(value: PerceptionValue) -> Modifier {
    Modifier { scope: Scope::Always, op: ModifierOp::Perception { value } }
}

fn guise(name: &str) -> GuiseAppearance {
    GuiseAppearance {
        name: name.into(),
        description: "An ordinary closed chest.".into(),
        class: crate::ItemClass::Tool,
        asset: None,
    }
}

#[test]
fn capability_fold_is_order_independent_and_suppression_wins() {
    let mut sources = vec![
        modifier(PerceptionValue::Invisible { enabled: true }),
        modifier(PerceptionValue::SeeInvisible { enabled: true }),
        modifier(PerceptionValue::ObjectGuise { appearance: Some(guise("chest")) }),
        modifier(PerceptionValue::AmbushFromGuise { enabled: true }),
    ];
    let folded = PerceptionCapabilities::from_modifiers(sources.iter()).unwrap();
    assert!(folded.invisible && folded.see_invisible && folded.ambush_from_guise);
    assert!(folded.sight);
    assert_eq!(folded.object_guise, Some(guise("chest")));
    sources.reverse();
    assert_eq!(PerceptionCapabilities::from_modifiers(sources.iter()).unwrap(), folded);
    sources.extend([
        modifier(PerceptionValue::Invisible { enabled: false }),
        modifier(PerceptionValue::SeeInvisible { enabled: false }),
        modifier(PerceptionValue::Sight { enabled: false }),
        modifier(PerceptionValue::ObjectGuise { appearance: None }),
        modifier(PerceptionValue::AmbushFromGuise { enabled: false }),
    ]);
    let folded = PerceptionCapabilities::from_modifiers(sources.iter()).unwrap();
    assert!(!folded.invisible && !folded.see_invisible && !folded.sight);
    assert!(!folded.ambush_from_guise && folded.object_guise.is_none());
}

#[test]
fn contradictory_authored_guises_fail_in_either_source_order() {
    let mut sources = vec![
        modifier(PerceptionValue::ObjectGuise { appearance: Some(guise("chest")) }),
        modifier(PerceptionValue::ObjectGuise { appearance: Some(guise("urn")) }),
    ];
    assert!(PerceptionCapabilities::from_modifiers(sources.iter()).is_err());
    sources.reverse();
    assert!(PerceptionCapabilities::from_modifiers(sources.iter()).is_err());
}

#[test]
fn perception_source_records_reject_positional_extra_and_missing_nullable_fields() {
    for invalid in [
        serde_json::json!(["invisible", true]),
        serde_json::json!({"kind":"invisible", "enabled":true, "secret":17}),
        serde_json::json!({"kind":"object_guise"}),
        serde_json::json!({"kind":"object_guise", "appearance":{"name":"", "description":"", "class":"tool", "asset":null}}),
        serde_json::json!({"kind":"object_guise", "appearance":{"name":"chest", "description":"object", "class":"tool"}}),
    ] {
        assert!(serde_json::from_value::<PerceptionValue>(invalid).is_err());
    }
    let suppression = PerceptionValue::ObjectGuise { appearance: None };
    assert_eq!(serde_json::from_value::<PerceptionValue>(serde_json::to_value(&suppression).unwrap()).unwrap(), suppression);
    let mut invalid = modifier(PerceptionValue::Invisible { enabled: true });
    invalid.scope = Scope::VersusSource;
    assert!(crate::rules::validate_modifier_shapes(&[invalid]).is_err());
}

#[test]
fn guise_enters_once_and_source_loss_never_rearms_it() {
    let source = PerceptionCapabilities { object_guise: Some(guise("chest")), ambush_from_guise: true, ..Default::default() };
    let mut state = GuiseState::default();
    state.reconcile(&source, false);
    assert_eq!(state.appearance(), Some(&guise("chest")));
    assert!(state.ambush_available());
    state.reveal();
    assert!(state.appearance().is_none());
    state.reconcile(&PerceptionCapabilities::default(), false);
    state.reconcile(&source, false);
    assert!(state.appearance().is_none());
    assert!(!state.ambush_available());
}

#[test]
fn ordinary_later_ambush_grant_can_arm_only_the_unbroken_initial_guise() {
    let mut source = PerceptionCapabilities { object_guise: Some(guise("chest")), ..Default::default() };
    let mut state = GuiseState::default();
    state.reconcile(&source, false);
    assert!(!state.ambush_available());
    source.ambush_from_guise = true;
    state.reconcile(&source, false);
    assert!(state.ambush_available());
    source.ambush_from_guise = false;
    state.reconcile(&source, false);
    source.ambush_from_guise = true;
    state.reconcile(&source, false);
    assert!(!state.ambush_available());
}
