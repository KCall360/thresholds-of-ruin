use tor_client_common::{ClientState, StreamError};
use tor_protocol::*;

fn snapshot() -> Snapshot {
    serde_json::from_str(r#"{
        "actor":1,"branch":"branch-1","cursor":{"sequence":0,"tick":0},"has_control":false,
        "history":{"entries":[],"older_before":null},
        "state":{"revision":0,"observation":{
            "actor":1,"tick":0,"position":{"region":1,"x":1,"y":1,"z":0},
            "region":{"id":1,"name":"Entry","width":5,"depth":3,"height":1},
            "ground_items":[],"inventory":[],"visible_actors":[],"exits":[],"known_places":[],"ready":true
        }}
    }"#).unwrap()
}

fn note(sequence: u64) -> StreamUpdate {
    StreamUpdate {
        actor: ActorId(1),
        branch: BranchId("branch-1".into()),
        cursor: StreamCursor { sequence, tick: 0 },
        body: UpdateBody::Annotation {
            entry: Box::new(HistoryEntry {
                id: EntryId("note-1".into()),
                actor: ActorId(1),
                branch: BranchId("branch-1".into()),
                tick: 0,
                author: Author::User {
                    user: "alice".into(),
                },
                audience: Audience::Private,
                content: HistoryContent::Annotation {
                    anchor: Anchor::State { revision: 0 },
                    category: AnnotationCategory::Note,
                    text: "A note".into(),
                },
            }),
        },
    }
}

#[test]
fn notes_advance_stream_order_but_not_action_revision_or_observation() {
    let initial = snapshot();
    let mut model = ClientState::from_snapshot(initial.clone()).unwrap();
    model.apply(note(1)).unwrap();
    assert_eq!(model.state(), &initial.state);
    assert_eq!(model.cursor().sequence, 1);
    assert_eq!(model.history().len(), 1);
    model
        .apply(StreamUpdate {
            actor: ActorId(1),
            branch: initial.branch,
            cursor: StreamCursor {
                sequence: 2,
                tick: 0,
            },
            body: UpdateBody::Control { has_control: true },
        })
        .unwrap();
    assert!(model.has_control());
}

#[test]
fn gaps_wrong_branches_and_inconsistent_payloads_leave_the_client_unchanged() {
    let mut model = ClientState::from_snapshot(snapshot()).unwrap();
    let before = model.clone();
    assert_eq!(model.apply(note(2)), Err(StreamError::SequenceMismatch));
    let mut wrong_branch = note(1);
    wrong_branch.branch = BranchId("another-branch".into());
    assert_eq!(model.apply(wrong_branch), Err(StreamError::WrongBranch));
    let mut wrong_entry = note(1);
    if let UpdateBody::Annotation { entry } = &mut wrong_entry.body {
        entry.actor = ActorId(2);
    }
    assert_eq!(
        model.apply(wrong_entry),
        Err(StreamError::InconsistentState)
    );
    assert_eq!(model, before);
}
