use tor_client_common::{ClientState, StreamError};
use tor_protocol::*;

pub(super) fn snapshot(revision: u64) -> Snapshot {
    serde_json::from_value(serde_json::json!({
        "readiness":{"revision":"0","admission":false,"resume":[],"cancel":[]},
        "context":{"stream":"3b7523b8-893a-4ea9-8b09-0a3887a7e6a1","epoch":"0"},
        "actor":"1","branch":"validation","cursor":{"sequence":"0","tick":revision.to_string()},
        "has_control":false,"intentions":[],"history":{"entries":[],"older_before":null},
        "state":{"wizard_game":false,"revision":revision.to_string(),"observation":{
            "actor":"1","tick":revision.to_string(),"position":{"x":0,"y":0,"z":0},
            "places":[{"key":"here","name":"Here","origin":"authored"}],
            "visible_cells":[
                {"key":"here","position":{"x":0,"y":0,"z":0},"wall":false,"stairs_up":false,"stairs_down":false,"place_hint":false},
                {"key":"there","position":{"x":1,"y":0,"z":0},"wall":false,"stairs_up":false,"stairs_down":false,"place_hint":false}
            ],
            "ground_items":[{"reachable":false,"position":{"x":1,"y":0,"z":0},"item":{"id":"8","quantity":"1","name":"stone","appearance":"stone","identified":true}}],
            "inventory":[{"id":"7","quantity":"1","name":"stone","appearance":"stone","identified":true}],
            "visible_actors":[{"id":"2","position":{"x":1,"y":0,"z":0}}],
            "combat":{"hp":10,"max_hp":10,"preparation_remaining":null,"preparation_active":false,"recovery_remaining":"0","actors":[],"events":[],"objective":null,"victory":false,"dead":false,"terminal":false},
            "motion":{"velocity":["0","0","0"],"units_per_cell":65536,"displaced":false,"impacted":false},
            "ready":true
        }}
    }))
    .unwrap()
}

#[test]
fn reply_boundary_validation_never_installs_missing_or_foreign_stream_state() {
    let client = ClientState::from_snapshot(snapshot(4)).unwrap();
    let current = client.reply_context();
    assert!(client.validate_reply_context(&current).is_ok());
    let before = client.clone();
    let mut cases = Vec::new();
    let mut foreign = current.clone();
    foreign.input.stream.stream = StreamId("another-attachment".into());
    cases.push((foreign, StreamError::WrongStreamContext));
    let mut old_epoch = current.clone();
    old_epoch.input.stream.epoch += 1;
    cases.push((old_epoch, StreamError::WrongStreamContext));
    let mut actor = current.clone();
    actor.actor = ActorId(2);
    cases.push((actor, StreamError::WrongActor));
    let mut branch = current.clone();
    branch.branch = BranchId("receipt-origin-is-not-current-context".into());
    cases.push((branch, StreamError::WrongBranch));
    let mut sequence = current.clone();
    sequence.cursor.sequence += 1;
    cases.push((sequence, StreamError::SequenceMismatch));
    let mut tick = current.clone();
    tick.cursor.tick += 1;
    cases.push((tick, StreamError::SequenceMismatch));
    let mut revision = current.clone();
    revision.revision += 1;
    cases.push((revision, StreamError::InconsistentState));
    let mut readiness = current;
    readiness.input.readiness_revision += 1;
    cases.push((readiness, StreamError::InconsistentState));
    for (context, error) in cases {
        assert_eq!(client.validate_reply_context(&context), Err(error));
        assert_eq!(client, before);
    }
}

#[test]
fn reply_context_captures_current_disclosure_independently_of_receipt_origin() {
    let initial = snapshot(0);
    let mut client = ClientState::from_snapshot(initial.clone()).unwrap();
    let before = client.reply_context();
    client
        .apply(StreamUpdate {
            context: initial.context.clone(),
            actor: initial.actor,
            branch: initial.branch.clone(),
            cursor: StreamCursor {
                sequence: 1,
                tick: 0,
            },
            body: UpdateBody::Readiness {
                readiness: Readiness {
                    revision: 1,
                    admission: false,
                    resume: vec![],
                    cancel: vec![],
                },
            },
        })
        .unwrap();
    let permission_boundary = client.reply_context();
    assert_eq!(permission_boundary.revision, before.revision);
    assert_eq!(
        permission_boundary.cursor.sequence,
        before.cursor.sequence + 1
    );
    assert_eq!(permission_boundary.input.readiness_revision, 1);
    let receipt = RequestReceipt::Immediate {
        actor: initial.actor,
        branch: BranchId("original-operation".into()),
        entry_id: None,
    };
    let mut reset = initial;
    reset.context.epoch += 1;
    reset.branch = BranchId("new-disclosure".into());
    client.replace_snapshot(reset.clone()).unwrap();
    let current = client.reply_context();
    assert_eq!(current.input.stream, reset.context);
    assert_eq!(current.branch, reset.branch);
    assert_eq!(receipt.branch(), &BranchId("original-operation".into()));
    assert_ne!(current.branch, *receipt.branch());
    assert_eq!(
        before.input.readiness_revision, 0,
        "captured contexts never restamp"
    );
}

#[test]
fn admission_permission_is_independent_of_due_turn_and_blocks_interim_lifecycle() {
    let mut initial = snapshot(0);
    initial.has_control = true;
    initial.state.observation.ready = false;
    initial.readiness.admission = true;
    let mut client = ClientState::from_snapshot(initial.clone()).unwrap();
    assert!(client.can_admit_intention());
    let base = client.observation_base();
    client
        .apply(StreamUpdate {
            context: initial.context.clone(),
            actor: initial.actor,
            branch: initial.branch.clone(),
            cursor: StreamCursor {
                sequence: 1,
                tick: 0,
            },
            body: UpdateBody::Intention {
                status: IntentionStatus {
                    actor: initial.actor,
                    branch: initial.branch.clone(),
                    intention: IntentionId("queued".into()),
                    entry_id: EntryId("root".into()),
                    phase: IntentionPhase::Queued,
                },
            },
        })
        .unwrap();
    assert!(!client.can_admit_intention());
    client
        .apply(StreamUpdate {
            context: initial.context,
            actor: initial.actor,
            branch: initial.branch,
            cursor: StreamCursor {
                sequence: 2,
                tick: 0,
            },
            body: UpdateBody::Readiness {
                readiness: Readiness {
                    revision: 1,
                    admission: false,
                    resume: vec![],
                    cancel: vec![IntentionId("queued".into())],
                },
            },
        })
        .unwrap();
    assert!(!client.can_admit_intention());
    assert!(client.can_cancel_intention());
    assert!(!client.can_resume_intention());
    assert_eq!(client.observation_base(), base);
}

#[test]
fn readiness_is_ordered_independently_of_observation_revision_and_rejects_atomically() {
    let initial = snapshot(0);
    let mut client = ClientState::from_snapshot(initial.clone()).unwrap();
    let wire = serde_json::json!({
        "context": initial.context, "actor": initial.actor, "branch": initial.branch,
        "cursor": {"sequence":"1","tick":"0"},
        "body": {"type":"readiness", "readiness":{
            "revision":"1","admission":false,"resume":[],"cancel":[]
        }}
    });
    let update: StreamUpdate = serde_json::from_value(wire.clone()).unwrap();
    client.apply(update).unwrap();
    assert_eq!(client.state().revision, 0);
    assert_eq!(client.observation_base().cursor.sequence, 0);
    assert_eq!(
        serde_json::to_value(client.snapshot()).unwrap()["readiness"]["revision"],
        "1"
    );
    let before = client.clone();
    for invalid in [
        serde_json::json!({"revision":"1","admission":false,"resume":[],"cancel":[]}),
        serde_json::json!({"revision":"3","admission":false,"resume":[],"cancel":[]}),
        serde_json::json!({"revision":"2","admission":true,"resume":[],"cancel":[]}),
        serde_json::json!({"revision":"2","admission":false,"resume":["unknown"],"cancel":[]}),
    ] {
        let mut malformed = wire.clone();
        malformed["cursor"]["sequence"] = serde_json::json!("2");
        malformed["body"]["readiness"] = invalid;
        let update: StreamUpdate = serde_json::from_value(malformed).unwrap();
        assert!(client.apply(update).is_err());
        assert_eq!(client, before);
    }
}

#[test]
fn readiness_rejects_duplicate_or_unavailable_controls_and_counter_overflow_atomically() {
    let mut initial = snapshot(0);
    initial.has_control = true;
    initial.intentions = vec![IntentionStatus {
        actor: initial.actor,
        branch: initial.branch.clone(),
        intention: IntentionId("paused".into()),
        entry_id: EntryId("root".into()),
        phase: IntentionPhase::Paused,
    }];
    initial.readiness = Readiness {
        revision: u64::MAX,
        admission: false,
        resume: vec![IntentionId("paused".into())],
        cancel: vec![IntentionId("paused".into())],
    };
    let mut client = ClientState::from_snapshot(initial.clone()).unwrap();
    let before = client.clone();
    let mut wrapped = initial.readiness.clone();
    wrapped.revision = 0;
    assert!(client
        .apply(StreamUpdate {
            context: initial.context.clone(),
            actor: initial.actor,
            branch: initial.branch.clone(),
            cursor: StreamCursor {
                sequence: 1,
                tick: 0
            },
            body: UpdateBody::Readiness { readiness: wrapped },
        })
        .is_err());
    assert_eq!(client, before);
    let mut duplicate = initial.clone();
    duplicate
        .readiness
        .resume
        .push(IntentionId("paused".into()));
    assert!(ClientState::from_snapshot(duplicate).is_err());
    let mut unavailable = initial.clone();
    unavailable.intentions[0].phase = IntentionPhase::Queued;
    assert!(ClientState::from_snapshot(unavailable).is_err());
    let mut foreign = initial.clone();
    foreign.readiness.cancel = vec![IntentionId("undisclosed".into())];
    assert!(ClientState::from_snapshot(foreign).is_err());
    initial.has_control = false;
    assert!(ClientState::from_snapshot(initial).is_err());
}

#[test]
fn intention_controls_require_disclosed_permission_even_when_phase_and_control_are_eligible() {
    let mut initial = snapshot(0);
    initial.has_control = true;
    initial.intentions = vec![IntentionStatus {
        actor: initial.actor,
        branch: initial.branch.clone(),
        intention: IntentionId("original".into()),
        entry_id: EntryId("root".into()),
        phase: IntentionPhase::Suspended,
    }];
    let mut client = ClientState::from_snapshot(initial.clone()).unwrap();
    assert!(
        client.resume_intention_request().is_none(),
        "phase alone does not grant resume"
    );
    assert!(
        client.cancel_intention_request().is_none(),
        "phase alone does not grant cancel"
    );
    let update = |sequence, revision, resume| StreamUpdate {
        context: initial.context.clone(),
        actor: initial.actor,
        branch: initial.branch.clone(),
        cursor: StreamCursor { sequence, tick: 0 },
        body: UpdateBody::Readiness {
            readiness: Readiness {
                revision,
                admission: false,
                resume,
                cancel: vec![IntentionId("original".into())],
            },
        },
    };
    client.apply(update(1, 1, vec![])).unwrap();
    assert!(client.resume_intention_request().is_none());
    let cancel = client
        .cancel_intention_request()
        .expect("explicit cancellation permission");
    assert!(
        matches!(&cancel, Request::Command { context, command: Command::CancelIntention { intention, .. }, .. }
        if context.readiness_revision == 1 && intention.0 == "original")
    );
    client
        .apply(update(2, 2, vec![IntentionId("original".into())]))
        .unwrap();
    assert!(
        matches!(client.resume_intention_request(), Some(Request::Command { context, .. }) if context.readiness_revision == 2)
    );
    assert!(
        matches!(cancel, Request::Command { context, .. } if context.readiness_revision == 1),
        "a queued request retains its originating permission generation"
    );
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
            initial.readiness.cancel = vec![
                IntentionId("queued-work".into()),
                IntentionId("paused-progress".into()),
            ];
            if phase == IntentionPhase::Suspended {
                initial.readiness.resume = vec![IntentionId("queued-work".into())];
            }
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
                context: client.context().clone(),
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
            context: client.context().clone(),
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
    initial.readiness.resume = vec![IntentionId("original".into())];
    initial.readiness.cancel = vec![IntentionId("original".into())];
    let mut client = ClientState::from_snapshot(initial.clone()).unwrap();
    assert!(
        matches!(client.resume_intention_request(), Some(Request::Command { branch, context: _, command:
        Command::ResumeIntention { expected_revision: 9, intention } })
        if branch == initial.branch && intention.0 == "original")
    );
    client
        .apply(StreamUpdate {
            context: client.context().clone(),
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
    replaced.readiness.cancel = vec![IntentionId("new-work".into())];
    super::reset_snapshot(&mut client, replaced.clone()).unwrap();
    assert!(client.resume_intention_request().is_none());
    assert!(
        matches!(client.cancel_intention_request(), Some(Request::Command { branch, context: _, command:
        Command::CancelIntention { expected_revision: 11, intention } })
        if branch == replaced.branch && intention.0 == "new-work")
    );
    replaced.intentions[0].phase = IntentionPhase::Started;
    super::reset_snapshot(&mut client, replaced).unwrap();
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
            context: client.context().clone(),
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
            context: client.context().clone(),
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
            context: client.context().clone(),
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
            super::reset_snapshot(&mut client, next.clone()),
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
                base: client.observation_base(),
                state: Box::new(delta),
                event: None,
            });
        }
        for body in bodies {
            let update = StreamUpdate {
                context: client.context().clone(),
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

fn stream_context_snapshot(stream: &str, epoch: u64) -> Snapshot {
    let mut wire = serde_json::to_value(snapshot(0)).unwrap();
    wire["context"] = serde_json::json!({"stream":stream,"epoch":epoch.to_string()});
    serde_json::from_value(wire).unwrap()
}

fn stream_context_control(stream: &str, epoch: u64) -> StreamUpdate {
    serde_json::from_value(serde_json::json!({
        "actor":"1","branch":"validation","cursor":{"sequence":"1","tick":"0"},
        "context":{"stream":stream,"epoch":epoch.to_string()},
        "body":{"type":"control","has_control":true}
    }))
    .unwrap()
}

#[test]
fn stream_context_rejects_an_obsolete_attachment_without_granting_control() {
    let current = "3b7523b8-893a-4ea9-8b09-0a3887a7e6a1";
    let obsolete = "3b7523b8-893a-4ea9-8b09-0a3887a7e6a2";
    let mut client = ClientState::from_snapshot(stream_context_snapshot(current, 2)).unwrap();
    let before = client.clone();
    assert!(client.apply(stream_context_control(obsolete, 2)).is_err(),
        "matching actor, branch, sequence and tick must not admit another attachment's control update");
    assert_eq!(client, before);
    client.apply(stream_context_control(current, 2)).unwrap();
    assert!(client.has_control());
}

#[test]
fn stream_context_rejects_an_obsolete_reset_without_consuming_its_sequence() {
    let current = "3b7523b8-893a-4ea9-8b09-0a3887a7e6a1";
    let mut client = ClientState::from_snapshot(stream_context_snapshot(current, 2)).unwrap();
    let before = client.clone();
    assert!(
        client.apply(stream_context_control(current, 1)).is_err(),
        "an older reset must not grant control even when the remaining stream fields match"
    );
    assert_eq!(client, before);
    client.apply(stream_context_control(current, 2)).unwrap();
    assert!(client.has_control());
}

#[test]
fn snapshot_reset_requires_a_new_epoch_in_the_current_attachment() {
    let current = "attachment-current";
    let mut client = ClientState::from_snapshot(stream_context_snapshot(current, 2)).unwrap();
    let before = client.clone();
    for (stream, epoch) in [(current, 1), (current, 2), ("another-attachment", 3)] {
        assert_eq!(
            client.replace_snapshot(stream_context_snapshot(stream, epoch)),
            Err(StreamError::WrongStreamContext)
        );
        assert_eq!(client, before);
    }
    let mut malformed = stream_context_snapshot(current, 3);
    malformed.state.observation.actor = ActorId(2);
    assert!(client.replace_snapshot(malformed).is_err());
    assert_eq!(client, before);
    client
        .replace_snapshot(stream_context_snapshot(current, 3))
        .unwrap();
    assert_eq!(client.context().epoch, 3);
    client.apply(stream_context_control(current, 3)).unwrap();
    assert!(client.has_control());
}

#[test]
fn stream_context_rejects_malformed_identity_and_never_wraps_epoch() {
    for stream in [String::new(), "a".repeat(129), "bad\nidentity".into()] {
        assert_eq!(
            ClientState::from_snapshot(stream_context_snapshot(&stream, 0)),
            Err(StreamError::WrongStreamContext)
        );
    }
    let context = super::stream_context(u64::MAX);
    assert_eq!(context.next_reset(), None);
    assert_eq!(context.epoch, u64::MAX);
    let mut wire = serde_json::to_value(snapshot(0)).unwrap();
    wire.as_object_mut().unwrap().remove("context");
    assert!(serde_json::from_value::<Snapshot>(wire).is_err());
    let mut wire = serde_json::to_value(snapshot(0)).unwrap();
    wire["context"]["unexpected"] = serde_json::json!(true);
    assert!(serde_json::from_value::<Snapshot>(wire).is_err());
}

#[test]
fn delta_base_names_the_last_observation_not_an_intervening_control_message() {
    let mut client = ClientState::from_snapshot(snapshot(0)).unwrap();
    let control = StreamUpdate {
        context: client.context().clone(),
        actor: ActorId(1),
        branch: client.branch().clone(),
        cursor: StreamCursor {
            sequence: 1,
            tick: 0,
        },
        body: UpdateBody::Control { has_control: true },
    };
    client.apply(control).unwrap();
    let delta = StateDelta::between(client.state(), &snapshot(1).state).unwrap();
    let wire = serde_json::json!({
        "context":client.context(), "actor":"1","branch":"validation",
        "cursor":{"sequence":"2","tick":"1"},
        "body":{"type":"observation_delta","state":delta,"event":null,
            "base":{"cursor":{"sequence":"1","tick":"0"},"revision":"0"}}
    });
    let before = client.clone();
    assert!(
        client
            .apply(serde_json::from_value(wire.clone()).unwrap())
            .is_err(),
        "a control message is not the base observation even when its tick/revision match"
    );
    assert_eq!(client, before);
    let mut correct = wire;
    correct["body"]["base"]["cursor"]["sequence"] = serde_json::json!("0");
    client
        .apply(serde_json::from_value(correct).unwrap())
        .unwrap();
    assert_eq!(client.state().revision, 1);
    assert_eq!(client.cursor().sequence, 2);
}

#[test]
fn reset_establishes_a_new_exact_observation_base_and_rejected_bases_are_atomic() {
    let mut client = ClientState::from_snapshot(snapshot(0)).unwrap();
    let mut reset = snapshot(0);
    reset.context.epoch = 1;
    reset.cursor.sequence = 7;
    client.replace_snapshot(reset).unwrap();
    let expected = client.observation_base();
    let delta = StateDelta::between(client.state(), &snapshot(1).state).unwrap();
    for base in [
        ObservationBase {
            cursor: StreamCursor {
                sequence: 0,
                tick: 0,
            },
            revision: 0,
        },
        ObservationBase {
            cursor: StreamCursor {
                sequence: 7,
                tick: 1,
            },
            revision: 0,
        },
        ObservationBase {
            cursor: StreamCursor {
                sequence: 7,
                tick: 0,
            },
            revision: 1,
        },
    ] {
        let before = client.clone();
        assert_eq!(
            client.apply(StreamUpdate {
                context: client.context().clone(),
                actor: ActorId(1),
                branch: client.branch().clone(),
                cursor: StreamCursor {
                    sequence: 8,
                    tick: 1
                },
                body: UpdateBody::ObservationDelta {
                    base,
                    state: Box::new(delta.clone()),
                    event: None
                },
            }),
            Err(StreamError::WrongObservationBase)
        );
        assert_eq!(client, before);
    }
    client
        .apply(StreamUpdate {
            context: client.context().clone(),
            actor: ActorId(1),
            branch: client.branch().clone(),
            cursor: StreamCursor {
                sequence: 8,
                tick: 1,
            },
            body: UpdateBody::ObservationDelta {
                base: expected,
                state: Box::new(delta),
                event: None,
            },
        })
        .unwrap();
    assert_eq!(
        client.observation_base(),
        ObservationBase {
            cursor: StreamCursor {
                sequence: 8,
                tick: 1
            },
            revision: 1,
        }
    );
}
