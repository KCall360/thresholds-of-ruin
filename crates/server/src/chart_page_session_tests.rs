//! Adapter/Service authority fixtures. Paid acquisition is covered separately.
use super::*;
fn service() -> (Service, Connection) {
    let mut service = Service::new(Engine::memory(crate::Scenario::two_room(42)).unwrap());
    let client = connect(&mut service);
    (service, client)
}
fn connect(service: &mut Service) -> Connection {
    let mut client = service
        .connect(
            &Account {
                role: AccessRole::Spectator,
                user: "chart-reader".into(),
                token: "fixture".into(),
                actors: BTreeSet::from([ActorId(1)]),
            },
            "headless".into(),
        )
        .unwrap();
    service.handle(
        client.id,
        "attach".into(),
        Request::Attach { actor: ActorId(1) },
    );
    while client.messages.try_recv_frame().is_ok() {}
    client
}
fn synthetic(service: &mut Service, id: u64, rows: usize) -> ChartTransfer {
    let candidate = chart::Candidate::from_complete(
        chart::Kind::Food,
        vec![chart::Component {
            alignment: tor_simulation::scope_projection::Alignment::Aligned,
        }],
        vec![
            chart::Row {
                component: 0,
                position: Some([i64::MIN, i64::MAX, 0]),
                mark: chart::Mark::Food
            };
            rows
        ],
        chart::Budget {
            components: 1,
            rows,
            json_bytes: usize::MAX,
        },
    )
    .unwrap();
    let client = service.clients.get_mut(&id).unwrap();
    let observed = client.last_observation.as_mut().unwrap();
    let summary = ChartSummary {
        kind: ChartKind::Food,
        supported: true,
        mode: ChartMode::Historical,
        status: if rows == 0 {
            ChartStatus::Empty
        } else {
            ChartStatus::Available
        },
        acquired_at: Some(0),
        chart: Some("adapter-fixture".into()),
        latest_attempt: None,
    };
    // Explicit adapter fixture: not a counterfeit paid acquisition claim.
    Arc::make_mut(&mut observed.state).observation.charts = vec![summary.clone()];
    let issued = client.presentations.fixture_entry_mut(&observed.base);
    issued.charts = vec![summary.clone()];
    ChartTransfer {
        context: client.context.clone().unwrap(),
        actor: ActorId(1),
        branch: observed.branch.clone(),
        base: observed.base.clone(),
        summary,
        chart: candidate.shared_chart().unwrap().clone(),
        next: 0,
        bytes: 0,
    }
}
fn query(t: &ChartTransfer) -> ChartQuery {
    ChartQuery {
        context: t.context.clone(),
        branch: t.branch.clone(),
        base: t.base.clone(),
        chart: t.summary.chart.clone().unwrap(),
        cursor: Some(t.next as u64),
    }
}
fn messages(client: &mut Connection) -> Vec<ServerMessage> {
    std::iter::from_fn(|| client.messages.try_recv_frame().ok())
        .map(|f| decode_response(&f.text).unwrap())
        .collect()
}
#[test]
fn bounded_pages_preserve_every_occurrence_and_complete_correlated_envelope() {
    let (mut service, client) = service();
    let mut transfer = synthetic(&mut service, client.id, 1025);
    let context = service.reply_context(client.id).unwrap();
    let mut records = Vec::new();
    while records.len() < 1026 {
        let (page, bytes) =
            fit_page(&transfer, &context, &"\\".repeat(128), MAX_CHART_PAGE_BYTES).unwrap();
        assert!(page.records.len() <= MAX_CHART_PAGE_RECORDS);
        assert_eq!(page.start as usize, records.len());
        let message = ServerMessage::ChartPage {
            context: context.clone(),
            request_id: "\\".repeat(128),
            page: Box::new(page.clone()),
        };
        let encoded = encode_bounded_json(&message, MAX_CHART_PAGE_BYTES).unwrap();
        assert_eq!(encoded.len(), bytes);
        assert_eq!(decode_response(&encoded).unwrap(), message);
        records.extend(page.records);
        match page.next {
            Some(next) => transfer.next = next as usize,
            None => break,
        }
    }
    assert_eq!(records.len(), 1026);
    assert!(matches!(records[0], ChartRecord::Component(_)));
    assert!(records[1..].iter().all(|row| row == &records[1]));
}
#[test]
fn continuations_bind_attachment_branch_base_token_and_exact_admitted_cursor() {
    let (mut service, mut client) = service();
    let transfer = synthetic(&mut service, client.id, 1025);
    service
        .publish_chart_page(client.id, "first", transfer)
        .unwrap();
    let first = messages(&mut client)
        .into_iter()
        .find_map(|m| match m {
            ServerMessage::ChartPage { page, .. } => Some(page),
            _ => None,
        })
        .unwrap();
    let active = service.clients[&client.id].chart_transfer.clone().unwrap();
    assert_eq!(Some(active.next as u64), first.next);
    assert!(service
        .prepare_chart_page(client.id, query(&active))
        .is_ok());
    service.handle(client.id, "new-picture".into(), Request::Observe);
    assert_ne!(
        service.clients[&client.id]
            .last_observation
            .as_ref()
            .unwrap()
            .base
            .snapshot,
        active.base.snapshot
    );
    let retained = service
        .prepare_chart_page(client.id, query(&active))
        .unwrap();
    assert!(Arc::ptr_eq(&retained.chart, &active.chart));
    for change in 0..7 {
        let mut q = query(&active);
        match change {
            0 => q.context.epoch += 1,
            1 => q.branch = BranchId("other".into()),
            2 => q.base.revision += 1,
            3 => q.base.cursor.sequence += 1,
            4 => q.chart = "not-advertised".into(),
            5 => q.base.snapshot = tor_protocol::SnapshotId("not-issued".into()),
            _ => q.cursor = Some(active.next as u64 - 1),
        }
        assert_eq!(
            service.prepare_chart_page(client.id, q).err().unwrap().code,
            ErrorCode::StaleContext
        );
        assert_eq!(
            service.clients[&client.id]
                .chart_transfer
                .as_ref()
                .unwrap()
                .next,
            active.next
        );
    }
    let second = connect(&mut service);
    assert_eq!(
        service
            .prepare_chart_page(second.id, query(&active))
            .err()
            .unwrap()
            .code,
        ErrorCode::StaleContext
    );
    service.handle(client.id, "reset".into(), Request::Snapshot);
    assert!(service.clients[&client.id].chart_transfer.is_none());
    assert_eq!(
        service
            .prepare_chart_page(client.id, query(&active))
            .err()
            .unwrap()
            .code,
        ErrorCode::StaleContext
    );
}
#[test]
fn empty_chart_streams_component_proof_and_small_output_cap_refuses_atomically() {
    let (mut service, client) = service();
    let transfer = synthetic(&mut service, client.id, 0);
    let context = service.reply_context(client.id).unwrap();
    let (page, _) = fit_page(&transfer, &context, "empty", MAX_CHART_PAGE_BYTES).unwrap();
    assert_eq!(page.records.len(), 1);
    assert_eq!(page.total_records, 1);
    assert!(page.next.is_none());
    assert_eq!(
        fit_page(&transfer, &context, "small", CHART_ENVELOPE_BYTES)
            .err()
            .unwrap()
            .code,
        ErrorCode::ResourceLimit
    );
    assert!(service.clients[&client.id].chart_transfer.is_none());
}
#[test]
fn output_failure_drops_capability_and_does_not_mutate_game_or_history() {
    let (mut service, client) = service();
    let transfer = synthetic(&mut service, client.id, 1025);
    let before = service.engine.state(ActorId(1)).unwrap();
    let history = service
        .engine
        .history(ActorId(1), "chart-reader", None, 100)
        .unwrap();
    let filler = ServerMessage::Error {
        scope: ErrorScope::Attached {
            context: service.reply_context(client.id).unwrap(),
        },
        request_id: None,
        code: ErrorCode::InvalidRequest,
        message: "full".into(),
    };
    while service.clients[&client.id]
        .messages
        .try_send_with(&filler, encode_bounded_json)
        .is_ok()
    {}
    service
        .publish_chart_page(client.id, "blocked", transfer)
        .unwrap();
    assert!(!service.clients.contains_key(&client.id));
    assert_eq!(*client.close.borrow(), Some(DisconnectMode::Abort));
    assert_eq!(service.engine.state(ActorId(1)).unwrap(), before);
    assert_eq!(
        service
            .engine
            .history(ActorId(1), "chart-reader", None, 100)
            .unwrap(),
        history
    );
}
