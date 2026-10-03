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
    service.run_until_blocked();
    drain(&mut client);
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
    service.run_until_blocked();
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
fn spectators_cannot_start_or_retry_travel() {
    let (mut service, mut client) = fixture();
    let mut spectator = connect(&mut service, AccessRole::Spectator);
    let request = start(&mut service, &mut client, 7);
    service.handle(spectator.id, "travel".into(), request);
    assert!(drain(&mut spectator).iter().any(|m| matches!(
        m,
        ServerMessage::Error {
            code: ErrorCode::Unauthorized,
            ..
        }
    )));
    service.run_until_blocked();
    assert_eq!(
        service.travel_status[&ActorId(1)].phase,
        TravelPhase::Arrived
    );
}

#[test]
fn commands_during_a_journey_are_busy_and_leave_it_running() {
    let (mut service, mut client) = fixture();
    let request = start(&mut service, &mut client, 7);
    let travel_id = service.travel_status[&ActorId(1)].id.clone();
    let Request::Command { branch, command } = request else {
        unreachable!()
    };
    let Command::Travel { destination, .. } = command else {
        unreachable!()
    };
    let revision = service.engine.revision(ActorId(1)).unwrap();
    for (id, command) in [
        (
            "act",
            Command::Act {
                expected_revision: revision,
                action: Action::Wait,
            },
        ),
        (
            "replacement",
            Command::Travel {
                expected_revision: revision,
                destination,
            },
        ),
    ] {
        service.handle(
            client.id,
            id.into(),
            Request::Command {
                branch: branch.clone(),
                command,
            },
        );
        assert!(drain(&mut client).iter().any(|m| matches!(
            m,
            ServerMessage::Error {
                code: ErrorCode::ActorBusy,
                ..
            }
        )));
    }
    let status = &service.travel_status[&ActorId(1)];
    assert_eq!(
        (status.phase, &status.id),
        (TravelPhase::Active, &travel_id)
    );
    assert_eq!(service.engine.revision(ActorId(1)).unwrap(), revision);
    service.run_until_blocked();
    assert_eq!(
        service.travel_status[&ActorId(1)].phase,
        TravelPhase::Arrived
    );
}

#[test]
fn saving_during_a_journey_does_not_stop_it() {
    let (mut service, mut client) = fixture();
    start(&mut service, &mut client, 7);
    service.handle(client.id, "save".into(), Request::Save);
    service.poll_saves();
    assert!(drain(&mut client)
        .iter()
        .any(|m| matches!(m, ServerMessage::Ack { request_id, .. } if request_id == "save")));
    assert_eq!(
        service.travel_status[&ActorId(1)].phase,
        TravelPhase::Active
    );
    service.run_until_blocked();
    assert_eq!(service.travel_status[&ActorId(1)].completed_steps, 7);
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
        service.run_until_blocked();
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
    service.run_until_blocked();
    drain(&mut client);
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
    service.run_until_blocked();
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
    service.run_until_blocked();
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
            asset: None,
            id: ActorId(1),
            position: Position { x: 1, y: 0, z: 0 },
        },
        ActorView {
            name: "figure".into(),
            description: "A figure.".into(),
            asset: None,
            id: ActorId(2),
            position: Position { x: 2, y: 0, z: 0 },
        },
        ActorView {
            name: "figure".into(),
            description: "A figure.".into(),
            asset: None,
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
fn blocked_step_is_free() {
    let (mut service, mut client) = fixture();
    setup(&mut service, &mut client, "actor 100 3 1 0 0");
    start(&mut service, &mut client, 5);
    service.run_until_blocked();
    assert_eq!(
        service.travel_status[&ActorId(1)].phase,
        TravelPhase::Blocked
    );
    assert_eq!(service.engine.observation(ActorId(1)).unwrap().tick, 0);
    assert!(service.travels.is_empty());
}

/// Connect a second player controlling a new actor at `x` in the corridor.
fn second_player(service: &mut Service, client: &mut Connection, x: i32) -> Connection {
    setup(service, client, &format!("actor 100 3 {x} 0 0"));
    let mut other = service
        .connect(
            &Account {
                user: "other-user".into(),
                token: "other".into(),
                role: AccessRole::Player,
                actors: BTreeSet::from([ActorId(2)]),
            },
            "test".into(),
        )
        .unwrap();
    service.handle(
        other.id,
        "attach".into(),
        Request::Attach { actor: ActorId(2) },
    );
    service.handle(other.id, "control".into(), Request::AcquireControl);
    drain(&mut other);
    other
}

fn act(service: &mut Service, client: &mut Connection, actor: ActorId, action: Action) {
    service.handle(
        client.id,
        uuid::Uuid::new_v4().to_string(),
        Request::Command {
            branch: service.engine.branch().clone(),
            command: Command::Act {
                expected_revision: service.engine.revision(actor).unwrap(),
                action,
            },
        },
    );
    let messages = drain(client);
    assert!(
        !messages
            .iter()
            .any(|m| matches!(m, ServerMessage::Error { .. })),
        "{messages:?}"
    );
}

#[test]
fn a_journey_waits_while_another_player_acts_and_then_continues() {
    let (mut service, mut client) = fixture();
    let mut other = second_player(&mut service, &mut client, 7);
    start(&mut service, &mut client, 5);
    service.run_until_blocked();
    let status = &service.travel_status[&ActorId(1)];
    assert_eq!(
        (status.phase, status.completed_steps),
        (TravelPhase::Active, 1)
    );
    assert_eq!(service.engine.next_actor(), Some(ActorId(2)));
    assert!(matches!(service.step(), Step::Blocked));
    while service.travel_status[&ActorId(1)].phase == TravelPhase::Active {
        act(&mut service, &mut other, ActorId(2), Action::Wait);
        service.run_until_blocked();
    }
    let status = &service.travel_status[&ActorId(1)];
    assert_eq!(
        (status.phase, status.completed_steps),
        (TravelPhase::Arrived, 5)
    );
}

#[test]
fn another_players_action_can_interrupt_a_waiting_journey() {
    let (mut service, mut client) = fixture();
    setup(&mut service, &mut client, "door 3 6 0 0 closed");
    let mut other = second_player(&mut service, &mut client, 7);
    assert!(service
        .engine
        .observation(ActorId(1))
        .unwrap()
        .visible_actors
        .is_empty());
    start(&mut service, &mut client, 4);
    service.run_until_blocked();
    assert_eq!(service.travel_status[&ActorId(1)].completed_steps, 1);
    assert_eq!(service.engine.next_actor(), Some(ActorId(2)));
    let door = service
        .engine
        .observation(ActorId(2))
        .unwrap()
        .visible_cells
        .iter()
        .find_map(|cell| cell.door.as_ref().map(|door| door.id))
        .unwrap();
    act(
        &mut service,
        &mut other,
        ActorId(2),
        Action::SetDoor { door, open: true },
    );
    service.run_until_blocked();
    let status = &service.travel_status[&ActorId(1)];
    assert_eq!(
        (status.phase, status.completed_steps),
        (TravelPhase::Hazard, 1)
    );
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
fn slow_controller_during_start_cannot_resurrect_travel_on_observers() {
    let (mut service, mut controller) = fixture();
    let mut observer = connect(&mut service, AccessRole::Spectator);
    for i in 0..QUEUE {
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

#[test]
fn slow_spectator_pauses_the_journey_until_dropped_and_reconnects_at_committed_state() {
    let (mut service, mut controller) = fixture();
    let mut slow = connect(&mut service, AccessRole::Spectator);
    start(&mut service, &mut controller, 7);
    drain(&mut slow);
    service.step();
    for i in 0..QUEUE - HEADROOM {
        service.handle(slow.id, format!("fill-{i}"), Request::Snapshot);
    }
    drain(&mut controller);
    let Step::Full(full) = service.step() else {
        panic!("the journey must wait for the spectator");
    };
    assert_eq!(
        full.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
        [slow.id]
    );
    assert_eq!(service.travel_status[&ActorId(1)].completed_steps, 1);
    // The runner disconnects a client that stays full; the journey goes on.
    service.disconnect(slow.id);
    assert!(*slow.close.borrow());
    assert_eq!(
        service.travel_status[&ActorId(1)].phase,
        TravelPhase::Active
    );
    let mut replacement = connect(&mut service, AccessRole::Spectator);
    service.handle(replacement.id, "resync".into(), Request::Snapshot);
    let ServerMessage::Snapshot { snapshot, .. } = drain(&mut replacement).pop().unwrap() else {
        panic!("replacement snapshot");
    };
    assert_eq!(snapshot.state, service.engine.state(ActorId(1)).unwrap());
    assert_eq!(snapshot.travel.unwrap().completed_steps, 1);
    assert!(!snapshot.has_control);
    service.run_until_blocked();
    assert_eq!(
        service.travel_status[&ActorId(1)].phase,
        TravelPhase::Arrived
    );
    assert_eq!(service.travel_status[&ActorId(1)].completed_steps, 7);
    let observations = drain(&mut replacement)
        .into_iter()
        .filter(|m| {
            matches!(m, ServerMessage::Update { update } if matches!(
                update.body,
                UpdateBody::Observation { .. } | UpdateBody::ObservationDelta { .. }
            ))
        })
        .count();
    assert_eq!(
        observations, 6,
        "every remaining step reaches the spectator"
    );
}

/// The runner, over its mailbox: a spectator that never reads is dropped
/// after the stall timeout, and the controller's journey finishes.
#[tokio::test]
async fn the_runner_drops_a_stalled_spectator_and_finishes_the_journey() {
    let (mut service, mut controller) = fixture();
    let slow = connect(&mut service, AccessRole::Spectator);
    start(&mut service, &mut controller, 7);
    for i in 0..QUEUE - HEADROOM {
        service.handle(slow.id, format!("fill-{i}"), Request::Snapshot);
    }
    let (mail, mailbox) = mpsc::channel(8);
    let runner = tokio::spawn(crate::runner::run(
        service,
        mailbox,
        std::time::Duration::from_millis(50),
    ));
    let finished = tokio::time::timeout(std::time::Duration::from_secs(30), async {
        while let Some(message) = controller.messages.recv().await {
            if let ServerMessage::Update { update } = message {
                if let UpdateBody::Travel { status, .. } = update.body {
                    if status.phase != TravelPhase::Active {
                        return status;
                    }
                }
            }
        }
        panic!("the controller was disconnected");
    })
    .await
    .expect("the journey must finish");
    assert_eq!(
        (finished.phase, finished.completed_steps),
        (TravelPhase::Arrived, 7)
    );
    assert!(*slow.close.borrow());
    let (reply, stopped) = tokio::sync::oneshot::channel();
    mail.send(crate::runner::Mail::Shutdown(reply))
        .await
        .unwrap();
    stopped.await.unwrap();
    runner.await.unwrap();
}

#[test]
fn a_stopped_run_tells_each_client_whose_move_it_is_once() {
    let (mut service, mut client) = fixture();
    let mut other = second_player(&mut service, &mut client, 7);
    let waiting = |c: &mut Connection| -> Vec<Waiting> {
        drain(c)
            .into_iter()
            .filter_map(|m| match m {
                ServerMessage::Waiting { on } => Some(on),
                _ => None,
            })
            .collect()
    };
    for _ in 0..3 {
        service.run_until_blocked();
        let next = service.engine.next_actor().unwrap();
        let (mine, theirs) = if next == ActorId(1) {
            (Waiting::You, Waiting::Others)
        } else {
            (Waiting::Others, Waiting::You)
        };
        assert_eq!(waiting(&mut client), [mine]);
        assert_eq!(waiting(&mut other), [theirs]);
        // Still stopped: nothing new to say.
        assert!(matches!(service.step(), Step::Blocked));
        assert!(waiting(&mut client).is_empty());
        let mover = if next == ActorId(1) {
            &mut client
        } else {
            &mut other
        };
        act(&mut service, mover, next, Action::Wait);
    }
}
