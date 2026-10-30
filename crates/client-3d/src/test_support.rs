//! Valid renderer/transport fixtures, without a window or a simulation dependency.
use super::*;
use tor_protocol::{BranchId, InputContext, ReplyContext, StreamContext, StreamCursor, StreamId};

pub(crate) fn view(revision: u64) -> View {
    let context = StreamContext {
        stream: StreamId("test".into()),
        epoch: 0,
    };
    let branch = BranchId("test".into());
    View {
        shown: ReplyContext {
            snapshot: tor_protocol::SnapshotId(format!("fixture-display-{revision}")),
            input: InputContext {
                stream: context.clone(),
                readiness_revision: 0,
            },
            actor: ActorId(1),
            branch: branch.clone(),
            cursor: StreamCursor {
                sequence: revision,
                tick: 0,
            },
            revision,
        },
        context,
        branch,
        feet: Position { x: 0, y: 0, z: 0 },
        visible: vec![],
        object_models: Default::default(),
        memory: vec![],
        actors: vec![],
        items: vec![],
        inventory: vec![],
        equipment: Default::default(),
        holds: vec![],
        abilities: vec![],
        identify: None,
        narration: vec![],
        status: Default::default(),
        hp: None,
        attributes: None,
        has_control: false,
        can_act: false,
        history: vec![],
        chart_kinds: vec![],
        places: vec![],
        can_resume: false,
        can_cancel: false,
        tick: 0,
        revision,
        events: vec![],
    }
}

pub(crate) fn input(command: Cmd) -> tor_client_3d_model::Input {
    tor_client_3d_model::Input {
        shown: view(0).shown,
        command,
    }
}
