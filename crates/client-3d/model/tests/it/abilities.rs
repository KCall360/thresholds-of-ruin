//! Granted abilities chosen and aimed from the server's disclosed catalog.
use tor_client_3d_model::{request_from, view, AbilityTarget, Cmd};
use tor_client_common::{test_support, ClientState, Palette};
use tor_protocol::*;

/// A valid catalog entry: each target kind has the reach the server pairs it with.
fn ability(id: &str, target: AbilityTargetKind) -> AbilityView {
    let reach = match target {
        AbilityTargetKind::SelfOnly | AbilityTargetKind::SelfStep => AbilityReach::SelfOnly,
        AbilityTargetKind::DirectionalProjectile => AbilityReach::Projectile,
        AbilityTargetKind::DirectionalRay => AbilityReach::UntilObstruction,
        _ => AbilityReach::Sight { range: 6 },
    };
    AbilityView {
        target_kind: target,
        reach,
        weapon_selection: false,
        ..test_support::ability(id)
    }
}

fn state() -> ClientState {
    let mut observed = test_support::observation();
    let stats = observed
        .combat
        .as_mut()
        .unwrap()
        .own_stats
        .as_mut()
        .unwrap();
    stats.abilities.extend([
        ability("magic_missile", AbilityTargetKind::Actor),
        ability("frost_ray", AbilityTargetKind::DirectionalRay),
        ability("lunge", AbilityTargetKind::SelfStep),
        ability("heal_self", AbilityTargetKind::SelfOnly),
        ability("arrow", AbilityTargetKind::DirectionalProjectile),
    ]);
    let snapshot: Snapshot = serde_json::from_value(serde_json::json!({
        "readiness": {"revision": "0", "admission": true, "resume": [], "cancel": []},
        "context": StreamContext { stream: StreamId("abilities".into()), epoch: 0 },
        "actor": observed.actor, "branch": "play", "cursor": {"sequence": "0", "tick": "0"},
        "has_control": true, "intentions": [], "history": {"entries": [], "older_before": null},
        "state": {"wizard_game": false, "revision": "0", "observation": observed},
    }))
    .unwrap();
    ClientState::from_snapshot(snapshot).unwrap()
}

fn action(state: &ClientState, command: Cmd) -> Result<Action, String> {
    match request_from(state, &state.reply_context(), command)? {
        Request::Command {
            command: Command::Act { action, .. },
            ..
        } => Ok(action),
        other => panic!("expected an action, got {other:?}"),
    }
}

fn used(id: &str, aim: AbilityAim) -> Action {
    Action::UseAbility {
        id: id.into(),
        invocation: AbilityInvocation {
            aim,
            weapon: WeaponSelection::Default,
        },
        supplies: SupplySelection::Carried,
    }
}

#[test]
fn granted_abilities_are_listed_without_the_basic_attack_or_projectiles() {
    let projected = view(&state(), &Palette::default());
    let listed: Vec<_> = projected.abilities.iter().map(|a| a.id.as_str()).collect();
    assert_eq!(listed, ["magic_missile", "frost_ray", "lunge", "heal_self"]);
    assert_eq!(
        projected.abilities[1].target,
        AbilityTargetKind::DirectionalRay
    );
}

#[test]
fn abilities_are_aimed_as_their_target_kind_requires() {
    let state = state();
    let foe = ActorTarget::from_digest([2; 32]);
    let own = ActorTarget::from_digest([1; 32]);
    assert_eq!(
        action(
            &state,
            Cmd::Ability("magic_missile".into(), AbilityTarget::Actor(foe))
        ),
        Ok(used(
            "magic_missile",
            AbilityAim::ActorTarget { target: foe }
        ))
    );
    assert_eq!(
        action(
            &state,
            Cmd::Ability(
                "frost_ray".into(),
                AbilityTarget::Direction(Direction::West)
            )
        ),
        Ok(used(
            "frost_ray",
            AbilityAim::DirectionalRay {
                direction: Direction::West
            }
        ))
    );
    assert_eq!(
        action(
            &state,
            Cmd::Ability("lunge".into(), AbilityTarget::Direction(Direction::South))
        ),
        Ok(used(
            "lunge",
            AbilityAim::SelfStep {
                direction: Direction::South
            }
        ))
    );
    assert_eq!(
        action(
            &state,
            Cmd::Ability("heal_self".into(), AbilityTarget::Actor(own))
        ),
        Ok(used("heal_self", AbilityAim::ActorTarget { target: own }))
    );
    for refused in [
        Cmd::Ability(
            "magic_missile".into(),
            AbilityTarget::Direction(Direction::North),
        ),
        Cmd::Ability("frost_ray".into(), AbilityTarget::Actor(foe)),
        Cmd::Ability("heal_self".into(), AbilityTarget::Actor(foe)),
        Cmd::Ability("unknown".into(), AbilityTarget::Actor(foe)),
    ] {
        assert!(action(&state, refused).is_err());
    }
}

#[test]
fn only_a_disclosed_hold_can_be_escaped_and_it_names_its_holder() {
    let mut observed = test_support::observation();
    let hold = HoldTarget::from_digest([21; 32]);
    observed.linked_holds = vec![LinkedHold {
        target: hold,
        holder: Some(ActorTarget::from_digest([2; 32])),
    }];
    let snapshot: Snapshot = serde_json::from_value(serde_json::json!({
        "readiness": {"revision": "0", "admission": true, "resume": [], "cancel": []},
        "context": StreamContext { stream: StreamId("holds".into()), epoch: 0 },
        "actor": observed.actor, "branch": "play", "cursor": {"sequence": "0", "tick": "0"},
        "has_control": true, "intentions": [], "history": {"entries": [], "older_before": null},
        "state": {"wizard_game": false, "revision": "0", "observation": observed},
    }))
    .unwrap();
    let state = ClientState::from_snapshot(snapshot).unwrap();
    let projected = view(&state, &Palette::default());
    assert_eq!(projected.holds.len(), 1);
    assert_eq!(projected.holds[0].label, "sentinel");
    assert_eq!(
        action(&state, Cmd::Escape(hold)),
        Ok(Action::Escape { hold })
    );
    assert!(action(&state, Cmd::Escape(HoldTarget::from_digest([22; 32]))).is_err());
}

#[test]
fn examining_a_trap_sends_only_for_an_unknown_one_within_reach() {
    let mut observed = test_support::observation();
    let near = TrapTarget::from_digest([41; 32]);
    let known = TrapTarget::from_digest([42; 32]);
    let cell = |x: i32, id, family| CellView {
        object_presentation: None,
        lit: true,
        key: format!("cell-{x}"),
        position: Position { x, y: 0, z: 0 },
        wall: false,
        material: String::new(),
        trap: Some(TrapView {
            id,
            family,
            reachable: true,
        }),
        door: None,
        asset: None,
        stairs_up: false,
        stairs_down: false,
        place_hint: false,
    };
    observed.visible_cells = vec![cell(0, near, None), cell(1, known, Some(TrapFamily::Arrow))];
    let snapshot: Snapshot = serde_json::from_value(serde_json::json!({
        "readiness": {"revision": "0", "admission": true, "resume": [], "cancel": []},
        "context": StreamContext { stream: StreamId("traps".into()), epoch: 0 },
        "actor": observed.actor, "branch": "play", "cursor": {"sequence": "0", "tick": "0"},
        "has_control": true, "intentions": [], "history": {"entries": [], "older_before": null},
        "state": {"wizard_game": false, "revision": "0", "observation": observed},
    }))
    .unwrap();
    let state = ClientState::from_snapshot(snapshot).unwrap();
    assert_eq!(
        action(&state, Cmd::ExamineTrap(near)),
        Ok(Action::ExamineTrap { trap: near })
    );
    assert!(
        action(&state, Cmd::ExamineTrap(known)).is_err(),
        "already known"
    );
    assert!(action(&state, Cmd::ExamineTrap(TrapTarget::from_digest([43; 32]))).is_err());
}
