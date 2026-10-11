use super::*;
use crate::Scenario;
use tokio::sync::mpsc;

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
            context: service.input_context(client.id).unwrap(),
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
        context: service.input_context(client.id).unwrap(),
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
fn travel_admits_a_step_before_simulation_advances_journey_progress() {
    let (mut service, mut client) = fixture();
    start(&mut service, &mut client, 5);
    let actor = ActorId(1);
    let before = service.engine.state(actor).unwrap();
    let journey = service.travel_status[&actor].id.clone();

    assert!(matches!(service.step(), Step::Progress));
    assert_eq!(service.engine.state(actor).unwrap(), before);
    assert_eq!(service.travel_status[&actor].completed_steps, 0);
    assert_eq!(service.travel_status[&actor].id, journey);
    assert_eq!(service.engine.next_intention_actor(), Some(actor));

    assert!(matches!(service.step(), Step::Progress));
    assert_eq!(service.engine.observation(actor).unwrap().tick, 100);
    assert_eq!(service.travel_status[&actor].completed_steps, 1);
    assert_eq!(service.travel_status[&actor].id, journey);
    assert_eq!(service.travel_status[&actor].phase, TravelPhase::Active);
    assert_eq!(service.engine.next_intention_actor(), None);
    assert!(!drain(&mut client)
        .iter()
        .any(|message| matches!(message, ServerMessage::Error { .. })));
}

#[test]
fn releasing_control_between_travel_admission_and_execution_prevents_movement() {
    let (mut service, mut client) = fixture();
    start(&mut service, &mut client, 5);
    let actor = ActorId(1);
    let before = service.engine.state(actor).unwrap();

    assert!(matches!(service.step(), Step::Progress));
    service.handle(
        client.id,
        "release-after-admission".into(),
        Request::ReleaseControl,
    );
    drain(&mut client);
    service.run_until_blocked();

    assert_eq!(service.engine.state(actor).unwrap(), before);
    assert_eq!(service.travel_status[&actor].completed_steps, 0);
    assert_eq!(
        service.travel_status[&actor].phase,
        TravelPhase::ControlLost
    );
    assert_eq!(service.engine.next_intention_actor(), None);
    assert!(!service.travels.contains_key(&actor));

    service.handle(client.id, "reacquire".into(), Request::AcquireControl);
    drain(&mut client);
    service.run_until_blocked();
    assert_eq!(service.engine.state(actor).unwrap(), before);
    assert_eq!(
        service.travel_status[&actor].phase,
        TravelPhase::ControlLost
    );
}

#[test]
fn travel_execution_is_authored_by_scheduler_instead_of_synthetic_client_requests() {
    let (mut service, mut client) = fixture();
    start(&mut service, &mut client, 5);
    service.run_until_blocked();
    let history = service
        .engine
        .history(ActorId(1), "same-user", None, 100)
        .unwrap();
    let actions: Vec<_> = history
        .entries
        .iter()
        .filter(|entry| matches!(entry.content, HistoryContent::Action { .. }))
        .collect();
    assert_eq!(actions.len(), 5);
    assert!(actions.iter().all(|entry| entry.author
        == Author::Backend {
            component: "scheduler".into(),
        }));
    assert_eq!(service.travel_status[&ActorId(1)].completed_steps, 5);
    assert_eq!(
        service.travel_status[&ActorId(1)].phase,
        TravelPhase::Arrived
    );
}

#[test]
fn restarted_service_settles_pending_travel_without_resuming_the_journey() {
    let (mut service, mut client) = fixture();
    start(&mut service, &mut client, 5);
    let actor = ActorId(1);
    let journey = service.travel_status[&actor].id.clone();
    let step = service.travels[&actor].steps[0];
    let before = service.engine.state(actor).unwrap();
    service
        .engine
        .admit_travel(actor, &journey, 1, step)
        .unwrap();
    assert_eq!(service.engine.next_intention_actor(), Some(actor));
    let mut restarted = Service::new(service.engine);
    assert_eq!(restarted.engine.next_intention_actor(), None);
    assert_eq!(restarted.engine.state(actor).unwrap(), before);
    assert!(restarted.travels.is_empty());
    let mut reconnected = connect(&mut restarted, AccessRole::Wizard);
    restarted.run_until_blocked();
    drain(&mut reconnected);
    assert_eq!(restarted.engine.state(actor).unwrap(), before);
}

#[test]
fn mismatched_pending_travel_identity_cannot_execute_movement() {
    let (mut service, mut client) = fixture();
    start(&mut service, &mut client, 5);
    let actor = ActorId(1);
    let before = service.engine.state(actor).unwrap();
    assert!(matches!(service.step(), Step::Progress));
    service.travels.get_mut(&actor).unwrap().pending =
        Some(EntryId(uuid::Uuid::new_v4().to_string()));
    assert!(matches!(service.step(), Step::Progress));
    assert_eq!(service.engine.state(actor).unwrap(), before);
    assert_eq!(service.travel_status[&actor].completed_steps, 0);
    assert_eq!(service.travel_status[&actor].phase, TravelPhase::Failed);
    assert_eq!(service.engine.next_intention_actor(), None);
}

#[test]
fn startup_cancellation_storage_failure_preserves_saved_travel_for_retry() {
    for interval in [1, 4096] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("pending-travel.db");
        let scenario = Scenario::two_room(42);
        let policy = crate::SavePolicy {
            checkpoint_interval: interval,
            ..Default::default()
        };
        let actor = ActorId(1);
        let mut engine = Engine::open_with_policy(&path, scenario.clone(), policy.clone()).unwrap();
        let before = engine.state(actor).unwrap();
        let destination = before
            .observation
            .visible_cells
            .iter()
            .find(|cell| cell.position == Position { x: 3, y: 1, z: 0 })
            .unwrap()
            .key
            .clone();
        let root = engine
            .command(
                "same-user",
                "test",
                actor,
                "journey",
                &engine.branch().clone(),
                crate::journal::Command::Travel {
                    expected_revision: before.revision,
                    destination: destination.clone(),
                },
            )
            .unwrap();
        let step = engine.travel_route(actor, &destination).unwrap()[0];
        let admission = engine.admit_travel(actor, &root.entry.id, 1, step).unwrap();
        engine.flush().unwrap();
        drop(engine);
        let rejected = Engine::open_with_policy(
            &path,
            scenario.clone(),
            crate::SavePolicy {
                max_pending_bytes: 1,
                ..policy.clone()
            },
        )
        .unwrap();
        let error = Service::with_outbound_limits(rejected, crate::OutboundLimits::default())
            .err()
            .expect("startup must fail while cancellation cannot be journaled");
        assert_eq!(error.code, ErrorCode::StorageFailure);
        let recovered = Engine::open_with_policy(&path, scenario.clone(), policy.clone()).unwrap();
        assert_eq!(
            recovered.queued_travel_admission(actor),
            Some(&admission.entry.id)
        );
        let mut service = Service::new(recovered);
        assert_eq!(service.engine.state(actor).unwrap(), before);
        assert_eq!(service.engine.next_intention_actor(), None);
        let mut client = connect(&mut service, AccessRole::Player);
        service.run_until_blocked();
        drain(&mut client);
        assert_eq!(service.engine.state(actor).unwrap(), before);
        service.engine.flush().unwrap();
        drop(service);
        let reopened = Engine::open_with_policy(&path, scenario, policy).unwrap();
        assert_eq!(reopened.state(actor).unwrap(), before);
        assert_eq!(reopened.next_intention_actor(), None);
    }
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
    let Request::Command {
        branch, command, ..
    } = request
    else {
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
                context: service.input_context(client.id).unwrap(),
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
    for admitted in [false, true] {
        for mode in ["release", "disconnect", "setup", "rewind"] {
            let (mut service, mut client) = fixture();
            let actor = ActorId(1);
            start(&mut service, &mut client, 7);
            if admitted {
                assert!(matches!(service.step(), Step::Progress));
                assert!(service.engine.queued_travel_admission(actor).is_some());
                assert_eq!(service.travel_status[&actor].completed_steps, 0);
            }
            match mode {
                "release" => service.handle(client.id, "release".into(), Request::ReleaseControl),
                "disconnect" => service.disconnect(client.id),
                "setup" => setup(&mut service, &mut client, "place 3 1 0 0 on"),
                _ => setup(&mut service, &mut client, "rewind initial"),
            }
            assert!(service.travels.is_empty());
            assert_eq!(service.engine.queued_travel_admission(actor), None);
            assert!(service.pending_travel_cancellations.is_empty());
            service.run_until_blocked();
            assert_eq!(service.engine.observation(actor).unwrap().tick, 0);
            assert_eq!(service.engine.next_intention_actor(), None);
        }
    }
}

#[test]
fn harmless_discoveries_do_not_interrupt_travel() {
    let (mut service, mut client) = fixture();
    setup(&mut service, &mut client, "room 4 20 1 1 Long corridor");
    setup(&mut service, &mut client, "item tablet 4 17 0 0");
    setup(&mut service, &mut client, "place 4 18 0 0 on");
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
    setup(&mut service, &mut client, "actor 100 4 17 0 0");
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
        .any(|actor| actor.id
            == service
                .engine
                .target_scope(ActorId(1))
                .actor(tor_simulation::ActorId(2))));
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
            id: observation.self_target,
            position: Position { x: 1, y: 0, z: 0 },
        },
        ActorView {
            name: "figure".into(),
            description: "A figure.".into(),
            asset: None,
            id: service
                .engine
                .target_scope(ActorId(1))
                .actor(tor_simulation::ActorId(2)),
            position: Position { x: 2, y: 0, z: 0 },
        },
        ActorView {
            name: "figure".into(),
            description: "A figure.".into(),
            asset: None,
            id: service
                .engine
                .target_scope(ActorId(1))
                .actor(tor_simulation::ActorId(2)),
            position: Position { x: 3, y: 0, z: 0 },
        },
    ];
    assert_eq!(
        potential_hazards(&observation),
        BTreeSet::from([service
            .engine
            .target_scope(ActorId(1))
            .actor(tor_simulation::ActorId(2))])
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
            context: service.input_context(client.id).unwrap(),
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
                context: service.input_context(client.id).unwrap(),
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
    assert!(matches!(service.step(), Step::Progress));
    assert!(service.engine.queued_travel_admission(ActorId(1)).is_some());
    let old_branch = service.engine.branch().clone();
    drain(&mut spectator);
    setup(&mut service, &mut client, "rewind initial");
    let messages = drain(&mut spectator);
    assert!(
        matches!(messages.first(), Some(ServerMessage::Snapshot { snapshot, .. }) if snapshot.travel.is_none())
    );
    assert!(service.travels.is_empty());
    assert_ne!(service.engine.branch(), &old_branch);
    assert_eq!(service.engine.queued_travel_admission(ActorId(1)), None);
    service.run_until_blocked();
    assert_eq!(service.engine.observation(ActorId(1)).unwrap().tick, 0);
}

#[test]
fn rewind_to_retained_pending_travel_settles_restored_work_before_snapshot() {
    let (mut service, mut controller) = fixture();
    let mut observer = connect(&mut service, AccessRole::Spectator);
    let actor = ActorId(1);
    start(&mut service, &mut controller, 7);
    assert!(matches!(service.step(), Step::Progress));
    let admission = service
        .engine
        .queued_travel_admission(actor)
        .unwrap()
        .clone();

    // A trusted backend boundary retains queued work independently of Session's
    // policy for stopping a journey when a wizard request changes the world.
    let marker = service
        .engine
        .command(
            "same-user",
            "test",
            actor,
            "retain-pending-travel",
            &service.engine.branch().clone(),
            crate::journal::Command::Wizard {
                expected_revision: service.engine.revision(actor).unwrap(),
                operation: crate::developer::parse_wizard("place 3 7 0 0 on").unwrap(),
            },
        )
        .unwrap();
    assert_eq!(
        service.engine.queued_travel_admission(actor),
        Some(&admission)
    );
    assert!(matches!(service.step(), Step::Progress));
    assert_eq!(service.engine.observation(actor).unwrap().tick, 100);
    let old_branch = service.engine.branch().clone();
    drain(&mut observer);
    setup(
        &mut service,
        &mut controller,
        &format!("rewind {}", marker.entry.id.0),
    );

    assert_ne!(service.engine.branch(), &old_branch);
    assert_eq!(service.engine.observation(actor).unwrap().tick, 0);
    assert_eq!(service.engine.queued_travel_admission(actor), None);
    assert!(service.travels.is_empty());
    assert!(service.travel_status.is_empty());
    assert!(service.pending_travel_cancellations.is_empty());
    let messages = drain(&mut observer);
    let Some(ServerMessage::Snapshot { snapshot, .. }) = messages.first() else {
        panic!("rewind must publish a snapshot before new-branch updates");
    };
    assert_eq!(&snapshot.branch, service.engine.branch());
    assert!(snapshot.travel.is_none());
    assert!(snapshot.intentions.is_empty());
    service.run_until_blocked();
    assert_eq!(service.engine.observation(actor).unwrap().tick, 0);
    assert_eq!(service.engine.next_intention_actor(), None);
}

#[test]
fn slow_controller_during_start_cannot_resurrect_travel_on_observers() {
    let (mut service, mut controller) = fixture();
    let mut observer = connect(&mut service, AccessRole::Spectator);
    for i in 0..QUEUE {
        service.handle(controller.id, format!("fill-{i}"), Request::Snapshot);
    }
    start(&mut service, &mut controller, 7);
    assert!(controller.close.borrow().is_some());
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
fn controller_transport_loss_during_execution_keeps_committed_progress_without_resuming() {
    let (mut service, mut controller) = fixture();
    let mut observer = connect(&mut service, AccessRole::Spectator);
    let actor = ActorId(1);
    start(&mut service, &mut controller, 7);
    assert!(matches!(service.step(), Step::Progress));
    assert_eq!(service.travel_status[&actor].completed_steps, 0);
    drain(&mut observer);

    // The transport can disappear after mailbox processing and before publication.
    // Keep the server-side connection until its next output detects the loss.
    let controller_id = controller.id;
    let closed = controller.close.clone();
    drop(controller);
    assert!(matches!(service.step(), Step::Progress));
    assert!(closed.borrow().is_some());
    assert!(!service.clients.contains_key(&controller_id));
    assert_eq!(service.engine.observation(actor).unwrap().tick, 100);
    assert_eq!(service.travel_status[&actor].completed_steps, 1);
    assert_eq!(
        service.travel_status[&actor].phase,
        TravelPhase::ControlLost
    );
    assert!(service.travels.is_empty());
    assert_eq!(service.engine.queued_travel_admission(actor), None);

    let statuses: Vec<_> = drain(&mut observer)
        .into_iter()
        .filter_map(|message| match message {
            ServerMessage::Update { update } => match update.body {
                UpdateBody::Travel { status, .. } => Some(status),
                _ => None,
            },
            _ => None,
        })
        .collect();
    assert!(!statuses.is_empty());
    assert!(statuses
        .iter()
        .all(|status| { status.phase == TravelPhase::ControlLost && status.completed_steps == 1 }));
    let mut replacement = connect(&mut service, AccessRole::Wizard);
    service.run_until_blocked();
    drain(&mut replacement);
    assert_eq!(service.engine.observation(actor).unwrap().tick, 100);
    assert_eq!(
        service.travel_status[&actor].phase,
        TravelPhase::ControlLost
    );
    let history = service
        .engine
        .history(actor, "same-user", None, 100)
        .unwrap();
    assert_eq!(
        history
            .entries
            .iter()
            .filter(|entry| { matches!(entry.content, HistoryContent::Action { .. }) })
            .count(),
        1
    );
}

#[test]
fn slow_spectator_pauses_the_journey_until_dropped_and_reconnects_at_committed_state() {
    let (mut service, mut controller) = fixture();
    let mut slow = connect(&mut service, AccessRole::Spectator);
    start(&mut service, &mut controller, 7);
    drain(&mut slow);
    assert!(matches!(service.step(), Step::Progress));
    assert_eq!(service.travel_status[&ActorId(1)].completed_steps, 0);
    assert!(matches!(service.step(), Step::Progress));
    assert_eq!(service.travel_status[&ActorId(1)].completed_steps, 1);
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
    assert!(slow.close.borrow().is_some());
    assert_eq!(
        service.travel_status[&ActorId(1)].phase,
        TravelPhase::Active
    );
    let mut replacement = connect(&mut service, AccessRole::Spectator);
    service.handle(replacement.id, "resync".into(), Request::Snapshot);
    let ServerMessage::Snapshot { snapshot, .. } = drain(&mut replacement).pop().unwrap() else {
        panic!("replacement snapshot");
    };
    assert_eq!(*snapshot.state, service.engine.state(ActorId(1)).unwrap());
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
    assert!(slow.close.borrow().is_some());
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

#[test]
fn every_answered_request_is_followed_by_whose_move_it_is() {
    // Regression: a `continue` that changed nothing got no fresh word, so a
    // client waited out its safety timeout.
    let (mut service, mut client) = fixture();
    let mut other = second_player(&mut service, &mut client, 7);
    service.run_until_blocked();
    drain(&mut client);
    drain(&mut other);
    let next = service.engine.next_actor().unwrap();
    let idle = if next == ActorId(1) {
        &mut other
    } else {
        &mut client
    };
    service.handle(idle.id, "continue".into(), Request::Continue);
    service.run_until_blocked();
    let told: Vec<_> = drain(idle)
        .into_iter()
        .filter_map(|m| match m {
            ServerMessage::Waiting { on } => Some(on),
            _ => None,
        })
        .collect();
    assert_eq!(told, [Waiting::Others]);
}

#[test]
fn paused_arena_announces_waiting_after_step_budget_finishes() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/mob-arena");
    let mut scenario = crate::scenario_package::load(&root, 42, None, false).unwrap();
    let package = scenario.package.take().unwrap();
    let mut manifest = package.manifest.clone();
    let arena = manifest.arena.as_mut().unwrap();
    arena.start_paused = true;
    arena.control = crate::scenario_package::ArenaControl::AllAi;
    scenario.package = Some(std::sync::Arc::new(
        crate::scenario_package::Package::from_parts(manifest, package.region_defs().unwrap())
            .unwrap(),
    ));
    let mut engine = Engine::memory(scenario).unwrap();
    engine.enable_wizard().unwrap();
    let mut service = Service::new(engine);
    let mut client = connect(&mut service, AccessRole::Wizard);
    setup(&mut service, &mut client, "arena step 32");
    let mut committed = 0;
    loop {
        let step = service.step();
        let messages = drain(&mut client);
        match step {
            Step::Progress => {
                committed += 1;
                assert!(committed <= 32);
                assert!(!messages
                    .iter()
                    .any(|m| matches!(m, ServerMessage::Waiting { .. })));
            }
            Step::Blocked => {
                assert_eq!(committed, 32);
                assert!(
                    messages.iter().any(|m| matches!(
                        m,
                        ServerMessage::Waiting {
                            on: Waiting::Stopped
                        }
                    )),
                    "completed arena step must notify clients"
                );
                break;
            }
            Step::Full(_) => panic!("drained client must have headroom"),
        }
    }
}
