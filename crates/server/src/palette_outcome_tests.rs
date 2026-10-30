//! Palette forecasts bracket event-time appearances across region transitions.
use super::*;

const OLD: &str = "creature.entry-guard";
const NEW: &str = "creature.vault-guard";

fn fixture() -> (tempfile::TempDir, Service, Connection, BTreeSet<String>) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../scenarios/tests/generated-filler");
    let package = crate::scenario_package::load(&root, 5, None, false)
        .unwrap()
        .package
        .unwrap();
    let mut manifest = package.manifest.clone();
    let mut regions = package.region_defs().unwrap();
    // Archetypes are dual-use: concealed item instances forecast only this
    // shared pool asset, while actors use their region-authorized actor asset.
    // This is the same distinction covered by the palette validation tests.
    manifest.appearance_pools.insert(
        "guard_fur".into(),
        crate::scenario_package::AppearancePool {
            appearances: vec!["grey fur".into(), "black fur".into()],
            confounding: false,
            asset: Some("creature.rat".into()),
        },
    );
    for (theme, name, asset, region, actor) in [
        ("stone", "entry_guard", OLD, 0, 2),
        ("vault", "vault_guard", NEW, 3, 3),
    ] {
        let mut archetype = manifest.archetypes["rat"].clone();
        archetype.asset = Some(asset.into());
        archetype.name = Some(name.into());
        archetype.appearance_pool = Some("guard_fur".into());
        manifest.archetypes.insert(name.into(), archetype);
        manifest.assets.get_mut(theme).unwrap().push(asset.into());
        regions[region].actors.push(
            toml::from_str(&format!(
                "id = {actor}\nat = [3, 2, 0]\narchetype = '{name}'\n"
            ))
            .unwrap(),
        );
    }
    let directory = tempfile::tempdir().unwrap();
    crate::scenario_package::write_package(
        directory.path(),
        &manifest,
        &regions,
        &package.rules_source,
    )
    .unwrap();
    let mut scenario = crate::scenario_package::load(directory.path(), 5, None, true).unwrap();
    scenario.streaming = Some(crate::Streaming {
        active_radius: 0,
        load_radius: 0,
    });
    let mut engine = Engine::memory(scenario).unwrap();
    engine.enable_wizard().unwrap();
    let mut service = Service::new(engine);
    let account = Account {
        role: AccessRole::Spectator,
        user: "observer".into(),
        token: "test".into(),
        actors: BTreeSet::from([ActorId(1)]),
    };
    let mut client = service.connect(&account, "text".into()).unwrap();
    service.handle(
        client.id,
        "attach".into(),
        Request::Attach { actor: ActorId(1) },
    );
    while client.messages.try_recv().is_ok() {}
    let assets = service.clients[&client.id]
        .palette
        .as_ref()
        .unwrap()
        .1
        .clone();
    assert!(assets.contains(OLD));
    assert!(!assets.contains(NEW));
    assert!(service
        .engine
        .state(ActorId(1))
        .unwrap()
        .observation
        .visible_actors
        .iter()
        .any(|actor| actor.asset.as_deref() == Some(OLD)));
    (directory, service, client, assets)
}

fn cross(service: &mut Service) {
    let before = service.semantic_states().unwrap();
    let result = service
        .engine
        .command(
            "backend",
            "test",
            ActorId(1),
            "cross-region",
            &service.engine.branch().clone(),
            crate::journal::Command::Wizard {
                display_basis: None,
                expected_boundary: service.engine.causal_boundary(ActorId(1)).unwrap(),
                operation: crate::journal::WizardOperation::Teleport {
                    actor: ActorId(1),
                    position: crate::journal::Position {
                        region: 4,
                        x: 2,
                        y: 2,
                        z: 0,
                    },
                },
            },
        )
        .unwrap();
    let forecast = service.engine.palette(ActorId(1)).unwrap();
    assert!(forecast.contains(NEW));
    assert!(!forecast.contains(OLD));
    service.action_result_update(&before, &result).unwrap();
}

#[test]
fn region_palette_additions_precede_outcome_and_removals_follow_old_sightings() {
    let (_directory, mut service, mut client, mut held) = fixture();
    cross(&mut service);
    let mut outcomes = 0;
    let mut palette_updates = 0;
    while let Ok(message) = client.messages.try_recv() {
        match message {
            ServerMessage::Palette { palette, .. } => {
                let PaletteBody::Delta { added, removed, .. } = palette.body else {
                    panic!("delta")
                };
                if outcomes == 0 {
                    assert!(removed.is_empty());
                }
                for asset in removed {
                    held.remove(&asset);
                }
                held.extend(added);
                palette_updates += 1;
            }
            ServerMessage::Update { update } => {
                let UpdateBody::Outcome { facts, .. } = update.body else {
                    continue;
                };
                outcomes += 1;
                assert!(
                    held.contains(NEW),
                    "new asset metadata precedes dependent outcome"
                );
                assert!(
                    held.contains(OLD),
                    "old asset remains authorized through LostSight"
                );
                assert!(facts.facts.iter().any(|fact| matches!(fact,
                    OutcomeFact::Noticed { asset: Some(asset), .. } if asset == NEW)));
                assert!(facts.facts.iter().any(|fact| matches!(fact,
                    OutcomeFact::LostSight { asset: Some(asset), .. } if asset == OLD)));
            }
            _ => {}
        }
    }
    assert_eq!(outcomes, 1);
    assert_eq!(palette_updates, 2);
    assert!(!held.contains(OLD));
    assert!(held.contains(NEW));
    assert_eq!(held, service.engine.palette(ActorId(1)).unwrap());
}

#[test]
fn palette_transition_output_failures_close_stream_without_unknown_asset_outcome() {
    for free_slots in [0, 2] {
        let (_directory, mut service, mut client, mut held) = fixture();
        let sender = &service.clients[&client.id].messages;
        let mut filled = 0;
        while sender
            .try_send(ServerMessage::Waiting {
                on: Waiting::Others,
            })
            .is_ok()
        {
            filled += 1;
        }
        assert!(filled > free_slots);
        for _ in 0..free_slots {
            client.messages.try_recv().unwrap();
        }
        cross(&mut service);
        assert!(!service.clients.contains_key(&client.id));
        assert_eq!(*client.close.borrow(), Some(DisconnectMode::Abort));
        let mut outcomes = 0;
        while let Ok(message) = client.messages.try_recv() {
            match message {
                ServerMessage::Palette { palette, .. } => {
                    let PaletteBody::Delta { added, removed, .. } = palette.body else {
                        panic!("delta")
                    };
                    for asset in removed {
                        held.remove(&asset);
                    }
                    held.extend(added);
                }
                ServerMessage::Update { update }
                    if matches!(update.body, UpdateBody::Outcome { .. }) =>
                {
                    outcomes += 1;
                    assert!(held.contains(OLD) && held.contains(NEW));
                }
                _ => {}
            }
        }
        assert_eq!(outcomes, usize::from(free_slots == 2));
    }
}
