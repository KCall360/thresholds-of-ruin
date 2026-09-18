use super::*;
use crate::Scenario;

fn drain(client: &mut Connection) -> Vec<ServerMessage> {
    let mut messages = Vec::new();
    while let Ok(message) = client.messages.try_recv() {
        messages.push(message);
    }
    messages
}
fn connect(service: &mut Service, role: AccessRole) -> Connection {
    let mut client = service
        .connect(
            &Account {
                user: "same-user".into(),
                token: "test".into(),
                role,
                actors: BTreeSet::from([ActorId(1)]),
            },
            "test".into(),
        )
        .unwrap();
    service.handle(
        client.id,
        "attach".into(),
        Request::Attach { actor: ActorId(1) },
    );
    if role != AccessRole::Spectator {
        service.handle(client.id, "control".into(), Request::AcquireControl);
    }
    drain(&mut client);
    client
}
fn setup(service: &mut Service, client: &mut Connection, operation: &str) {
    service.handle(
        client.id,
        uuid::Uuid::new_v4().to_string(),
        Request::Command {
            branch: service.engine.branch().clone(),
            command: Command::Wizard {
                expected_revision: service.engine.revision(ActorId(1)).unwrap(),
                operation: operation.into(),
            },
        },
    );
    assert!(!drain(client)
        .iter()
        .any(|m| matches!(m, ServerMessage::Error { .. })));
}
fn fixture() -> (Service, Connection) {
    let mut engine = Engine::memory(Scenario::two_room(42)).unwrap();
    engine.enable_wizard().unwrap();
    let mut service = Service::new(engine);
    let mut client = connect(&mut service, AccessRole::Wizard);
    setup(&mut service, &mut client, "room 3 8 1 1 Corridor");
    setup(&mut service, &mut client, "teleport 1 3 0 0 0");
    (service, client)
}
fn start(service: &mut Service, client: &mut Connection, x: i32) -> Request {
    let state = service.engine.state(ActorId(1)).unwrap();
    let destination = state
        .observation
        .visible_cells
        .iter()
        .find(|c| c.position == Position { x, y: 0, z: 0 })
        .unwrap()
        .key
        .clone();
    let request = Request::Command {
        branch: service.engine.branch().clone(),
        command: Command::Travel {
            expected_revision: state.revision,
            destination,
        },
    };
    service.handle(client.id, "travel".into(), request.clone());
    assert!(!drain(client)
        .iter()
        .any(|m| matches!(m, ServerMessage::Error { .. })));
    request
}

#[test]
fn travel_steps_are_saved_ordinary_actions_and_retry_does_not_restart() {
    let (mut service, mut client) = fixture();
    let request = start(&mut service, &mut client, 5);
    assert_eq!(service.engine.observation(ActorId(1)).unwrap().tick, 0);
    for _ in 0..5 {
        service.advance_travel();
        drain(&mut client);
    }
    let status = &service.travel_status[&ActorId(1)];
    assert_eq!(status.phase, TravelPhase::Arrived);
    assert_eq!(status.completed_steps, 5);
    assert_eq!(service.engine.observation(ActorId(1)).unwrap().tick, 500);
    service.handle(client.id, "travel".into(), request);
    assert!(matches!(
        drain(&mut client).last(),
        Some(ServerMessage::Ack { .. })
    ));
    assert!(service.travels.is_empty());
    service.advance_travel();
    assert_eq!(service.engine.observation(ActorId(1)).unwrap().tick, 500);
    let history = service
        .engine
        .history(ActorId(1), "same-user", None, 100)
        .unwrap();
    assert_eq!(
        history
            .entries
            .iter()
            .filter(|e| matches!(e.content, HistoryContent::Action { .. }))
            .count(),
        5
    );
}

#[test]
fn cancellation_stops_at_boundary_and_spectator_cannot_cancel_or_retry() {
    let (mut service, mut client) = fixture();
    let mut spectator = connect(&mut service, AccessRole::Spectator);
    let request = start(&mut service, &mut client, 7);
    service.advance_travel();
    drain(&mut client);
    let cancel = Request::CancelTravel {
        branch: service.engine.branch().clone(),
        travel_id: service.travel_status[&ActorId(1)].id.clone(),
    };
    for (id, request) in [("travel", request), ("cancel", cancel.clone())] {
        service.handle(spectator.id, id.into(), request);
        assert!(drain(&mut spectator).iter().any(|m| matches!(
            m,
            ServerMessage::Error {
                code: ErrorCode::Unauthorized,
                ..
            }
        )));
    }
    service.handle(client.id, "cancel".into(), cancel.clone());
    drain(&mut client);
    service.advance_travel();
    assert_eq!(
        service.travel_status[&ActorId(1)].phase,
        TravelPhase::Cancelled
    );
    assert_eq!(service.engine.observation(ActorId(1)).unwrap().tick, 100);
    service.handle(client.id, "cancel-again".into(), cancel);
    assert!(matches!(
        drain(&mut client).last(),
        Some(ServerMessage::Ack { .. })
    ));
}

#[test]
fn control_loss_and_wizard_changes_stop_jobs() {
    for mode in ["release", "disconnect", "setup", "rewind"] {
        let (mut service, mut client) = fixture();
        start(&mut service, &mut client, 7);
        match mode {
            "release" => service.handle(client.id, "release".into(), Request::ReleaseControl),
            "disconnect" => service.disconnect(client.id),
            "setup" => setup(&mut service, &mut client, "place 3 1 0 0 on"),
            _ => setup(&mut service, &mut client, "rewind initial"),
        }
        assert!(service.travels.is_empty());
        service.advance_travel();
        assert_eq!(service.engine.observation(ActorId(1)).unwrap().tick, 0);
    }
}

#[test]
fn harmless_discoveries_do_not_interrupt_travel() {
    let (mut service, mut client) = fixture();
    setup(&mut service, &mut client, "room 4 20 1 1 Long corridor");
    setup(&mut service, &mut client, "item tablet 4 9 0 0");
    setup(&mut service, &mut client, "place 4 10 0 0 on");
    setup(&mut service, &mut client, "teleport 1 4 0 0 0");
    let before = service.engine.observation(ActorId(1)).unwrap();
    assert!(before.ground_items.is_empty());
    assert!(!before.visible_cells.iter().any(|cell| cell.place_hint));
    start(&mut service, &mut client, 7);
    for _ in 0..7 {
        service.advance_travel();
        drain(&mut client);
    }
    assert_eq!(
        service.travel_status[&ActorId(1)].phase,
        TravelPhase::Arrived
    );
    assert_eq!(service.travel_status[&ActorId(1)].completed_steps, 7);
    let after = service.engine.observation(ActorId(1)).unwrap();
    assert_eq!(after.tick, 700);
    assert!(!after.ground_items.is_empty());
    assert!(after.visible_cells.iter().any(|cell| cell.place_hint));
    assert!(after
        .visible_cells
        .iter()
        .any(|cell| !before.visible_cells.iter().any(|old| old.key == cell.key)));
}

#[test]
fn a_newly_seen_other_actor_interrupts_before_another_step() {
    let (mut service, mut client) = fixture();
    setup(&mut service, &mut client, "room 4 20 1 1 Long corridor");
    setup(&mut service, &mut client, "actor 100 4 9 0 0");
    setup(&mut service, &mut client, "teleport 1 4 0 0 0");
    assert!(service
        .engine
        .observation(ActorId(1))
        .unwrap()
        .visible_actors
        .is_empty());
    start(&mut service, &mut client, 7);
    service.advance_travel();
    assert_eq!(
        service.travel_status[&ActorId(1)].phase,
        TravelPhase::Hazard
    );
    assert_eq!(service.travel_status[&ActorId(1)].completed_steps, 1);
    let stopped = service.engine.state(ActorId(1)).unwrap();
    assert!(stopped
        .observation
        .visible_actors
        .iter()
        .any(|actor| actor.id == ActorId(2)));
    service.advance_travel();
    assert_eq!(service.engine.state(ActorId(1)).unwrap(), stopped);
}

#[test]
fn repeated_views_of_self_are_not_potential_hazards() {
    let (service, _) = fixture();
    let mut observation = service.engine.observation(ActorId(1)).unwrap();
    observation.visible_actors = vec![
        ActorView {
            name: "figure".into(),
            description: "A figure.".into(),
            id: ActorId(1),
            position: Position { x: 1, y: 0, z: 0 },
        },
        ActorView {
            name: "figure".into(),
            description: "A figure.".into(),
            id: ActorId(2),
            position: Position { x: 2, y: 0, z: 0 },
        },
        ActorView {
            name: "figure".into(),
            description: "A figure.".into(),
            id: ActorId(2),
            position: Position { x: 3, y: 0, z: 0 },
        },
    ];
    assert_eq!(
        potential_hazards(&observation),
        BTreeSet::from([ActorId(2)])
    );
}

#[test]
fn blocked_step_is_free_and_other_actor_decision_interrupts() {
    for position in [1, 6] {
        let (mut service, mut client) = fixture();
        setup(
            &mut service,
            &mut client,
            &format!("actor 100 3 {position} 0 0"),
        );
        start(&mut service, &mut client, 5);
        service.advance_travel();
        assert_eq!(
            service.travel_status[&ActorId(1)].phase,
            if position == 1 {
                TravelPhase::Blocked
            } else {
                TravelPhase::DecisionRequired
            }
        );
        assert!(service.travels.is_empty());
    }
}

#[test]
fn stale_unknown_and_wrong_branch_requests_are_atomic() {
    let (mut service, mut client) = fixture();
    let before = service.engine.state(ActorId(1)).unwrap();
    let known = before.observation.visible_cells[0].key.clone();
    for (branch, revision, destination, code) in [
        (
            service.engine.branch().clone(),
            before.revision,
            "unknown".into(),
            ErrorCode::InvalidAction,
        ),
        (
            service.engine.branch().clone(),
            before.revision - 1,
            known.clone(),
            ErrorCode::StaleRevision,
        ),
        (
            BranchId("wrong".into()),
            before.revision,
            known,
            ErrorCode::WrongBranch,
        ),
    ] {
        service.handle(
            client.id,
            "invalid".into(),
            Request::Command {
                branch,
                command: Command::Travel {
                    expected_revision: revision,
                    destination,
                },
            },
        );
        assert!(drain(&mut client).iter().any(
            |m| matches!(m, ServerMessage::Error { code: received, .. } if *received == code)
        ));
        assert_eq!(service.engine.state(ActorId(1)).unwrap(), before);
        assert!(service.travels.is_empty());
    }
}

#[test]
fn active_rewind_publishes_snapshot_before_any_new_branch_update() {
    let (mut service, mut client) = fixture();
    let mut spectator = connect(&mut service, AccessRole::Spectator);
    start(&mut service, &mut client, 7);
    drain(&mut spectator);
    setup(&mut service, &mut client, "rewind initial");
    let messages = drain(&mut spectator);
    assert!(
        matches!(messages.first(), Some(ServerMessage::Snapshot { snapshot, .. }) if snapshot.travel.is_none())
    );
    assert!(service.travels.is_empty());
}

#[test]
fn an_old_cancel_cannot_stop_a_replacement_trip() {
    let (mut service, mut client) = fixture();
    let request = start(&mut service, &mut client, 7);
    let old_id = service.travel_status[&ActorId(1)].id.clone();
    service.handle(client.id, "replacement".into(), request);
    drain(&mut client);
    let new_id = service.travel_status[&ActorId(1)].id.clone();
    assert_ne!(new_id, old_id);
    service.handle(
        client.id,
        "late-cancel".into(),
        Request::CancelTravel {
            branch: service.engine.branch().clone(),
            travel_id: old_id,
        },
    );
    assert!(matches!(
        drain(&mut client).last(),
        Some(ServerMessage::Error { .. })
    ));
    assert_eq!(
        service.travel_status[&ActorId(1)].phase,
        TravelPhase::Active
    );
    assert_eq!(service.travel_status[&ActorId(1)].id, new_id);
}

#[test]
fn slow_controller_during_start_cannot_resurrect_travel_on_observers() {
    let (mut service, mut controller) = fixture();
    let mut observer = connect(&mut service, AccessRole::Spectator);
    for i in 0..64 {
        service.handle(controller.id, format!("fill-{i}"), Request::Snapshot);
    }
    start(&mut service, &mut controller, 7);
    assert!(*controller.close.borrow());
    let statuses: Vec<_> = drain(&mut observer)
        .into_iter()
        .filter_map(|m| match m {
            ServerMessage::Update { update } => match update.body {
                UpdateBody::Travel { status, .. } => Some(status),
                _ => None,
            },
            _ => None,
        })
        .collect();
    assert!(!statuses.is_empty());
    assert!(statuses.iter().all(|s| s.phase == TravelPhase::ControlLost));
    assert!(service.travels.is_empty());
}
