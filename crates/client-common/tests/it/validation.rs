use tor_client_common::{ClientState, StreamError};
use tor_protocol::*;

fn snapshot(revision: u64) -> Snapshot {
    serde_json::from_value(serde_json::json!({
        "actor":1,"branch":"validation","cursor":{"sequence":0,"tick":revision},
        "has_control":false,"intentions":[],"history":{"entries":[],"older_before":null},
        "state":{"wizard_game":false,"revision":revision,"observation":{
            "actor":1,"tick":revision,"position":{"x":0,"y":0,"z":0},
            "places":[{"key":"here","name":"Here","origin":"authored"}],
            "visible_cells":[
                {"key":"here","position":{"x":0,"y":0,"z":0},"wall":false,"stairs_up":false,"stairs_down":false,"place_hint":false},
                {"key":"there","position":{"x":1,"y":0,"z":0},"wall":false,"stairs_up":false,"stairs_down":false,"place_hint":false}
            ],
            "ground_items":[{"reachable":false,"position":{"x":1,"y":0,"z":0},"item":{"id":8,"quantity":1,"name":"stone","appearance":"stone","identified":true}}],
            "inventory":[{"id":7,"quantity":1,"name":"stone","appearance":"stone","identified":true}],
            "visible_actors":[{"id":2,"position":{"x":1,"y":0,"z":0}}],
            "combat":{"hp":10,"max_hp":10,"preparation_remaining":null,"preparation_active":false,"recovery_remaining":0,"actors":[],"events":[],"objective":null,"victory":false,"dead":false,"terminal":false},
            "motion":{"velocity":[0,0,0],"units_per_cell":65536,"displaced":false,"impacted":false},
            "ready":true
        }}
    }))
    .unwrap()
}

type Corrupt = fn(&mut Observation);

#[test]
fn paused_progress_can_coexist_with_independent_queued_work() {
    let mut initial = snapshot(0);
    initial.has_control = true;
    initial
        .state
        .observation
        .combat
        .as_mut()
        .unwrap()
        .preparation_remaining = Some(30);
    initial.intentions = vec![
        IntentionStatus {
            actor: initial.actor,
            branch: initial.branch.clone(),
            intention: IntentionId("old-progress".into()),
            entry_id: EntryId("old-root".into()),
            phase: IntentionPhase::Paused,
        },
        IntentionStatus {
            actor: initial.actor,
            branch: initial.branch.clone(),
            intention: IntentionId("later-work".into()),
            entry_id: EntryId("later-root".into()),
            phase: IntentionPhase::Queued,
        },
    ];
    let client = ClientState::from_snapshot(initial).unwrap();
    assert_eq!(client.intentions().len(), 2);
    assert!(client.has_pending_intention());
    assert!(client.resume_intention_request().is_none());
}

#[test]
fn mixed_intention_controls_are_independent_of_snapshot_order() {
    for phase in [IntentionPhase::Queued, IntentionPhase::Suspended] {
        for queue_first in [false, true] {
            let mut initial = snapshot(0);
            initial.has_control = true;
            initial
                .state
                .observation
                .combat
                .as_mut()
                .unwrap()
                .preparation_remaining = Some(30);
            let progress = IntentionStatus {
                actor: initial.actor,
                branch: initial.branch.clone(),
                intention: IntentionId("paused-progress".into()),
                entry_id: EntryId("progress-root".into()),
                phase: IntentionPhase::Paused,
            };
            let queued = IntentionStatus {
                actor: initial.actor,
                branch: initial.branch.clone(),
                intention: IntentionId("queued-work".into()),
                entry_id: EntryId("queue-root".into()),
                phase,
            };
            initial.intentions = if queue_first {
                vec![queued, progress]
            } else {
                vec![progress, queued]
            };
            let client = ClientState::from_snapshot(initial).unwrap();
            let Some(Request::Command {
                command: Command::CancelIntention { intention, .. },
                ..
            }) = client.cancel_intention_request()
            else {
                panic!("cancel request");
            };
            assert_eq!(intention.0, "queued-work");
            if phase == IntentionPhase::Queued {
                assert!(client.resume_intention_request().is_none());
            } else {
                let Some(Request::Command {
                    command: Command::ResumeIntention { intention, .. },
                    ..
                }) = client.resume_intention_request()
                else {
                    panic!("resume request");
                };
                assert_eq!(intention.0, "queued-work");
            }
            assert_eq!(client.intentions().len(), 2);
        }
    }
}

#[test]
fn started_intention_can_suspend_resume_and_cancel_under_original_context() {
    let mut initial = snapshot(0);
    let original = IntentionStatus {
        actor: initial.actor,
        branch: initial.branch.clone(),
        intention: IntentionId("original-progress".into()),
        entry_id: EntryId("original-admission".into()),
        phase: IntentionPhase::Started,
    };
    initial.intentions = vec![original.clone()];
    let mut client = ClientState::from_snapshot(initial.clone()).unwrap();
    for (sequence, phase) in [
        IntentionPhase::Paused,
        IntentionPhase::Queued,
        IntentionPhase::Started,
        IntentionPhase::Cancelled,
    ]
    .into_iter()
    .enumerate()
    {
        let mut status = original.clone();
        status.phase = phase;
        client
            .apply(StreamUpdate {
                actor: initial.actor,
                branch: initial.branch.clone(),
                cursor: StreamCursor {
                    sequence: sequence as u64 + 1,
                    tick: 0,
                },
                body: UpdateBody::Intention { status },
            })
            .unwrap();
        if phase.active() {
            assert_eq!(client.intentions()[0].intention, original.intention);
            assert_eq!(client.intentions()[0].entry_id, original.entry_id);
        } else {
            assert!(client.intentions().is_empty());
        }
    }
    let before = client.clone();
    assert_eq!(
        client.apply(StreamUpdate {
            actor: initial.actor,
            branch: initial.branch,
            cursor: StreamCursor {
                sequence: 5,
                tick: 0
            },
            body: UpdateBody::Intention { status: original },
        }),
        Err(StreamError::InconsistentState)
    );
    assert_eq!(client, before);
}

#[test]
fn intention_controls_require_current_control_and_never_reuse_an_abandoned_context() {
    let mut initial = snapshot(9);
    initial.has_control = true;
    initial.state.observation.ready = false;
    initial.intentions = vec![IntentionStatus {
        actor: initial.actor,
        branch: initial.branch.clone(),
        intention: IntentionId("original".into()),
        entry_id: EntryId("admission".into()),
        phase: IntentionPhase::Suspended,
    }];
    let mut client = ClientState::from_snapshot(initial.clone()).unwrap();
    assert!(
        matches!(client.resume_intention_request(), Some(Request::Command { branch, command:
        Command::ResumeIntention { expected_revision: 9, intention } })
        if branch == initial.branch && intention.0 == "original")
    );
    client
        .apply(StreamUpdate {
            actor: initial.actor,
            branch: initial.branch.clone(),
            cursor: StreamCursor {
                sequence: 1,
                tick: 9,
            },
            body: UpdateBody::Control { has_control: false },
        })
        .unwrap();
    assert!(client.resume_intention_request().is_none());
    assert!(client.cancel_intention_request().is_none());
    let mut replaced = snapshot(11);
    replaced.branch = BranchId("replacement".into());
    replaced.has_control = true;
    replaced.intentions = vec![IntentionStatus {
        actor: replaced.actor,
        branch: replaced.branch.clone(),
        intention: IntentionId("new-work".into()),
        entry_id: EntryId("new-admission".into()),
        phase: IntentionPhase::Queued,
    }];
    client.replace_snapshot(replaced.clone()).unwrap();
    assert!(client.resume_intention_request().is_none());
    assert!(
        matches!(client.cancel_intention_request(), Some(Request::Command { branch, command:
        Command::CancelIntention { expected_revision: 11, intention } })
        if branch == replaced.branch && intention.0 == "new-work")
    );
    replaced.intentions[0].phase = IntentionPhase::Started;
    client.replace_snapshot(replaced).unwrap();
    assert!(client.resume_intention_request().is_none());
    assert!(
        matches!(client.cancel_intention_request(), Some(Request::Command {
        command: Command::CancelIntention { intention, .. }, .. }) if intention.0 == "new-work")
    );
}

#[test]
fn intention_lifecycle_is_separate_from_observation_readiness_and_rejects_foreign_context_atomically(
) {
    let initial = snapshot(0);
    let mut client = ClientState::from_snapshot(initial.clone()).unwrap();
    let queued = IntentionStatus {
        actor: initial.actor,
        branch: initial.branch.clone(),
        intention: IntentionId("opaque".into()),
        entry_id: EntryId("admission".into()),
        phase: IntentionPhase::Queued,
    };
    client
        .apply(StreamUpdate {
            actor: initial.actor,
            branch: initial.branch.clone(),
            cursor: StreamCursor {
                sequence: 1,
                tick: 0,
            },
            body: UpdateBody::Intention {
                status: queued.clone(),
            },
        })
        .unwrap();
    assert_eq!(client.state(), &initial.state);
    assert!(client.has_pending_intention());
    let before = client.clone();
    let mut foreign = queued.clone();
    foreign.actor = ActorId(2);
    foreign.phase = IntentionPhase::Resolved;
    assert_eq!(
        client.apply(StreamUpdate {
            actor: initial.actor,
            branch: initial.branch.clone(),
            cursor: StreamCursor {
                sequence: 2,
                tick: 0
            },
            body: UpdateBody::Intention { status: foreign },
        }),
        Err(StreamError::InconsistentState)
    );
    assert_eq!(client, before);
    let mut resolved = queued;
    resolved.phase = IntentionPhase::Resolved;
    client
        .apply(StreamUpdate {
            actor: initial.actor,
            branch: initial.branch,
            cursor: StreamCursor {
                sequence: 2,
                tick: 0,
            },
            body: UpdateBody::Intention { status: resolved },
        })
        .unwrap();
    assert!(!client.has_pending_intention());
    assert_eq!(client.state(), &initial.state);
}

fn corruptions() -> [(&'static str, Corrupt); 12] {
    [
        ("cell occurrence", |o| {
            o.visible_cells.push(o.visible_cells[0].clone())
        }),
        ("inventory identity", |o| {
            o.inventory.push(o.inventory[0].clone())
        }),
        ("ground occurrence", |o| {
            o.ground_items.push(o.ground_items[0].clone())
        }),
        ("actor occurrence", |o| {
            o.visible_actors.push(o.visible_actors[0].clone())
        }),
        ("place identity", |o| o.places.push(o.places[0].clone())),
        ("inventory quantity", |o| o.inventory[0].quantity = 0),
        ("ground quantity", |o| o.ground_items[0].item.quantity = 0),
        ("health", |o| o.combat.as_mut().unwrap().hp = 11),
        ("preparation", |o| {
            o.combat.as_mut().unwrap().preparation_active = true
        }),
        ("death", |o| o.combat.as_mut().unwrap().dead = true),
        ("motion scale", |o| {
            o.motion.as_mut().unwrap().units_per_cell = 0
        }),
        ("item location", |o| {
            o.ground_items[0].item.id = o.inventory[0].id
        }),
    ]
}

#[test]
fn malformed_observations_reject_at_every_boundary_without_partial_effects() {
    for (label, corrupt) in corruptions() {
        let initial = snapshot(0);
        let mut next = snapshot(1);
        corrupt(&mut next.state.observation);
        assert_eq!(
            ClientState::from_snapshot(next.clone()),
            Err(StreamError::InconsistentState),
            "snapshot: {label}"
        );
        let mut client = ClientState::from_snapshot(initial).unwrap();
        let before = client.clone();
        assert_eq!(
            client.replace_snapshot(next.clone()),
            Err(StreamError::InconsistentState),
            "replacement: {label}"
        );
        assert_eq!(client, before);
        let delta = StateDelta::between(client.state(), &next.state);
        let full = UpdateBody::Observation {
            state: Box::new(next.state),
            event: None,
        };
        let mut bodies = vec![full];
        if let Some(delta) = delta {
            bodies.push(UpdateBody::ObservationDelta {
                state: Box::new(delta),
                event: None,
            });
        }
        for body in bodies {
            let update = StreamUpdate {
                actor: ActorId(1),
                branch: next.branch.clone(),
                cursor: StreamCursor {
                    sequence: 1,
                    tick: 1,
                },
                body,
            };
            assert_eq!(
                client.apply(update),
                Err(StreamError::InconsistentState),
                "update: {label}"
            );
            assert_eq!(client, before);
        }
    }
}

#[test]
fn repeated_portal_entities_at_distinct_offsets_remain_valid() {
    let mut next = snapshot(0);
    let o = &mut next.state.observation;
    o.visible_cells[1].key = o.visible_cells[0].key.clone();
    let mut item = o.ground_items[0].clone();
    item.position = o.visible_cells[0].position;
    o.ground_items.push(item);
    let mut actor = o.visible_actors[0].clone();
    actor.position = o.visible_cells[0].position;
    o.visible_actors.push(actor);
    assert!(ClientState::from_snapshot(next).is_ok());
}
