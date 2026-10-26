//! Schema compatibility only: dedicated charts do not become native scene facts.
use std::sync::Arc;
use tor_client_3d_model::view;
use tor_client_common::{ClientState, Palette};
use tor_protocol::{chart_pages::*, *};

#[test]
fn required_chart_catalog_roundtrips_and_delta_replacement_never_manufactures_native_world_objects()
{
    let original = super::projection::state();
    assert!(original.state().observation.charts.is_empty());
    let original_view = view(&original, &Palette::default());
    let mut snapshot = original.snapshot();
    Arc::make_mut(&mut snapshot.state).observation.charts = vec![
        ChartSummary {
            kind: ChartKind::Monsters,
            supported: true,
            mode: ChartMode::Live,
            status: ChartStatus::Available,
            acquired_at: Some(snapshot.cursor.tick),
            chart: Some("monster-chart".into()),
            latest_attempt: None,
        },
        ChartSummary {
            kind: ChartKind::Mapping,
            supported: false,
            mode: ChartMode::Historical,
            status: ChartStatus::Available,
            acquired_at: Some(snapshot.cursor.tick),
            chart: Some("historical-chart".into()),
            latest_attempt: None,
        },
    ];
    let wire = serde_json::to_value(&snapshot).unwrap();
    assert!(wire["state"]["observation"]["charts"].is_array());
    let mut missing = wire.clone();
    missing["state"]["observation"]
        .as_object_mut()
        .unwrap()
        .remove("charts");
    assert!(serde_json::from_value::<Snapshot>(missing).is_err());
    let mut client = ClientState::from_snapshot(serde_json::from_value(wire).unwrap()).unwrap();
    let projected = view(&client, &Palette::default());
    assert_eq!(projected.visible, original_view.visible);
    assert_eq!(projected.memory, original_view.memory);
    assert_eq!(
        projected
            .actors
            .iter()
            .map(|actor| (actor.id, actor.position))
            .collect::<Vec<_>>(),
        original_view
            .actors
            .iter()
            .map(|actor| (actor.id, actor.position))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        projected
            .items
            .iter()
            .map(|item| (item.id, item.position))
            .collect::<Vec<_>>(),
        original_view
            .items
            .iter()
            .map(|item| (item.id, item.position))
            .collect::<Vec<_>>()
    );
    let mut next = client.state().clone();
    next.revision += 1;
    next.observation.charts[0].status = ChartStatus::Inactive;
    next.observation.charts[0].acquired_at = None;
    next.observation.charts[0].chart = None;
    let delta = StateDelta::between(client.state(), &next).unwrap();
    let encoded = serde_json::to_value(&delta).unwrap();
    assert_eq!(
        encoded["charts"],
        serde_json::to_value(&next.observation.charts).unwrap()
    );
    let mut missing = encoded.clone();
    missing.as_object_mut().unwrap().remove("charts");
    assert!(serde_json::from_value::<StateDelta>(missing).is_err());
    let delta: StateDelta = serde_json::from_value(encoded).unwrap();
    client
        .apply(StreamUpdate {
            context: client.context().clone(),
            actor: next.observation.actor,
            branch: client.branch().clone(),
            cursor: StreamCursor {
                sequence: client.cursor().sequence + 1,
                tick: next.observation.tick,
            },
            body: UpdateBody::ObservationDelta {
                base: client.observation_base(),
                state: Box::new(delta),
                event: None,
            },
        })
        .unwrap();
    assert_eq!(client.state(), &next);
    let projected = view(&client, &Palette::default());
    assert_eq!(projected.visible, original_view.visible);
    assert_eq!(projected.memory, original_view.memory);
    assert_eq!(projected.actors.len(), original_view.actors.len());
    assert_eq!(projected.items.len(), original_view.items.len());
}
