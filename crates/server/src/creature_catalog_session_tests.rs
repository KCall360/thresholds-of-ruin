//! Catalog pagination honors the configured whole response envelope.
use super::*;
use crate::scenario_package::{Manifest, Package, RegionDef};
use serde_json::Value;

#[test]
fn creature_catalog_pages_fit_4096_byte_transport_and_preserve_query_state() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../scenarios/tests/generated-filler");
    let original = crate::scenario_package::resolve_source(&root).unwrap();
    let mut manifest: Value = serde_json::to_value(&original.manifest).unwrap();
    manifest["characters"][0]["creature"] = Value::Null;
    for index in 0..40 {
        manifest["archetypes"][format!("rat_{index:02}")] = manifest["archetypes"]["rat"].clone();
    }
    let region: RegionDef = toml::from_str(
        "id = 1\nname = 'Catalog room'\nsize = [3,3,1]\nanchors = {start=[1,1,0]}\n",
    )
    .unwrap();
    let package = Package::from_parts(
        serde_json::from_value::<Manifest>(manifest).unwrap(),
        vec![region],
        tor_simulation::rules::DEFAULT_RULESET_TOML,
    )
    .unwrap();
    let mut engine = Engine::memory(crate::Scenario {
        actors: vec![],
        package: Some(Arc::new(package)),
        streaming: Some(crate::Streaming::default()),
        ..crate::Scenario::two_room(42)
    })
    .unwrap();
    engine.enable_wizard().unwrap();
    let catalog = engine
        .creature_catalog_query(
            ActorId(1),
            engine.branch(),
            engine.revision(ActorId(1)).unwrap(),
            None,
            None,
        )
        .unwrap();
    assert_eq!(
        catalog.entries.len(),
        32,
        "positive compiled catalog control"
    );
    assert!(catalog.next.is_some());
    let mut service = Service::with_outbound_limits(
        engine,
        crate::OutboundLimits {
            frame_bytes: 4096,
            client_bytes: 65_536,
            total_bytes: 262_144,
        },
    )
    .unwrap();
    let mut client = service
        .connect(
            &Account {
                role: AccessRole::Wizard,
                user: "wizard".into(),
                token: "test".into(),
                actors: BTreeSet::from([ActorId(1)]),
            },
            "headless".into(),
        )
        .unwrap();
    while client.messages.try_recv_frame().is_ok() {}
    service.handle(
        client.id,
        "attach".into(),
        Request::Attach { actor: ActorId(1) },
    );
    while client.messages.try_recv_frame().is_ok() {}
    assert!(
        service.clients.contains_key(&client.id),
        "fixture attaches under transport limit"
    );
    let before = service.engine.state(ActorId(1)).unwrap();
    let history = service
        .engine
        .history(ActorId(1), "wizard", None, 100)
        .unwrap();
    let mut after = None;
    let mut selectors = BTreeSet::new();
    for _ in 0..42 {
        let operation = after.as_ref().map_or_else(
            || "creature list".into(),
            |after| format!("creature list after {after}"),
        );
        service.handle(
            client.id,
            "\\".repeat(128),
            Request::Command {
                context: service.input_context(client.id).unwrap(),
                branch: service.engine.branch().clone(),
                command: Command::Wizard {
                    expected_revision: before.revision,
                    operation,
                },
            },
        );
        assert!(
            service.clients.contains_key(&client.id),
            "catalog must paginate, never disconnect"
        );
        let mut reply = None;
        while let Ok(frame) = client.messages.try_recv_frame() {
            assert!(frame.text.len() <= 4096);
            if let ServerMessage::CreatureCatalog { page, .. } =
                decode_response(&frame.text).unwrap()
            {
                reply = Some(page);
            }
        }
        let page = reply.expect("private bounded catalog page");
        assert!(!page.entries.is_empty());
        assert!(page.entries.len() <= 32);
        for row in &page.entries {
            assert!(selectors.insert(row.selector.clone()));
        }
        assert_eq!(service.engine.state(ActorId(1)).unwrap(), before);
        assert_eq!(
            service
                .engine
                .history(ActorId(1), "wizard", None, 100)
                .unwrap(),
            history
        );
        after = page.next.clone();
        if after.is_none() {
            break;
        }
    }
    assert!(after.is_none());
    assert_eq!(selectors.len(), 41);
}
