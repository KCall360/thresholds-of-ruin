//! Ordinary admitted self-device delivery, immutable preparation and source-free replay.
use super::*;

fn nothing_source(root: &Path, with_other: bool) -> crate::Scenario {
    self_device_source(root, with_other, false)
}

fn self_device_source(root: &Path, with_other: bool, condition: bool) -> crate::Scenario {
    use tor_simulation::{
        abilities::InvocationKind,
        rules::{Interaction, Reach, StrikeSource},
    };
    let base = scenario(with_other);
    let source = base.package.as_ref().unwrap();
    let mut document = source.rules().unwrap().document().clone();
    let ability = document
        .abilities
        .iter_mut()
        .find(|a| a.id.as_str() == "magic.ember_ray")
        .unwrap();
    ability.invocation = InvocationKind::ActorTarget;
    ability.interaction = Interaction::NonHostile;
    ability.reach = Reach::SelfOnly;
    ability.ai.hostile = false;
    ability.tags.clear();
    let StrikeSource::Authored { strikes } = &mut ability.strikes else {
        panic!("authored fixture")
    };
    assert_eq!(strikes.len(), 1);
    strikes[0].check = None;
    strikes[0].on_success.clear();
    strikes[0].on_failure.clear();
    strikes[0].descriptors.clear();
    strikes[0].riders.clear();
    if condition {
        strikes[0]
            .on_success
            .push(tor_simulation::rules::Effect::ApplyCondition {
                condition: tor_simulation::content_ids::ConditionId::new("fixture.self_device")
                    .unwrap(),
                duration: 2000,
            });
        document.conditions.push(
            serde_json::from_value(json!({
                "id":"fixture.self_device", "family":"fixture.self_device", "lifecycle":"timed",
                "name":"Self device", "modifiers":[], "stacking":{"kind":"keep"},
                "source_scoped":false, "descriptors":[], "capacity":"reject",
                "timing":{"kind":"actor_active","on_expiry":{"kind":"remove"}},
                "restrictions":[], "preparation":"preserve"
            }))
            .unwrap(),
        );
    }
    assert_eq!(document.constants.item_timing.consume_turns, 1);
    // Preserve original valid timing100/100, cost-free source and real compiler.
    let rules = tor_simulation::rules::Ruleset::validate(document).unwrap();
    let validated = rules
        .document()
        .abilities
        .iter()
        .find(|a| a.id.as_str() == "magic.ember_ray")
        .unwrap();
    assert!(
        validated.cost.is_none()
            && validated.cooldown.is_none()
            && validated.supply_costs.is_empty()
    );
    let rules = toml::to_string(rules.document()).unwrap();
    let mut manifest = serde_json::to_value(&source.manifest).unwrap();
    manifest["archetypes"]["ember"]["name"] = json!("wand of nothing");
    manifest["archetypes"]["ember"]["identity"] = json!("fixture.nothing");
    manifest["archetypes"]["ember"]["device"]["discovery"] = json!("never");
    let manifest = serde_json::from_value(manifest).unwrap();
    let regions = source.region_defs().unwrap();
    let checked = Package::from_parts(manifest, regions, &rules).unwrap();
    checked.check().unwrap();
    // Use the ordinary source writer so deletion below is real filesystem
    // absence, not only replacement of an in-memory Scenario argument.
    crate::scenario_package::write_package(
        root,
        &checked.manifest,
        &checked.region_defs().unwrap(),
        &rules,
    )
    .unwrap();
    crate::scenario_package::check(root).unwrap();
    crate::scenario_package::load(root, 42, None, true).unwrap()
}

fn nothing_actor(saved: &Value, actor: ActorId) -> &Value {
    let index = saved["game"]["actors"].as_u64().unwrap() as usize;
    &saved["shared"]["actors"][index][actor.0.to_string()]
}
fn nothing_items(saved: &Value) -> &serde_json::Map<String, Value> {
    let index = saved["game"]["items"].as_u64().unwrap() as usize;
    saved["shared"]["items"][index].as_object().unwrap()
}
fn nothing_wand_id(wire: &Wire, item: ItemTarget) -> String {
    match wire
        .service
        .engine
        .decode_action(
            OWNER,
            &Action::Zap {
                item,
                aim: DeviceAim::Direction {
                    direction: Direction::East,
                },
            },
        )
        .unwrap()
    {
        crate::actions::Action::Zap { item, .. } => item.to_string(),
        _ => panic!("resolved ordinary item handle"),
    }
}
fn nothing_affordance(snapshot: &Snapshot, item: ItemTarget) -> &ItemInteractionView {
    snapshot
        .state
        .observation
        .interactions
        .as_ref()
        .unwrap()
        .inventory
        .iter()
        .find(|r| r.item == item)
        .unwrap()
}
fn nothing_identified(snapshot: &Snapshot, item: ItemTarget) -> bool {
    snapshot
        .state
        .observation
        .inventory
        .iter()
        .find(|r| r.id == item)
        .unwrap()
        .identified
}

#[test]
fn paid_nothing_direction_and_known_self_spend_once_without_rng_or_identification() {
    for (known, aim) in [
        (
            false,
            DeviceAim::Direction {
                direction: Direction::East,
            },
        ),
        (
            false,
            DeviceAim::Direction {
                direction: Direction::West,
            },
        ),
        (
            true,
            DeviceAim::Direction {
                direction: Direction::East,
            },
        ),
        (true, DeviceAim::SelfTarget {}),
    ] {
        let source = tempfile::tempdir().unwrap();
        let mut wire = Wire::new(Engine::memory(nothing_source(source.path(), false)).unwrap());
        let wand = wire.take(ItemClass::Wand);
        let physical = nothing_wand_id(&wire, wand);
        let unknown = wire.snapshot(OWNER);
        assert!(!nothing_identified(&unknown, wand));
        assert_eq!(
            nothing_affordance(&unknown, wand).device_activation,
            Some(DeviceActivation::Direction)
        );
        let invalid = wire.request(
            OWNER,
            Action::Zap {
                item: wand,
                aim: DeviceAim::SelfTarget {},
            },
        );
        let untouched = wire.checkpoint();
        rejected(wire.send(OWNER, "unknown-self", invalid));
        assert_eq!(wire.checkpoint(), untouched);
        if known {
            let scroll = wire.take(ItemClass::Scroll);
            wire.act("read-identify", Action::Read { item: scroll });
            let paid = choice(&wire.snapshot(OWNER));
            wire.act(
                "identify",
                Action::Identify {
                    choice: paid.choice,
                    item: Some(wand),
                },
            );
            assert_eq!(
                nothing_affordance(&wire.snapshot(OWNER), wand).device_activation,
                Some(DeviceActivation::SelfTarget)
            );
        }
        let request = wire.request(OWNER, Action::Zap { item: wand, aim });
        let before = wire.checkpoint();
        assert_eq!(nothing_actor(&before, OWNER)["turn_ticks"], 100);
        assert_eq!(nothing_items(&before)[&physical]["charges"], 3);
        let admitted = ack(wire.send(OWNER, "nothing-once", request.clone()));
        let completed = wire.finish(OWNER, "nothing-once", &request);
        assert_eq!(admitted.entry_id(), completed.entry_id());
        let after = wire.checkpoint();
        assert_eq!(nothing_items(&after)[&physical]["charges"], 2);
        assert_eq!(after["game"]["rules_rng"], before["game"]["rules_rng"]);
        // Existing source has speed0, turn100, consume_turns1; only one owner.
        assert_eq!(
            after["game"]["tick"].as_u64().unwrap(),
            before["game"]["tick"].as_u64().unwrap() + 100
        );
        let view = wire.snapshot(OWNER);
        assert_eq!(nothing_identified(&view, wand), known);
        assert_eq!(nothing_affordance(&view, wand).charges, known.then_some(2));
        assert_eq!(ack(wire.send(OWNER, "nothing-once", request)), completed);
        assert_eq!(wire.checkpoint(), after);
    }
}

#[test]
fn paused_nothing_preserves_direction_and_payload_across_identification_and_deleted_source_reopen()
{
    for interval in [1, 4096] {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("source");
        let path = directory.path().join("nothing.db");
        let policy = crate::SavePolicy {
            checkpoint_interval: interval,
            ..Default::default()
        };
        let mut wire = Wire::new(
            Engine::open_with_policy(&path, nothing_source(&root, true), policy.clone()).unwrap(),
        );
        let scroll = wire.take(ItemClass::Scroll);
        let wand = wire.take(ItemClass::Wand);
        let physical = nothing_wand_id(&wire, wand);
        // Pay for a genuine outstanding selection before starting the unknown
        // device. Identify itself is zero-time, not an injected knowledge edit.
        wire.act("pay-identify", Action::Read { item: scroll });
        let selection = choice(&wire.snapshot(OWNER));
        let request = wire.request(
            OWNER,
            Action::Zap {
                item: wand,
                aim: DeviceAim::Direction {
                    direction: Direction::West,
                },
            },
        );
        let admitted = ack(wire.send(OWNER, "frozen-nothing", request.clone()));
        wire.service.run_until_blocked();
        wire.drain(OWNER);
        wire.drain(OTHER);
        let started = ack(wire.send(OWNER, "frozen-nothing", request.clone()));
        assert!(matches!(
            started,
            RequestReceipt::Admitted {
                phase: IntentionPhase::Started,
                ..
            }
        ));
        assert_eq!(started.entry_id(), admitted.entry_id());
        let pending = wire.checkpoint();
        let previous = nothing_actor(&pending, OWNER);
        assert_eq!(previous["pending"]["active"], true);
        assert_eq!(previous["pending"]["remaining"], 100);
        let preparation: tor_simulation::combat::Preparation =
            serde_json::from_value(previous["pending"].clone()).unwrap();
        // Public typed equivalent of has_prefix: only an Ability next_strike>0
        // is committed. Do not infer prefix absence from a missing JSON field.
        match &preparation.progress {
            tor_simulation::combat::PreparationProgress::Ability { next_strike, .. } => {
                assert_eq!(*next_strike, 0)
            }
            tor_simulation::combat::PreparationProgress::Item {}
            | tor_simulation::combat::PreparationProgress::Ray {} => {}
            other => panic!("unexpected device progress: {other:?}"),
        }
        assert_eq!(nothing_items(&pending)[&physical]["charges"], 3);
        let frozen = &previous["pending"]["plan"];
        assert_eq!(
            frozen["aim"],
            json!({"kind":"direction","direction":"west"})
        );
        assert_eq!(frozen["delivery"]["kind"], "self_effect");
        assert_eq!(frozen["delivery"]["plan"]["actor"], OWNER.0);
        assert_eq!(frozen["delivery"]["plan"]["ability"], "magic.ember_ray");
        assert_eq!(frozen["delivery"]["plan"]["effects"], json!([]));
        let plan: tor_simulation::abilities::Plan = serde_json::from_value(frozen.clone()).unwrap();
        assert_eq!(
            plan.work(),
            tor_simulation::Work::Zap {
                item: tor_simulation::ItemId(physical.parse().unwrap()),
                aim: tor_simulation::DeviceAim::Direction {
                    direction: tor_simulation::abilities::StepDirection::West
                },
            }
        );
        // Freeze at an actual started boundary, then exercise the supported
        // control-release pause rather than pretending queue admission is work.
        wire.send(OWNER, "pause", Request::ReleaseControl);
        ack(wire.send(OWNER, "reacquire", Request::AcquireControl));
        let paused = wire.checkpoint();
        assert_eq!(nothing_actor(&paused, OWNER)["pending"]["active"], false);
        assert_eq!(
            nothing_actor(&paused, OWNER)["pending"]["plan"],
            previous["pending"]["plan"]
        );
        assert_eq!(
            nothing_actor(&paused, OWNER)["pending"]["progress"],
            previous["pending"]["progress"]
        );
        assert_eq!(paused["game"]["tick"], pending["game"]["tick"]);
        assert_eq!(
            wire.service.engine.next_actor(),
            Some(OWNER),
            "owner must actually be executable, not only admit input"
        );
        let ready = wire.snapshot(OWNER);
        assert!(ready.has_control && ready.readiness.admission);
        assert_eq!(ready.readiness.resume.len(), 1);
        assert!(
            nothing_actor(&paused, OWNER)["ready_at"].as_u64().unwrap()
                <= paused["game"]["tick"].as_u64().unwrap()
        );
        let identify = wire.request(
            OWNER,
            Action::Identify {
                choice: selection.choice,
                item: Some(wand),
            },
        );
        ack(wire.send(OWNER, "identify-during-pause", identify.clone()));
        // Unlike Wire::finish, this boundary has no helper-generated peer Wait.
        // A setup ordering mistake must fail here, not advance time to hide it.
        wire.service.run_until_blocked();
        wire.drain(OWNER);
        wire.drain(OTHER);
        let identified = ack(wire.send(OWNER, "identify-during-pause", identify));
        assert!(matches!(
            identified,
            RequestReceipt::Admitted {
                phase: IntentionPhase::Resolved,
                ..
            }
        ));
        let learned = wire.checkpoint();
        assert_eq!(
            nothing_actor(&learned, OWNER)["pending"]["plan"],
            previous["pending"]["plan"]
        );
        assert_eq!(
            nothing_actor(&learned, OWNER)["pending"]["remaining"],
            previous["pending"]["remaining"]
        );
        assert_eq!(learned["game"]["tick"], pending["game"]["tick"]);
        assert_eq!(learned["game"]["rules_rng"], pending["game"]["rules_rng"]);
        assert_eq!(
            nothing_affordance(&wire.snapshot(OWNER), wand).device_activation,
            Some(DeviceActivation::SelfTarget)
        );
        wire.service.engine.flush().unwrap();
        drop(wire);
        std::fs::remove_dir_all(&root).unwrap();
        assert!(!root.exists());
        let engine =
            Engine::open_with_policy(&path, crate::Scenario::two_room(999), policy).unwrap();
        assert_eq!(
            checkpoint(&engine),
            learned,
            "restore frozen facts before Service attachment"
        );
        if interval == 4096 {
            assert!(engine.recovery_profile().records_replayed > 0);
        }
        let mut wire = Wire::new(engine); // exactly two real external owners, no AI conversion
        let snapshot = wire.snapshot(OWNER);
        assert_eq!(snapshot.readiness.resume.len(), 1);
        let resume = Request::Command {
            context: snapshot.reply_context().input,
            branch: snapshot.branch.clone(),
            command: Command::ResumeIntention {
                expected_revision: snapshot.state.revision,
                intention: snapshot.readiness.resume[0].clone(),
            },
        };
        ack(wire.send(OWNER, "resume-captured", resume));
        let completed = wire.finish(OWNER, "frozen-nothing", &request);
        assert_eq!(completed.entry_id(), admitted.entry_id());
        let after = wire.checkpoint();
        assert_eq!(nothing_items(&after)[&physical]["charges"], 2);
        assert_eq!(after["game"]["rules_rng"], pending["game"]["rules_rng"]);
        assert_eq!(
            nothing_affordance(&wire.snapshot(OWNER), wand).charges,
            Some(2)
        );
        assert_eq!(ack(wire.send(OWNER, "frozen-nothing", request)), completed);
        assert_eq!(wire.checkpoint(), after);
    }
}

#[test]
fn followed_spectator_uses_followed_identity_for_device_prompt_not_owner_only_gate() {
    let source = tempfile::tempdir().unwrap();
    let mut wire = Wire::new(Engine::memory(nothing_source(source.path(), false)).unwrap());
    let wand = wire.take(ItemClass::Wand);
    let mut spectator = wire
        .service
        .connect(
            &Account {
                user: "nothing-follower".into(),
                token: "fixture".into(),
                role: AccessRole::Spectator,
                actors: BTreeSet::from([OWNER]),
            },
            "nothing-follower".into(),
        )
        .unwrap();
    spectator.messages.try_recv_frame().unwrap();
    wire.service.handle(
        spectator.id,
        "follow".into(),
        Request::Attach { actor: OWNER },
    );
    let response = decode_response(&spectator.messages.try_recv_frame().unwrap().text).unwrap();
    let ServerMessage::Snapshot { snapshot, .. } = response else {
        panic!("follow snapshot")
    };
    assert!(!nothing_identified(&snapshot, wand));
    assert_eq!(
        nothing_affordance(&snapshot, wand).device_activation,
        Some(DeviceActivation::Direction)
    );
    let scroll = wire.take(ItemClass::Scroll);
    wire.act("read-for-follow", Action::Read { item: scroll });
    let paid = choice(&wire.snapshot(OWNER));
    wire.act(
        "learn-for-follow",
        Action::Identify {
            choice: paid.choice,
            item: Some(wand),
        },
    );
    while spectator.messages.try_recv_frame().is_ok() {}
    wire.service.handle(
        spectator.id,
        "known-follow-snapshot".into(),
        Request::Snapshot,
    );
    let response = decode_response(&spectator.messages.try_recv_frame().unwrap().text).unwrap();
    let ServerMessage::Snapshot { snapshot, .. } = response else {
        panic!("follow snapshot")
    };
    assert!(nothing_identified(&snapshot, wand));
    assert_eq!(
        nothing_affordance(&snapshot, wand).device_activation,
        Some(DeviceActivation::SelfTarget)
    );
    assert_eq!(
        nothing_affordance(&snapshot, wand).charges,
        None,
        "separate exact charge privacy remains"
    );
    assert!(!snapshot.has_control);
}

#[test]
fn paid_self_device_applies_nonempty_captured_condition_to_user_not_direction_recipient() {
    let source = tempfile::tempdir().unwrap();
    let mut wire =
        Wire::new(Engine::memory(self_device_source(source.path(), true, true)).unwrap());
    let wand = wire.take(ItemClass::Wand);
    let physical = nothing_wand_id(&wire, wand);
    let before = wire.checkpoint();
    for actor in [OWNER, OTHER] {
        let view = wire.snapshot(actor);
        assert!(!view
            .state
            .observation
            .combat
            .as_ref()
            .unwrap()
            .own_stats
            .as_ref()
            .unwrap()
            .conditions
            .iter()
            .any(|row| row.condition.id == "fixture.self_device"));
    }
    assert!(!nothing_identified(&wire.snapshot(OWNER), wand));
    let request = wire.request(
        OWNER,
        Action::Zap {
            item: wand,
            aim: DeviceAim::Direction {
                direction: Direction::East,
            },
        },
    );
    let admitted = ack(wire.send(OWNER, "self-condition", request.clone()));
    let completed = wire.finish(OWNER, "self-condition", &request);
    assert_eq!(completed.entry_id(), admitted.entry_id());
    let owner = wire.snapshot(OWNER);
    let own = &owner
        .state
        .observation
        .combat
        .as_ref()
        .unwrap()
        .own_stats
        .as_ref()
        .unwrap()
        .conditions;
    let matches: Vec<_> = own
        .iter()
        .filter(|row| row.condition.id == "fixture.self_device")
        .collect();
    let [applied] = matches.as_slice() else {
        panic!("exactly one condition on the actual device user")
    };
    assert_eq!(applied.phase, ConditionPhase::Active);
    assert_eq!(applied.remaining_ticks, Some(2000));
    let other = wire.snapshot(OTHER);
    assert!(!other
        .state
        .observation
        .combat
        .as_ref()
        .unwrap()
        .own_stats
        .as_ref()
        .unwrap()
        .conditions
        .iter()
        .any(|row| row.condition.id == "fixture.self_device"));
    let after = wire.checkpoint();
    assert_eq!(nothing_items(&after)[&physical]["charges"], 2);
    assert_eq!(after["game"]["rules_rng"], before["game"]["rules_rng"]);
    assert!(
        !nothing_identified(&owner, wand),
        "Never discovery remains independent of an observable condition"
    );
    assert_eq!(ack(wire.send(OWNER, "self-condition", request)), completed);
    assert_eq!(wire.checkpoint(), after);
}
