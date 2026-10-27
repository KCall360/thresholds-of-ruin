//! Production Service paging, identity and output-admission contracts.
use super::*;

fn service(limit: usize) -> (Service, Connection) {
    let mut service = Service::with_outbound_limits(
        Engine::memory(crate::Scenario::two_room(42)).unwrap(),
        crate::OutboundLimits {
            frame_bytes: limit,
            client_bytes: 16 * 1024 * 1024,
            total_bytes: 32 * 1024 * 1024,
        },
    )
    .unwrap();
    let client = connect(&mut service);
    (service, client)
}

fn connect(service: &mut Service) -> Connection {
    let mut client = service
        .connect(
            &Account {
                role: AccessRole::Spectator,
                user: "map-observer".into(),
                token: "test-only".into(),
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
    assert!(service.clients.contains_key(&client.id));
    client
}

fn query(service: &Service, client: u64) -> MapQuery {
    let client = &service.clients[&client];
    let observed = client.last_observation.as_ref().unwrap();
    MapQuery {
        context: client.context.clone().unwrap(),
        branch: observed.branch.clone(),
        base: observed.base,
        window: MapWindow::normal()
            .checked_translate(observed.state.observation.position)
            .unwrap(),
        cursor: None,
    }
}

fn page(service: &mut Service, client: &mut Connection, query: MapQuery) -> MapPage {
    service.handle(client.id, "\\".repeat(128), Request::Map { query });
    let mut result = None;
    while let Ok(frame) = client.messages.try_recv_frame() {
        assert!(frame.text.len() <= service.outbound.capabilities().max_response_bytes as usize);
        assert!(frame.following.is_empty());
        let decoded = decode_response(&frame.text).unwrap();
        match decoded {
            ServerMessage::MapPage { page, .. } => {
                assert!(result.replace(*page).is_none());
            }
            ServerMessage::Error { code, message, .. } => panic!("{code:?}: {message}"),
            _ => {}
        }
    }
    result.expect("one correlated map page")
}

fn synthetic_transfer(service: &Service, client: u64, count: usize) -> MapTransfer {
    let query = query(service, client);
    let mut transfer = service.prepare_map_page(client, query).unwrap();
    let projection = Arc::get_mut(&mut transfer.projection).unwrap();
    let sample = projection
        .cells
        .first()
        .expect("ordinary seen cell")
        .clone();
    projection.cells = (0..count)
        .map(|index| {
            let mut cell = sample.clone();
            cell.position = Position {
                x: transfer.window.min.x + (index % 63) as i32,
                y: transfer.window.min.y + ((index / 63) % 33) as i32,
                z: transfer.window.min.z + (index / (63 * 33)) as i32,
            };
            cell.ground_items.clear();
            cell.door = None;
            cell
        })
        .collect();
    projection.complete = true;
    projection.ambiguous = false;
    projection.limited = false;
    transfer
}

#[test]
fn actual_map_pages_are_complete_bounded_and_leave_state_history_unchanged() {
    let (mut service, mut client) = service(4096);
    let actor = ActorId(1);
    let before = service.engine.state(actor).unwrap();
    let history = service
        .engine
        .history(actor, "map-observer", None, 100)
        .unwrap();
    let mut request = query(&service, client.id);
    let expected = service.engine.explored_map(actor, request.window).unwrap();
    assert!(expected.complete && !expected.limited);
    assert!(!expected.cells.is_empty());
    let mut cells = Vec::new();
    let mut starts = BTreeSet::new();
    for _ in 0..MAX_MAP_TRANSFER_PAGES {
        let page = page(&mut service, &mut client, request.clone());
        assert!(page.complete && !page.limited);
        assert!(starts.insert(page.start));
        assert_eq!(page.start as usize, cells.len());
        assert_eq!(page.total_cells as usize, expected.cells.len());
        cells.extend(page.cells);
        request.cursor = page.next;
        if request.cursor.is_none() {
            break;
        }
    }
    assert!(request.cursor.is_none(), "bounded transfer must finish");
    assert_eq!(cells, expected.cells);
    assert_eq!(service.engine.state(actor).unwrap(), before);
    assert_eq!(
        service
            .engine
            .history(actor, "map-observer", None, 100)
            .unwrap(),
        history
    );
    assert!(service.clients[&client.id].map_transfer.is_none());
}

#[test]
fn prefix_fitting_counts_the_complete_escaped_envelope_without_skipping_cells() {
    let (service, client) = service(MAX_MAP_PAGE_BYTES);
    let mut transfer = synthetic_transfer(&service, client.id, 1025);
    let expected = transfer.projection.cells.clone();
    let context = service.reply_context(client.id).unwrap();
    let mut cells = Vec::new();
    for _ in 0..MAX_MAP_TRANSFER_PAGES {
        let (page, bytes) = fit_page(&transfer, &context, &"\\".repeat(128), 4096).unwrap();
        let message = ServerMessage::MapPage {
            context: context.clone(),
            request_id: "\\".repeat(128),
            page: Box::new(page.clone()),
        };
        let encoded = encode_bounded_json(&message, 4096).unwrap();
        assert_eq!(bytes, encoded.len());
        assert_eq!(decode_response(&encoded).unwrap(), message);
        assert!(!page.cells.is_empty() && page.cells.len() <= MAX_MAP_PAGE_CELLS);
        assert_eq!(page.start as usize, cells.len());
        cells.extend(page.cells);
        let Some(next) = page.next else { break };
        transfer.next = next.next as usize;
        transfer.page = next.page;
    }
    assert_eq!(cells, expected);
}

#[test]
fn stale_initial_context_and_cross_attachment_or_reordered_cursors_are_rejected() {
    let (mut service, mut client) = service(MAX_MAP_PAGE_BYTES);
    let original = query(&service, client.id);
    let before = service.engine.state(ActorId(1)).unwrap();
    let transfer = synthetic_transfer(&service, client.id, 1025);
    service
        .publish_map_page(client.id, "first", transfer)
        .unwrap();
    let first = std::iter::from_fn(|| client.messages.try_recv().ok())
        .find_map(|message| match message {
            ServerMessage::MapPage { page, .. } => Some(page),
            _ => None,
        })
        .unwrap();
    let cursor = first.next.clone().expect("more than one page");
    let active = service.clients[&client.id]
        .map_transfer
        .as_ref()
        .unwrap()
        .clone();
    for change in 0..5 {
        let mut request = original.clone();
        request.cursor = Some(cursor.clone());
        match change {
            0 => request.context.epoch += 1,
            1 => request.branch = BranchId("other".into()),
            2 => request.base.revision += 1,
            // Keep the cursor structurally valid while giving the wrong offset.
            3 => request.cursor.as_mut().unwrap().next -= 1,
            _ => request.cursor.as_mut().unwrap().page += 1,
        }
        assert_eq!(
            service
                .prepare_map_page(client.id, request)
                .err()
                .unwrap()
                .code,
            ErrorCode::StaleContext
        );
        assert_eq!(
            service.clients[&client.id]
                .map_transfer
                .as_ref()
                .unwrap()
                .next,
            active.next
        );
    }
    let mut malformed = original.clone();
    malformed.cursor = Some(cursor.clone());
    malformed.cursor.as_mut().unwrap().next = MAX_MAP_PAGE_CELLS as u32 + 1;
    assert_eq!(
        service
            .prepare_map_page(client.id, malformed)
            .err()
            .unwrap()
            .code,
        ErrorCode::InvalidRequest,
        "page one cannot follow more than one full page of cells"
    );
    assert_eq!(
        service.clients[&client.id]
            .map_transfer
            .as_ref()
            .unwrap()
            .next,
        active.next
    );
    let second = connect(&mut service);
    let mut other = query(&service, second.id);
    other.cursor = Some(cursor);
    assert_eq!(
        service
            .prepare_map_page(second.id, other)
            .err()
            .unwrap()
            .code,
        ErrorCode::StaleContext
    );
    service.handle(client.id, "reset".into(), Request::Snapshot);
    assert!(service.clients[&client.id].map_transfer.is_none());
    assert_eq!(
        service
            .prepare_map_page(client.id, original)
            .err()
            .unwrap()
            .code,
        ErrorCode::StaleContext
    );
    assert_eq!(service.engine.state(ActorId(1)).unwrap(), before);
}

#[test]
fn oversized_single_cell_and_transfer_byte_or_page_limits_refuse_without_advancing() {
    let (mut service, client) = service(MAX_MAP_PAGE_BYTES);
    let before = service.engine.state(ActorId(1)).unwrap();
    let mut transfer = synthetic_transfer(&service, client.id, 1);
    Arc::get_mut(&mut transfer.projection).unwrap().cells[0].ground_items = vec![
        MapItemAppearance {
            presentation: None,
            class: ItemClass::Misc,
            description: "x".repeat(1024),
            name: "token".into(),
            appearance: "plain".into(),
            identified: false,
            quantity: 1,
            asset: None,
        };
        128
    ];
    assert_eq!(
        service
            .publish_map_page(client.id, "too-large", transfer)
            .unwrap_err()
            .code,
        ErrorCode::ResourceLimit
    );
    assert!(service.clients[&client.id].map_transfer.is_none());
    let mut bytes = synthetic_transfer(&service, client.id, 1);
    bytes.bytes = MAX_MAP_TRANSFER_BYTES;
    assert_eq!(
        service
            .publish_map_page(client.id, "bytes", bytes)
            .unwrap_err()
            .code,
        ErrorCode::ResourceLimit
    );
    let mut pages = synthetic_transfer(&service, client.id, 1025);
    pages.page = MAX_MAP_TRANSFER_PAGES as u16 - 1;
    assert_eq!(
        service
            .publish_map_page(client.id, "pages", pages)
            .unwrap_err()
            .code,
        ErrorCode::ResourceLimit
    );
    assert!(service.clients.contains_key(&client.id));
    assert!(service.clients[&client.id].map_transfer.is_none());
    assert_eq!(service.engine.state(ActorId(1)).unwrap(), before);
}

#[test]
fn failed_outbound_admission_discards_transfer_without_committing_its_cursor() {
    let (mut service, client) = service(MAX_MAP_PAGE_BYTES);
    let before = service.engine.state(ActorId(1)).unwrap();
    let transfer = synthetic_transfer(&service, client.id, 1025);
    let context = service.reply_context(client.id).unwrap();
    let filler = ServerMessage::Error {
        scope: ErrorScope::Attached { context },
        request_id: None,
        code: ErrorCode::InvalidRequest,
        message: "fill".into(),
    };
    let sender = &service.clients[&client.id].messages;
    while sender.try_send_with(&filler, encode_bounded_json).is_ok() {}
    assert!(service.clients[&client.id].map_transfer.is_none());
    service
        .publish_map_page(client.id, "blocked", transfer)
        .unwrap();
    assert!(!service.clients.contains_key(&client.id));
    assert_eq!(*client.close.borrow(), Some(DisconnectMode::Abort));
    assert_eq!(service.engine.state(ActorId(1)).unwrap(), before);
}

#[test]
fn empty_and_work_limited_projections_have_one_explicit_terminal_page() {
    let (service, client) = service(MAX_MAP_PAGE_BYTES);
    let context = service.reply_context(client.id).unwrap();
    for limited in [false, true] {
        let mut transfer = synthetic_transfer(&service, client.id, 0);
        let projection = Arc::get_mut(&mut transfer.projection).unwrap();
        projection.complete = !limited;
        projection.limited = limited;
        let (page, bytes) = fit_page(&transfer, &context, "empty", 4096).unwrap();
        assert!(page.cells.is_empty() && page.next.is_none());
        assert_eq!((page.start, page.page, page.total_cells), (0, 0, 0));
        assert_eq!(page.complete, !limited);
        assert_eq!(page.limited, limited);
        assert!(bytes <= 4096);
    }
}

#[test]
fn publication_rechecks_observation_but_accepts_a_later_non_observation_cursor() {
    let (mut service, mut client) = service(MAX_MAP_PAGE_BYTES);
    let transfer = synthetic_transfer(&service, client.id, 1025);
    let before = service.engine.state(ActorId(1)).unwrap();
    let base = transfer.base;
    service.update(client.id, UpdateBody::Control { has_control: false });
    service
        .publish_map_page(client.id, "later-cursor", transfer)
        .unwrap();
    let page = std::iter::from_fn(|| client.messages.try_recv().ok())
        .find_map(|message| match message {
            ServerMessage::MapPage { context, page, .. } => {
                assert!(context.cursor.sequence > base.cursor.sequence);
                assert_eq!(page.base, base);
                Some(page)
            }
            _ => None,
        })
        .unwrap();
    let mut query = query(&service, client.id);
    query.cursor = page.next;
    let staged = service.prepare_map_page(client.id, query.clone()).unwrap();
    service.update(
        client.id,
        UpdateBody::Observation {
            state: before.clone().into(),
            event: None,
        },
    );
    assert!(service.clients[&client.id].map_transfer.is_none());
    assert_eq!(
        service
            .publish_map_page(client.id, "obsolete", staged)
            .unwrap_err()
            .code,
        ErrorCode::StaleContext
    );
    assert_eq!(
        service
            .prepare_map_page(client.id, query)
            .err()
            .unwrap()
            .code,
        ErrorCode::StaleContext
    );
    assert_eq!(service.engine.state(ActorId(1)).unwrap(), before);
}

#[test]
fn exact_envelope_byte_ceiling_and_last_terminal_page_are_inclusive() {
    let (service, client) = service(MAX_MAP_PAGE_BYTES);
    let context = service.reply_context(client.id).unwrap();
    for count in [0, 1] {
        // Empty transfers have only page zero. A final page at the page limit
        // must follow a valid nonempty prefix, even when only one cell remains.
        let prefix = if count == 0 {
            0
        } else {
            MAX_MAP_TRANSFER_PAGES - 1
        };
        let mut transfer = synthetic_transfer(&service, client.id, prefix + count);
        transfer.next = prefix;
        transfer.page = prefix as u16;
        let request_id = "\\\"".repeat(32);
        let (page, bytes) = fit_page(&transfer, &context, &request_id, 4096).unwrap();
        assert!(page.next.is_none());
        assert_eq!(page.page as usize, prefix);
        assert_eq!(
            fit_page(&transfer, &context, &request_id, bytes).unwrap(),
            (page, bytes)
        );
        assert_eq!(
            fit_page(&transfer, &context, &request_id, bytes - 1)
                .unwrap_err()
                .code,
            ErrorCode::ResourceLimit,
            "even an empty result must fit its complete escaped envelope",
        );
        transfer.page = MAX_MAP_TRANSFER_PAGES as u16;
        assert_eq!(
            fit_page(&transfer, &context, &request_id, 4096)
                .unwrap_err()
                .code,
            ErrorCode::ResourceLimit,
        );
    }
}

#[test]
fn exact_aggregate_transfer_ceiling_admits_the_terminal_page() {
    let (mut service, mut client) = service(MAX_MAP_PAGE_BYTES);
    let mut transfer = synthetic_transfer(&service, client.id, 1);
    let context = service.reply_context(client.id).unwrap();
    let (_, bytes) = fit_page(&transfer, &context, "exact", MAX_MAP_PAGE_BYTES).unwrap();
    transfer.bytes = MAX_MAP_TRANSFER_BYTES - bytes;
    service
        .publish_map_page(client.id, "exact", transfer)
        .unwrap();
    let messages: Vec<_> = std::iter::from_fn(|| client.messages.try_recv().ok()).collect();
    assert!(
        matches!(messages.as_slice(), [ServerMessage::MapPage { page, .. }]
        if page.next.is_none() && page.cells.len() == 1)
    );
    assert!(service.clients[&client.id].map_transfer.is_none());
}
