//! Stationary-scene capability changes must cross the ordinary Service boundary.
use super::*;
use serde_json::json;
use std::sync::Arc;

fn scenario(suppressed: bool) -> crate::Scenario {
    use crate::scenario_package::{Manifest, Package, RegionDef};
    use tor_simulation::rules::{Modifier, ModifierOp, Scope, Timing, TimingSource};
    let manifest: Manifest = toml::from_str(include_str!(
        "../../../scenarios/tests/interactions/scenario.toml"
    ))
    .unwrap();
    let mut region: RegionDef = toml::from_str(include_str!(
        "../../../scenarios/tests/interactions/regions/1.toml"
    ))
    .unwrap();
    region.size[2] = 4;
    // No physical field: expiry leaves the observer's complete scene unchanged.
    region.gravity = None;
    let mut rules: tor_simulation::rules::RulesetDocument =
        toml::from_str(tor_simulation::rules::DEFAULT_RULESET_TOML).unwrap();
    rules
        .conditions
        .iter_mut()
        .find(|condition| condition.id.as_str() == "braced")
        .unwrap()
        .modifiers = vec![Modifier {
        cancellable: false,
        scope: Scope::Always,
        op: ModifierOp::Locomotion {
            value: tor_simulation::locomotion::LocomotionValue::Flying { enabled: true },
        },
    }];
    let suppression = rules
        .conditions
        .iter_mut()
        .find(|condition| condition.id.as_str() == "fear")
        .unwrap();
    suppression.modifiers = vec![Modifier {
        cancellable: false,
        scope: Scope::Always,
        op: ModifierOp::Locomotion {
            value: tor_simulation::locomotion::LocomotionValue::Flying { enabled: false },
        },
    }];
    suppression.restrictions.clear();
    suppression.preparation = tor_simulation::rules::ConditionPreparation::Preserve;
    let mut hover = rules
        .abilities
        .iter()
        .find(|ability| ability.id.as_str() == "brace_for_impact")
        .unwrap()
        .clone();
    hover.id = tor_simulation::content_ids::AbilityId::new("fixture_hover").unwrap();
    hover.name = "Fixture Hover".into();
    // Suppressing a movement capability is direct harm even when this
    // self-only fixture deliberately applies it to its own actor.
    if suppressed {
        hover.interaction = tor_simulation::rules::Interaction::HostileToTarget;
    }
    hover.reach = tor_simulation::rules::Reach::SelfOnly;
    hover.cost = None;
    hover.supply_costs.clear();
    hover.timing = Timing {
        source: TimingSource::Fixed {
            prepare: 1,
            recover: 1,
        },
        scaled_by: None,
    };
    let effects = if suppressed {
        json!([{"kind":"apply_condition","condition":"braced","duration":400},
            {"kind":"apply_condition","condition":"fear","duration":50}])
    } else {
        json!([{"kind":"apply_condition","condition":"braced","duration":50}])
    };
    hover.strikes = serde_json::from_value(json!({"kind":"authored","strikes":[{
        "offset":0, "unlock":null, "check":null,
        "on_success":effects,
        "on_failure":[], "descriptors":[], "riders":[]
    }]}))
    .unwrap();
    rules.constants.default_abilities.push(hover.id.clone());
    rules.abilities.push(hover);
    let package =
        Package::from_parts(manifest, vec![region], &toml::to_string(&rules).unwrap()).unwrap();
    crate::Scenario {
        actors: vec![],
        regions: 1,
        package: Some(Arc::new(package)),
        ..crate::Scenario::two_room(42)
    }
}

#[test]
fn wait_expiring_flight_refreshes_disclosed_vertical_route_with_unchanged_scene() {
    assert_wait_refresh(false);
}

#[test]
fn wait_expiring_flight_suppression_learns_vertical_route_with_unchanged_scene() {
    assert_wait_refresh(true);
}

fn assert_wait_refresh(suppressed: bool) {
    let actor = ActorId(1);
    let mut engine = Engine::memory(scenario(suppressed)).unwrap();
    engine
        .command(
            "player",
            "test",
            actor,
            "hover",
            &engine.branch().clone(),
            crate::journal::Command::Act {
                expected_boundary: engine.causal_boundary(actor).unwrap(),
                action: crate::journal::Action::UseAbility {
                    id: tor_simulation::content_ids::AbilityId::new("fixture_hover").unwrap(),
                    invocation: crate::journal::AbilityInvocation {
                        aim: crate::journal::AbilityAim::ActorTarget {
                            target: tor_simulation::ActorId(1),
                        },
                        weapon: crate::journal::WeaponSelection::Default,
                    },
                    supplies: Default::default(),
                },
            },
        )
        .unwrap();
    let before = engine.state(actor).unwrap();
    let destination = before
        .observation
        .visible_cells
        .iter()
        .find(|cell| cell.position == Position { x: 0, y: 0, z: 1 })
        .unwrap()
        .key
        .clone();
    if suppressed {
        assert!(engine.travel_route(actor, &destination).is_err());
    } else {
        assert_eq!(engine.travel_route(actor, &destination).unwrap().len(), 1);
    }
    let mut service = Service::new(engine);
    let mut client = service
        .connect(
            &Account {
                user: "player".into(),
                token: "test-only".into(),
                role: AccessRole::Player,
                actors: [actor].into(),
            },
            "headless".into(),
        )
        .unwrap();
    service.handle(client.id, "attach".into(), Request::Attach { actor });
    service.handle(client.id, "control".into(), Request::AcquireControl);
    while client.messages.try_recv().is_ok() {}
    service.handle(
        client.id,
        "wait".into(),
        Request::Command {
            context: service.input_context(client.id).unwrap(),
            branch: service.engine.branch().clone(),
            command: Command::Act {
                action: Action::Wait,
            },
        },
    );
    service.run_until_blocked();
    let messages: Vec<_> = std::iter::from_fn(|| client.messages.try_recv().ok()).collect();
    assert!(!messages
        .iter()
        .any(|message| matches!(message, ServerMessage::Error { .. })));
    let after = service.engine.state(actor).unwrap();
    assert!(after.observation.tick > before.observation.tick);
    assert_eq!(
        after.observation.visible_cells,
        before.observation.visible_cells
    );
    if suppressed {
        assert_eq!(
            service
                .engine
                .travel_route(actor, &destination)
                .unwrap()
                .len(),
            1,
            "expiry must learn newly disclosed flight edges even without motion"
        );
    } else {
        assert!(
            service.engine.travel_route(actor, &destination).is_err(),
            "expired flight must not leave an executable vertical route"
        );
    }
}
