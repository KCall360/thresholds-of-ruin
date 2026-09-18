use tor_client_common::{ClientState, StreamError};
use tor_protocol::*;

fn state() -> ClientState {
    ClientState::from_snapshot(serde_json::from_value(serde_json::json!({
        "actor":1,"branch":"branch","cursor":{"sequence":0,"tick":0},"has_control":true,"travel":null,
        "history":{"entries":[],"older_before":null},
        "state":{"wizard_game":false,"revision":0,"observation":{"actor":1,"tick":0,"position":{"x":0,"y":0,"z":0},"visible_cells":[],"ground_items":[],"inventory":[],"visible_actors":[],"ready":true}}
    })).unwrap()).unwrap()
}
fn update(sequence: u64, steps: u64, phase: TravelPhase, entry: bool) -> StreamUpdate {
    StreamUpdate {
        actor: ActorId(1),
        branch: BranchId("branch".into()),
        cursor: StreamCursor { sequence, tick: 0 },
        body: UpdateBody::Travel {
            status: TravelStatus {
                id: EntryId("trip".into()),
                destination: "cell".into(),
                completed_steps: steps,
                phase,
            },
            entry: entry.then(|| {
                Box::new(HistoryEntry {
                    id: EntryId("trip".into()),
                    branch: BranchId("branch".into()),
                    actor: ActorId(1),
                    tick: 0,
                    author: Author::User {
                        user: "player".into(),
                    },
                    audience: Audience::Actor,
                    content: HistoryContent::Travel {
                        destination: "cell".into(),
                    },
                })
            }),
        },
    }
}
#[test]
fn ordered_travel_status_is_separate_from_action_revisions_and_rejects_regression() {
    let mut client = state();
    let before = client.clone();
    assert_eq!(
        client.apply(update(1, 0, TravelPhase::Active, false)),
        Err(StreamError::InconsistentState)
    );
    assert_eq!(client, before);
    client
        .apply(update(1, 0, TravelPhase::Active, true))
        .unwrap();
    client
        .apply(update(2, 1, TravelPhase::Active, false))
        .unwrap();
    assert_eq!(client.state().revision, 0);
    assert_eq!(client.history().len(), 1);
    let before = client.clone();
    assert_eq!(
        client.apply(update(3, 0, TravelPhase::Active, false)),
        Err(StreamError::InconsistentState)
    );
    assert_eq!(client, before);
    client
        .apply(update(3, 1, TravelPhase::Cancelled, false))
        .unwrap();
    let before = client.clone();
    assert_eq!(
        client.apply(update(4, 1, TravelPhase::Active, false)),
        Err(StreamError::InconsistentState)
    );
    assert_eq!(client, before);
    client.replace_snapshot(serde_json::from_value(serde_json::json!({
        "actor":1,"branch":"rewound","cursor":{"sequence":0,"tick":0},"has_control":true,"travel":null,
        "history":{"entries":[],"older_before":null},"state":client.state()
    })).unwrap()).unwrap();
    assert!(client.travel().is_none());
}
