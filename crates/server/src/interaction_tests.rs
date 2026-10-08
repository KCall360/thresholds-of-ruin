use super::*;

fn fixture() -> Engine {
    use crate::scenario_package as author;
    let mut manifest: author::Manifest =
        toml::from_str(include_str!("../../../scenarios/tests/items/scenario.toml")).unwrap();
    manifest.characters.truncate(1);
    manifest.characters[0].combat = Some(author::CombatSpec::default());
    manifest.characters[0].anatomy = Some(author::AnatomySpec {
        slots: vec![author::EquipmentSlot::BodyArmor],
    });
    manifest.archetypes.insert(
        "mail".into(),
        toml::from_str(
            "class = 'armor'\nname = 'mail'\nequipment = { slot = 'body_armor', defense = 3 }",
        )
        .unwrap(),
    );
    for name in ["healing", "poison"] {
        manifest.archetypes.get_mut(name).unwrap().consumable = Some(author::ConsumableSpec {
            effects: vec![author::EffectSpec::Heal { amount: 4 }],
        });
    }
    let mut region: author::RegionDef = toml::from_str(include_str!(
        "../../../scenarios/tests/items/regions/1.toml"
    ))
    .unwrap();
    region.items = vec![
        toml::from_str("id = 10\nat = [1,1,0]\narchetype = 'mail'\ncarried_by = 1").unwrap(),
        toml::from_str("id = 20\nat = [1,1,0]\narchetype = 'healing'\ncarried_by = 1").unwrap(),
    ];
    region
        .actors
        .push(toml::from_str("id = 2\nat = [2,1,0]\nturn_ticks = 50").unwrap());
    region
        .items
        .push(toml::from_str("id = 21\nat = [1,1,0]\narchetype = 'poison'").unwrap());
    let package = author::Package::from_parts(manifest, vec![region]).unwrap();
    let mut scenario = Scenario::two_room(42);
    scenario.actors.clear();
    scenario.regions = 1;
    scenario.package = Some(Arc::new(package));
    Engine::memory(scenario).unwrap()
}

fn verify(engine: &Engine) {
    let replay = Engine::replay(engine.archive.clone(), None, None).unwrap();
    let checkpoint = Checkpoint::capture(engine)
        .encode("item-work", 1)
        .restore(engine.archive.clone())
        .unwrap();
    for restored in [&replay, &checkpoint] {
        assert_eq!(restored.game, engine.game);
        assert_eq!(
            restored.pending_intentions(ActorId(1)),
            engine.pending_intentions(ActorId(1))
        );
    }
}

fn command(engine: &mut Engine, actor: ActorId, request: &str, command: Command) -> CommandResult {
    engine
        .command(
            "player",
            "test",
            actor,
            request,
            &engine.branch().clone(),
            command,
        )
        .unwrap()
}

#[test]
fn item_preparation_preserves_admission_through_pause_resume_replay_and_completion() {
    let mut engine = fixture();
    let actor = ActorId(1);
    let revision = engine.revision(actor).unwrap();
    let admitted = command(
        &mut engine,
        actor,
        "equip",
        Command::AdmitIntention {
            expected_revision: revision,
            action: Action::Equip { item: 10, slot: 0 },
        },
    );
    engine.execute_next_intention().unwrap().unwrap();
    assert_eq!(
        engine.pending_intentions(actor)[0].phase,
        IntentionPhase::Started
    );
    verify(&engine);
    engine.pause_preparation(actor).unwrap().unwrap();
    assert_eq!(
        engine.pending_intentions(actor)[0].phase,
        IntentionPhase::Paused
    );
    verify(&engine);
    let before = engine.game.clone();
    let revision = engine.revision(actor).unwrap();
    assert!(engine
        .command(
            "player",
            "test",
            actor,
            "cancel",
            &engine.branch().clone(),
            Command::CancelIntention {
                expected_revision: revision,
                admission: admitted.entry.id.clone()
            }
        )
        .is_err());
    assert_eq!(engine.game, before);
    command(
        &mut engine,
        actor,
        "resume",
        Command::ResumeIntention {
            expected_revision: revision,
            admission: admitted.entry.id.clone(),
        },
    );
    verify(&engine);
    for step in 0..20 {
        if engine.game.preparation(SimActor(1)).is_none() {
            break;
        }
        if engine.game.next_intention_actor().is_some() {
            engine.execute_next_intention().unwrap().unwrap();
        } else {
            let next = engine.next_actor().unwrap();
            let revision = engine.revision(next).unwrap();
            command(
                &mut engine,
                next,
                &format!("wait-{step}"),
                Command::Act {
                    expected_revision: revision,
                    action: Action::Wait,
                },
            );
        }
        verify(&engine);
    }
    assert_eq!(
        engine.game.equipment(SimActor(1)).unwrap()[&tor_simulation::EquipmentSlotId(0)],
        tor_simulation::ItemId(10)
    );
    assert!(matches!(
        engine.request_receipt(&admitted),
        RequestReceipt::Admitted {
            phase: IntentionPhase::Resolved,
            ..
        }
    ));
}

#[test]
fn item_actions_use_observer_scoped_inventory_targets_and_reject_ground_targets() {
    let engine = fixture();
    let actor = ActorId(1);
    let action = Action::Drink { item: 20 };
    let wire = engine.encode_action(actor, &action);
    assert_eq!(engine.decode_action(actor, &wire).unwrap(), action);
    assert!(engine.decode_action(ActorId(2), &wire).is_err());
    let foreign = engine.encode_action(ActorId(2), &action);
    assert!(engine.decode_action(actor, &foreign).is_err());
    let missing = engine.encode_action(actor, &Action::Drink { item: 999 });
    assert!(engine.decode_action(actor, &missing).is_err());
    let ground = engine.encode_action(actor, &Action::Drink { item: 21 });
    assert!(engine.decode_action(actor, &ground).is_err());
}
