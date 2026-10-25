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
        let clock_watermark = wire.serial;
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
                                          // Request receipts are durable across reconnect; keep the existing
                                          // caller's sequence instead of reusing clock-1 for different work.
        wire.serial = clock_watermark;
        assert!(wire.serial.checked_add(1).unwrap() > clock_watermark);
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
// OnUse discovery is completion-owned, while another actor retains independent
// knowledge of its own device with the same identity.
#[test]
fn on_use_self_condition_identifies_only_on_completion_and_other_owner_remains_unknown() {
    let root = tempfile::tempdir().unwrap();
    let base = self_device_source(root.path(), true, true);
    let package = base.package.as_ref().unwrap();
    let mut manifest = serde_json::to_value(&package.manifest).unwrap();
    manifest["archetypes"]["ember"]["device"]["discovery"] = json!("on_use");
    let mut regions = package.region_defs().unwrap();
    let mut region = serde_json::to_value(&regions[0]).unwrap();
    // Existing second device, same actual identity, another independently
    // controlled actor. Use supported authored carriage, not inventory injection.
    let peer_item = region["items"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|row| row["id"] == 31)
        .unwrap();
    peer_item["carried_by"] = json!(OTHER.0);
    peer_item["at"] = json!([5, 2, 0]);
    regions[0] = serde_json::from_value(region).unwrap();
    let rules = toml::to_string(package.rules().unwrap().document()).unwrap();
    let package =
        Package::from_parts(serde_json::from_value(manifest).unwrap(), regions, &rules).unwrap();
    package.check().unwrap();
    crate::scenario_package::write_package(
        root.path(),
        &package.manifest,
        &package.region_defs().unwrap(),
        &rules,
    )
    .unwrap();
    crate::scenario_package::check(root.path()).unwrap();
    let scenario = crate::scenario_package::load(root.path(), 42, None, true).unwrap();
    let mut wire = Wire::new(Engine::memory(scenario).unwrap());
    let wand = wire.take(ItemClass::Wand);
    // Pickup resolves immediately for the owner, but the other external actor
    // is still due at tick0. Advance that real actor once before measuring the
    // zap; admission readiness alone does not imply execution readiness.
    assert_eq!(wire.service.engine.next_actor(), Some(OTHER));
    let peer_wait = wire.request(OTHER, Action::Wait);
    ack(wire.send(OTHER, "on-use-peer-ready", peer_wait.clone()));
    wire.finish(OTHER, "on-use-peer-ready", &peer_wait);
    assert_eq!(wire.service.engine.next_actor(), Some(OWNER));
    let ready = wire.checkpoint();
    let tick = ready["game"]["tick"].as_u64().unwrap();
    assert_eq!(nothing_actor(&ready, OWNER)["ready_at"], tick);
    let peer_ready = nothing_actor(&ready, OTHER)["ready_at"].as_u64().unwrap();
    assert!(
        tick < peer_ready && peer_ready < tick + 100,
        "peer boundary must stop inside, not before or after, the 100-tick preparation"
    );
    let physical = nothing_wand_id(&wire, wand);
    let peer = wire.snapshot(OTHER);
    let peer_wand = peer
        .state
        .observation
        .inventory
        .iter()
        .find(|row| row.class == ItemClass::Wand)
        .unwrap()
        .id;
    for (actor, item) in [(OWNER, wand), (OTHER, peer_wand)] {
        let snapshot = wire.snapshot(actor);
        assert!(!nothing_identified(&snapshot, item));
        assert_eq!(
            nothing_affordance(&snapshot, item).device_activation,
            Some(DeviceActivation::Direction)
        );
    }
    let mut follower = wire
        .service
        .connect(
            &Account {
                user: "on-use-follower".into(),
                token: "fixture".into(),
                role: AccessRole::Spectator,
                actors: BTreeSet::from([OWNER]),
            },
            "on-use-follower".into(),
        )
        .unwrap();
    follower.messages.try_recv_frame().unwrap();
    wire.service.handle(
        follower.id,
        "follow".into(),
        Request::Attach { actor: OWNER },
    );
    let response = decode_response(&follower.messages.try_recv_frame().unwrap().text).unwrap();
    let ServerMessage::Snapshot { snapshot, .. } = response else {
        panic!("follow snapshot")
    };
    assert!(!nothing_identified(&snapshot, wand));
    assert_eq!(
        nothing_affordance(&snapshot, wand).device_activation,
        Some(DeviceActivation::Direction)
    );

    let request = wire.request(
        OWNER,
        Action::Zap {
            item: wand,
            aim: DeviceAim::Direction {
                direction: Direction::East,
            },
        },
    );
    let before = wire.checkpoint();
    assert_eq!(
        nothing_items(&before)[&physical]["spec"]["identity"],
        "fixture.nothing"
    );
    assert_eq!(
        nothing_items(&before)["31"]["spec"]["identity"],
        "fixture.nothing"
    );
    assert_eq!(
        nothing_items(&before)["31"]["location"],
        json!({"Carried": OTHER.0})
    );
    assert_eq!(nothing_items(&before)["31"]["charges"], 3);
    let admitted = ack(wire.send(OWNER, "on-use-self", request.clone()));
    assert!(
        !nothing_identified(&wire.snapshot(OWNER), wand),
        "admission cannot identify"
    );
    wire.service.run_until_blocked();
    wire.drain(OWNER);
    wire.drain(OTHER);
    let started = ack(wire.send(OWNER, "on-use-self", request.clone()));
    assert!(
        matches!(
            started,
            RequestReceipt::Admitted {
                phase: IntentionPhase::Started,
                ..
            }
        ),
        "actual preparation receipt: {started:?}"
    );
    assert_eq!(started.entry_id(), admitted.entry_id());
    let pending = wire.checkpoint();
    assert_eq!(nothing_items(&pending)[&physical]["charges"], 3);
    let snapshot = wire.snapshot(OWNER);
    assert!(
        snapshot
            .state
            .observation
            .interactions
            .as_ref()
            .unwrap()
            .preparation
            .as_ref()
            .unwrap()
            .remaining
            > 0
    );
    assert!(
        !nothing_identified(&snapshot, wand),
        "preparation cannot identify"
    );
    assert!(!snapshot
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
        .any(|c| c.condition.id == "fixture.self_device"));
    let completed = wire.finish(OWNER, "on-use-self", &request);
    assert_eq!(completed.entry_id(), admitted.entry_id());
    let after = wire.checkpoint();
    assert_eq!(nothing_items(&after)[&physical]["charges"], 2);
    assert_eq!(after["game"]["rules_rng"], before["game"]["rules_rng"]);
    assert_eq!(
        after["game"]["tick"].as_u64().unwrap(),
        before["game"]["tick"].as_u64().unwrap() + 100
    );
    let owner = wire.snapshot(OWNER);
    assert!(nothing_identified(&owner, wand));
    assert_eq!(
        nothing_affordance(&owner, wand).device_activation,
        Some(DeviceActivation::SelfTarget)
    );
    assert_eq!(
        nothing_affordance(&owner, wand).charges,
        None,
        "identity is not exact balance knowledge"
    );
    let condition = owner
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
        .find(|c| c.condition.id == "fixture.self_device")
        .unwrap();
    assert_eq!(condition.remaining_ticks, Some(2000));
    let peer = wire.snapshot(OTHER);
    assert!(!nothing_identified(&peer, peer_wand));
    assert_eq!(
        nothing_affordance(&peer, peer_wand).device_activation,
        Some(DeviceActivation::Direction)
    );
    assert_eq!(nothing_affordance(&peer, peer_wand).charges, None);
    assert_eq!(nothing_items(&after)["31"]["charges"], 3);
    while follower.messages.try_recv_frame().is_ok() {}
    wire.service
        .handle(follower.id, "completed-follow".into(), Request::Snapshot);
    let response = decode_response(&follower.messages.try_recv_frame().unwrap().text).unwrap();
    let ServerMessage::Snapshot { snapshot, .. } = response else {
        panic!("follow snapshot")
    };
    assert!(nothing_identified(&snapshot, wand));
    assert_eq!(
        nothing_affordance(&snapshot, wand).device_activation,
        Some(DeviceActivation::SelfTarget)
    );
    assert_eq!(nothing_affordance(&snapshot, wand).charges, None);
    assert!(!snapshot.has_control);
    assert_eq!(ack(wire.send(OWNER, "on-use-self", request)), completed);
    assert_eq!(wire.checkpoint(), after);
}

#[test]
fn late_store_rejection_rolls_back_self_effect_charge_and_on_use_identity() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("source");
    let path = directory.path().join("self-effect.db");
    let source = self_device_source(&root, false, true);
    let package = source.package.as_ref().unwrap();
    let mut manifest = serde_json::to_value(&package.manifest).unwrap();
    manifest["archetypes"]["ember"]["device"]["discovery"] = json!("on_use");
    let rules = toml::to_string(package.rules().unwrap().document()).unwrap();
    let package = Package::from_parts(
        serde_json::from_value(manifest).unwrap(),
        package.region_defs().unwrap(),
        &rules,
    )
    .unwrap();
    package.check().unwrap();
    crate::scenario_package::write_package(
        &root,
        &package.manifest,
        &package.region_defs().unwrap(),
        &rules,
    )
    .unwrap();
    let fixture = crate::scenario_package::load(&root, 42, None, true).unwrap();
    let mut wire = Wire::new(Engine::open(&path, fixture.clone()).unwrap());
    let wand = wire.take(ItemClass::Wand);
    let physical = nothing_wand_id(&wire, wand);
    let observed = wire.snapshot(OWNER);
    assert!(!nothing_identified(&observed, wand));
    assert_eq!(
        nothing_affordance(&observed, wand).device_activation,
        Some(DeviceActivation::Direction)
    );
    assert!(!observed
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
        .any(|c| c.condition.id == "fixture.self_device"));
    assert_eq!(nothing_items(&wire.checkpoint())[&physical]["charges"], 3);
    let request = wire.request(
        OWNER,
        Action::Zap {
            item: wand,
            aim: DeviceAim::Direction {
                direction: Direction::West,
            },
        },
    );
    let admitted = ack(wire.send(OWNER, "atomic-self", request.clone()));
    wire.service.engine.flush().unwrap();
    drop(wire);
    let saved = crate::inspect_save(&path).unwrap();
    let mut engine = Engine::open_with_policy(
        &path,
        fixture.clone(),
        crate::SavePolicy {
            max_pending_bytes: 1,
            ..Default::default()
        },
    )
    .unwrap();
    let before = checkpoint(&engine);
    // Prove the storage fault will reject this still-unexecuted self payload.
    assert_eq!(nothing_items(&before)[&physical]["charges"], 3);
    let observed = engine.observation(OWNER).unwrap();
    assert!(
        !observed
            .inventory
            .iter()
            .find(|row| row.id == wand)
            .unwrap()
            .identified
    );
    assert!(!observed
        .combat
        .as_ref()
        .unwrap()
        .own_stats
        .as_ref()
        .unwrap()
        .conditions
        .iter()
        .any(|row| row.condition.id == "fixture.self_device"));
    let RequestReceipt::Admitted {
        intention, phase, ..
    } = &admitted
    else {
        panic!("actual admission")
    };
    assert_eq!(*phase, IntentionPhase::Queued);
    let statuses = engine.pending_intentions(OWNER);
    assert_eq!(statuses.len(), 1);
    assert_eq!(&statuses[0].intention, intention);
    assert_eq!(statuses[0].phase, IntentionPhase::Queued);
    let sequence = engine.save_status().accepted_sequence;
    for _ in 0..2 {
        let error = engine.execute_next_intention().unwrap_err();
        assert_eq!(error.code, ErrorCode::StorageFailure);
        assert_eq!(
            error.message,
            "Save queue is full; wait for saving to finish before retrying"
        );
        assert_eq!(checkpoint(&engine), before);
        assert_eq!(engine.save_status().accepted_sequence, sequence);
        assert_eq!(crate::inspect_save(&path).unwrap(), saved);
    }
    drop(engine);
    let mut engine = Engine::open(&path, fixture).unwrap();
    engine.execute_next_intention().unwrap().unwrap();
    let after = checkpoint(&engine);
    assert_eq!(nothing_items(&after)[&physical]["charges"], 2);
    assert_eq!(after["game"]["rules_rng"], before["game"]["rules_rng"]);
    assert_eq!(
        after["game"]["tick"].as_u64().unwrap(),
        before["game"]["tick"].as_u64().unwrap() + 100
    );
    let mut wire = Wire::new(engine);
    let observed = wire.snapshot(OWNER);
    assert!(nothing_identified(&observed, wand));
    assert_eq!(
        nothing_affordance(&observed, wand).charges,
        None,
        "OnUse identity does not reveal the exact charge balance"
    );
    assert_eq!(
        nothing_affordance(&observed, wand).device_activation,
        Some(DeviceActivation::SelfTarget)
    );
    let conditions = &observed
        .state
        .observation
        .combat
        .as_ref()
        .unwrap()
        .own_stats
        .as_ref()
        .unwrap()
        .conditions;
    let applied: Vec<_> = conditions
        .iter()
        .filter(|c| c.condition.id == "fixture.self_device")
        .collect();
    assert_eq!(applied.len(), 1);
    assert_eq!(applied[0].remaining_ticks, Some(2000));
    let settled = wire.checkpoint();
    let retry = ack(wire.send(OWNER, "atomic-self", request));
    assert_eq!(retry.entry_id(), admitted.entry_id());
    assert!(matches!(
        retry,
        RequestReceipt::Admitted {
            phase: IntentionPhase::Resolved,
            ..
        }
    ));
    assert_eq!(wire.checkpoint(), settled);
}

// No source/catalog mutation: production resolved archetypes and rules are retained.
fn imported_nothing_source(root: &Path) -> crate::Scenario {
    let production_root =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/rogue-exploration");
    let production = crate::scenario_package::resolve_source(&production_root).unwrap();
    let rules =
        std::fs::read_to_string(production_root.join(&production.manifest.ruleset)).unwrap();
    let rules = resolve_rules_source(&rules, "rules.toml").unwrap().source;
    let mut manifest = serde_json::to_value(production.manifest).unwrap();
    let original_items = manifest["archetypes"].clone();
    let original_creatures = manifest["creatures"].clone();
    assert_eq!(
        original_items["rogue.wand_nothing"]["device"]["ability"],
        "rogue.wand_nothing"
    );
    assert_eq!(
        original_items["rogue.wand_nothing"]["device"]["discovery"],
        "never"
    );
    manifest["id"] = json!("imported-nothing-service");
    manifest["characters"].as_array_mut().unwrap().truncate(1);
    let owner = &mut manifest["characters"][0];
    owner["anchor"] = json!("1/start");
    owner["known_identities"] = json!([]);
    owner["loadout"] = json!([]);
    owner["creature"]["nutrition"] = json!({"kind":"exempt"});
    owner["creature"]["future_growth"] = json!({"kind":"disabled"});
    for key in [
        "procedural_worlds",
        "generation_recipes",
        "generation_groups",
        "stair_pairs",
    ] {
        manifest[key] = json!({});
    }
    for key in ["floor_activation", "unique_retrieval", "objective"] {
        manifest[key] = Value::Null;
    }
    assert_eq!(manifest["archetypes"], original_items);
    assert_eq!(manifest["creatures"], original_creatures);
    let region: crate::scenario_package::RegionDef = serde_json::from_value(json!({
        "id":1,"name":"Imported Nothing laboratory","size":[7,5,2],"chamber":true,
        "anchors":{"start":[2,2,0]},"items":[
            {"id":30,"at":[2,2,0],"archetype":"rogue.wand_nothing"},
            {"id":20,"at":[2,2,0],"archetype":"rogue.identify_wand","quantity":1}
        ]
    }))
    .unwrap();
    let checked = Package::from_parts(
        serde_json::from_value(manifest).unwrap(),
        vec![region],
        &rules,
    )
    .unwrap();
    checked.check().unwrap();
    assert_eq!(
        checked
            .rules()
            .unwrap()
            .document()
            .constants
            .item_timing
            .consume_turns,
        1
    );
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

#[test]
fn imported_nothing_paid_unknown_then_identified_self_survives_source_deleted_reopen() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("source");
    let database = directory.path().join("nothing.db");
    let mut wire = Wire::new(Engine::open(&database, imported_nothing_source(&root)).unwrap());
    assert_eq!(wire.service.engine.actors(), vec![OWNER]);
    let wand = wire.take(ItemClass::Wand);
    let scroll = wire.take(ItemClass::Scroll);
    let physical = nothing_wand_id(&wire, wand);
    let unknown = wire.snapshot(OWNER);
    assert!(!nothing_identified(&unknown, wand));
    assert_eq!(
        nothing_affordance(&unknown, wand).device_activation,
        Some(DeviceActivation::Direction)
    );
    assert_eq!(nothing_affordance(&unknown, wand).charges, None);
    let bad = wire.request(
        OWNER,
        Action::Zap {
            item: wand,
            aim: DeviceAim::SelfTarget {},
        },
    );
    let untouched = wire.checkpoint();
    rejected(wire.send(OWNER, "unknown-self-rejected", bad));
    assert_eq!(wire.checkpoint(), untouched);
    assert_eq!(wire.service.engine.next_actor(), Some(OWNER));
    let before = wire.checkpoint();
    let initial = nothing_items(&before)[&physical]["charges"]
        .as_u64()
        .unwrap();
    assert!((3..=7).contains(&initial));
    let request = wire.request(
        OWNER,
        Action::Zap {
            item: wand,
            aim: DeviceAim::Direction {
                direction: Direction::West,
            },
        },
    );
    let admitted = ack(wire.send(OWNER, "catalog-nothing-unknown", request.clone()));
    let completed = wire.finish(OWNER, "catalog-nothing-unknown", &request);
    assert_eq!(admitted.entry_id(), completed.entry_id());
    let after = wire.checkpoint();
    let zap_ticks =
        after["game"]["tick"].as_u64().unwrap() - before["game"]["tick"].as_u64().unwrap();
    assert!(zap_ticks > 0);
    assert_eq!(nothing_items(&after)[&physical]["charges"], initial - 1);
    assert_eq!(after["game"]["rules_rng"], before["game"]["rules_rng"]);
    assert!(!nothing_identified(&wire.snapshot(OWNER), wand));
    assert_eq!(
        ack(wire.send(OWNER, "catalog-nothing-unknown", request)),
        completed
    );
    assert_eq!(wire.checkpoint(), after);

    // Independent same-source consume-turn baseline: ordinary paid Read uses
    // exactly the same turn law, without assuming Rogue's effective speed is0.
    assert_eq!(wire.service.engine.next_actor(), Some(OWNER));
    let before_read = wire.checkpoint();
    wire.act("pay-catalog-identify", Action::Read { item: scroll });
    let after_read = wire.checkpoint();
    let read_ticks = after_read["game"]["tick"].as_u64().unwrap()
        - before_read["game"]["tick"].as_u64().unwrap();
    assert_eq!(zap_ticks, read_ticks);
    let paid = choice(&wire.snapshot(OWNER));
    wire.act(
        "catalog-identify",
        Action::Identify {
            choice: paid.choice,
            item: Some(wand),
        },
    );
    let known = wire.snapshot(OWNER);
    assert!(nothing_identified(&known, wand));
    assert_eq!(
        nothing_affordance(&known, wand).device_activation,
        Some(DeviceActivation::SelfTarget)
    );
    assert_eq!(
        nothing_affordance(&known, wand).charges,
        Some((initial - 1).try_into().unwrap())
    );
    assert_eq!(wire.service.engine.next_actor(), Some(OWNER));
    let before_self = wire.checkpoint();
    let self_request = wire.request(
        OWNER,
        Action::Zap {
            item: wand,
            aim: DeviceAim::SelfTarget {},
        },
    );
    ack(wire.send(OWNER, "catalog-nothing-self", self_request.clone()));
    let self_receipt = wire.finish(OWNER, "catalog-nothing-self", &self_request);
    let saved = wire.checkpoint();
    assert_eq!(nothing_items(&saved)[&physical]["charges"], initial - 2);
    assert_eq!(saved["game"]["rules_rng"], before_self["game"]["rules_rng"]);
    assert_eq!(
        saved["game"]["tick"].as_u64().unwrap() - before_self["game"]["tick"].as_u64().unwrap(),
        read_ticks
    );
    wire.service.engine.flush().unwrap();
    let serial = wire.serial;
    drop(wire);
    std::fs::remove_dir_all(&root).unwrap();
    assert!(!root.exists());
    let engine = Engine::open(&database, crate::Scenario::two_room(999)).unwrap();
    assert_eq!(checkpoint(&engine), saved);
    let mut wire = Wire::new(engine); // only the one real human owner
    wire.serial = serial;
    let restored = wire.snapshot(OWNER);
    assert!(nothing_identified(&restored, wand));
    assert_eq!(
        nothing_affordance(&restored, wand).device_activation,
        Some(DeviceActivation::SelfTarget)
    );
    let retry_before = wire.checkpoint();
    assert_eq!(
        ack(wire.send(OWNER, "catalog-nothing-self", self_request)),
        self_receipt
    );
    assert_eq!(wire.checkpoint(), retry_before);
}

fn authored_room_light_source(root: &Path, passage: bool) -> crate::Scenario {
    let baseline = self_device_source(root, false, false);
    let source = baseline.package.as_ref().unwrap();
    let mut document = source.rules().unwrap().document().clone();
    let ability = document
        .abilities
        .iter_mut()
        .find(|a| a.id.as_str() == "magic.ember_ray")
        .unwrap();
    let tor_simulation::rules::StrikeSource::Authored { strikes } = &mut ability.strikes else {
        panic!("authored self ability")
    };
    strikes[0].on_success = vec![tor_simulation::rules::Effect::RoomLight];
    let rules = toml::to_string(&document).unwrap();
    let mut manifest = serde_json::to_value(&source.manifest).unwrap();
    manifest["archetypes"]["ember"]["device"]["discovery"] = json!("on_use");
    manifest["spatial_scopes"] = json!({"study":{
        "members":[1],"rooms":if passage {json!({})}else{json!({"1":[{"region":1,"min":[0,0,0],"max":[6,4,1]}]})},
        "passages":if passage {(0..2).flat_map(|z|(0..5).map(move|y|json!({"region":1,"z":z,"y":y,"first_x":0,"last_x":6}))).collect::<Vec<_>>()}else{vec![]},"doorways":[]
    }});
    let mut regions = source.region_defs().unwrap();
    let mut region = serde_json::to_value(&regions[0]).unwrap();
    region["lit"] = json!(false);
    region["lighting"] = json!([]);
    // Ordinary authored inventory avoids relying on visibility of a dark floor
    // item before testing the actual paid device. No synthetic item injection.
    region["items"] = json!([{"id":30,"at":[1,2,0],"archetype":"ember","carried_by":1}]);
    regions[0] = serde_json::from_value(region).unwrap();
    let checked =
        Package::from_parts(serde_json::from_value(manifest).unwrap(), regions, &rules).unwrap();
    checked.check().unwrap();
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
fn room_light_world(saved: &Value) -> tor_world::World {
    #[derive(serde::Deserialize)]
    struct Worlds(#[serde(with = "tor_world::checkpoint_worlds")] Vec<tor_world::World>);
    let worlds: Worlds = serde_json::from_value(saved["shared"]["worlds"].clone()).unwrap();
    worlds.0[saved["game"]["world"].as_u64().unwrap() as usize].clone()
}
fn paid_authored_room_light(passage: bool, stored_checkpoint: bool) {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("source");
    let path = directory.path().join("light.db");
    let policy = crate::SavePolicy {
        checkpoint_interval: if stored_checkpoint { 1 } else { 4096 },
        ..Default::default()
    };
    let mut wire = Wire::new(
        Engine::open_with_policy(&path, authored_room_light_source(&root, passage), policy)
            .unwrap(),
    );
    let snapshot = wire.snapshot(OWNER);
    let wand = snapshot
        .state
        .observation
        .inventory
        .iter()
        .find(|i| i.class == ItemClass::Wand)
        .unwrap()
        .id;
    assert!(!nothing_identified(&snapshot, wand));
    assert_eq!(
        nothing_affordance(&snapshot, wand).device_activation,
        Some(DeviceActivation::Direction)
    );
    assert_eq!(wire.service.engine.next_actor(), Some(OWNER));
    let physical = nothing_wand_id(&wire, wand);
    let before = wire.checkpoint();
    let at = tor_world::Location {
        region: tor_world::RegionId(1),
        position: tor_world::Position { x: 5, y: 3, z: 1 },
    };
    assert_eq!(room_light_world(&before).is_lit(at), Some(false));
    let request = wire.request(
        OWNER,
        Action::Zap {
            item: wand,
            aim: DeviceAim::Direction {
                direction: Direction::West,
            },
        },
    );
    ack(wire.send(OWNER, "room-light-once", request.clone()));
    let receipt = wire.finish(OWNER, "room-light-once", &request);
    let after = wire.checkpoint();
    assert_eq!(nothing_items(&after)[&physical]["charges"], 2);
    assert_eq!(
        after["game"]["tick"].as_u64().unwrap(),
        before["game"]["tick"].as_u64().unwrap() + 100
    );
    assert_eq!(after["game"]["rules_rng"], before["game"]["rules_rng"]);
    let known = wire.snapshot(OWNER);
    assert!(nothing_identified(&known, wand));
    assert_eq!(
        nothing_affordance(&known, wand).device_activation,
        Some(DeviceActivation::SelfTarget)
    );
    assert_eq!(nothing_affordance(&known, wand).charges, None);
    assert_eq!(
        room_light_world(&after).is_lit(at),
        Some(!passage),
        "paid RoomLight must use canonical Room versus Passage membership"
    );
    assert_eq!(ack(wire.send(OWNER, "room-light-once", request)), receipt);
    assert_eq!(wire.checkpoint(), after);
    wire.act(
        "room-light-repeat",
        Action::Zap {
            item: wand,
            aim: DeviceAim::SelfTarget {},
        },
    );
    let repeated = wire.checkpoint();
    assert_eq!(nothing_items(&repeated)[&physical]["charges"], 1);
    assert_eq!(repeated["game"]["rules_rng"], after["game"]["rules_rng"]);
    assert_eq!(room_light_world(&repeated).is_lit(at), Some(!passage));
    wire.service.engine.flush().unwrap();
    if stored_checkpoint {
        assert_real_stored_checkpoint(&path, true);
    }
    drop(wire);
    std::fs::remove_dir_all(&root).unwrap();
    let reopened = Engine::open(&path, crate::Scenario::two_room(999)).unwrap();
    assert_eq!(checkpoint(&reopened), repeated);
}
#[test]
fn actual_paid_room_light_uses_canonical_room_and_survives_source_deleted_reopen() {
    paid_authored_room_light(false, false);
}
#[test]
fn actual_paid_room_light_in_explicit_passage_is_identified_paid_noop() {
    paid_authored_room_light(true, false);
}

fn assert_real_stored_checkpoint(path: &Path, spatial: bool) {
    let database =
        rusqlite::Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap();
    let (sequence, payload): (i64, Vec<u8>) = database
        .query_row(
            "SELECT sequence,payload FROM checkpoint WHERE slot=1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert!(sequence > 0);
    let decoded = crate::storage::checkpoint_codec::decode(&payload).unwrap();
    let _: crate::engine::DiskCheckpoint = crate::storage::codec::strict(&decoded).unwrap();
    let value: Value = serde_json::from_slice(&decoded).unwrap();
    assert_eq!(
        !value["shared"]["spatial"]["scopes"]
            .as_array()
            .unwrap()
            .is_empty(),
        spatial
    );
    assert!(!value["boundaries"].as_array().unwrap().is_empty());
}
#[test]
fn actual_room_light_compressed_interval_one_checkpoint_reopens_without_sources() {
    paid_authored_room_light(false, true);
}
#[test]
fn actual_passage_light_compressed_interval_one_checkpoint_reopens_without_sources() {
    paid_authored_room_light(true, true);
}
#[test]
fn ordinary_empty_spatial_compressed_interval_one_checkpoint_reopens_without_sources() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("source");
    let path = directory.path().join("ordinary.db");
    let scenario = nothing_source(&root, false);
    let policy = crate::SavePolicy {
        checkpoint_interval: 1,
        ..Default::default()
    };
    let mut wire = Wire::new(Engine::open_with_policy(&path, scenario, policy).unwrap());
    wire.act("ordinary-wait", Action::Wait);
    let expected = wire.checkpoint();
    wire.service.engine.flush().unwrap();
    assert_real_stored_checkpoint(&path, false);
    drop(wire);
    std::fs::remove_dir_all(root).unwrap();
    let reopened = Engine::open(&path, crate::Scenario::two_room(999)).unwrap();
    assert_eq!(checkpoint(&reopened), expected);
}

#[test]
fn enabled_room_light_requires_metadata_but_unused_import_does_not() {
    let directory = tempfile::tempdir().unwrap();
    let scenario = authored_room_light_source(directory.path(), false);
    let source = scenario.package.as_ref().unwrap();
    source.check().unwrap();
    let rules = toml::to_string(source.rules().unwrap().document()).unwrap();
    let mut manifest = source.manifest.clone();
    manifest.spatial_scopes.clear();
    let regions = source.region_defs().unwrap();
    let enabled = Package::from_parts(manifest.clone(), regions.clone(), &rules).unwrap();
    assert!(
        enabled.check().is_err(),
        "actually reachable RoomLight needs canonical metadata"
    );
    let mut unused_regions = regions;
    unused_regions[0].items.clear();
    let unused = Package::from_parts(manifest, unused_regions, &rules).unwrap();
    unused.check().unwrap();
    assert!(unused
        .manifest
        .archetypes
        .get("ember")
        .unwrap()
        .device
        .is_some());
}

fn managed_remote_room_light(foreign_face: bool, connected: bool) {
    assert!(!(foreign_face && connected));
    let remote_region = if connected { 6 } else { 2 };
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("source");
    let path = directory.path().join("remote-light.db");
    let base = authored_room_light_source(&root, false);
    let source = base.package.as_ref().unwrap();
    let rules = toml::to_string(source.rules().unwrap().document()).unwrap();
    let mut manifest = serde_json::to_value(&source.manifest).unwrap();
    if foreign_face {
        manifest["spatial_scopes"]["study"]["rooms"]["1"][0]["max"] = json!([31, 4, 1]);
        let passages: Vec<_> = (0..2)
            .flat_map(|z| {
                (0..3).flat_map(move |y| {
                    let spans = if z == 1 && y == 1 {
                        vec![(0, 0), (2, 2)]
                    } else {
                        vec![(0, 2)]
                    };
                    spans
                        .into_iter()
                        .map(move |(a, b)| json!({"region":2,"z":z,"y":y,"first_x":a,"last_x":b}))
                })
            })
            .collect();
        manifest["spatial_scopes"]["neighbor"] =
            json!({"members":[2],"rooms":{},"passages":passages,"doorways":[]});
    } else if connected {
        manifest["spatial_scopes"]["study"]["members"] = json!([1, 2, 3, 4, 5, 6]);
        manifest["spatial_scopes"]["study"]["rooms"]["1"] = json!((1..=6)
            .map(|region| json!({"region":region,"min":[0,0,0],"max":[31,4,1]}))
            .collect::<Vec<_>>());
    } else {
        manifest["spatial_scopes"]["study"]["members"] = json!([1, 2]);
        manifest["spatial_scopes"]["study"]["rooms"]["1"]
            .as_array_mut()
            .unwrap()
            .push(json!({"region":2,"min":[0,0,0],"max":[2,2,1]}));
    }
    let creature = manifest["characters"][0]["creature"].clone();
    let mut definitions = source.region_defs().unwrap();
    let mut local = serde_json::to_value(&definitions[0]).unwrap();
    local["actors"] =
        json!([{"id":2,"at":[5,2,0],"turn_ticks":100,"controller":"external","creature":creature}]);
    if foreign_face {
        // Keep the linked target outside both observers' sight activation.
        // Linked targets are nevertheless loaded by streaming requirements,
        // so this branch proves preservation of an already-frozen source.
        local["size"] = json!([32, 5, 2]);
        local["portals"] = json!([{"at":[31,1,1],"direction":"east","to":"2/entry","kind":"boundary","width":2,"turns":1}]);
        let mouth_x = local["portals"][0]["at"][0].as_i64().unwrap();
        let peer_x = local["actors"][0]["at"][0].as_i64().unwrap();
        let owner_x = local["anchors"]["start"][0].as_i64().unwrap();
        assert!(mouth_x - peer_x > 16 && mouth_x - owner_x > 16);
    }
    if connected {
        local["size"] = json!([32, 5, 2]);
        local["anchors"]["east"] = json!([31, 2, 0]);
        local["portals"] =
            json!([{"at":[31,2,0],"direction":"east","to":"2/west","kind":"portal","height":2}]);
    }
    definitions[0] = serde_json::from_value(local).unwrap();
    let mut remote_definition = json!({"id":2,"name":"Remote surface","size":[3,3,2],"chamber":true,"lit":false,
        "actors":[{"id":3,"at":[0,0,0],"turn_ticks":100,"controller":"ai","creature":creature}]});
    if foreign_face {
        remote_definition["walls"] = json!([[1, 1, 1]]);
        remote_definition["anchors"] = json!({"entry":[2,1,1]});
    }
    if connected {
        for region in 2..=6 {
            let mut portals = vec![
                json!({"at":[0,2,0],"direction":"west","to":format!("{}/east",region-1),"kind":"portal","height":2}),
            ];
            if region < 6 {
                portals.push(json!({"at":[31,2,0],"direction":"east","to":format!("{}/west",region+1),"kind":"portal","height":2}));
            }
            let mut definition = json!({"id":region,"name":format!("Connected room member {region}"),"size":[32,5,2],"chamber":true,"lit":false,
                "anchors":{"west":[0,2,0],"east":[31,2,0]},"portals":portals});
            if region == 6 {
                definition["actors"] = remote_definition["actors"].clone();
            }
            definitions.push(serde_json::from_value(definition).unwrap());
        }
    } else {
        definitions.push(serde_json::from_value(remote_definition).unwrap());
    }
    let package = Package::from_parts(
        serde_json::from_value(manifest).unwrap(),
        definitions,
        &rules,
    )
    .unwrap();
    package.check().unwrap();
    assert!(package.manifest.floor_activation.is_none());
    crate::scenario_package::write_package(
        &root,
        &package.manifest,
        &package.region_defs().unwrap(),
        &rules,
    )
    .unwrap();
    crate::scenario_package::check(&root).unwrap();
    let mut scenario = crate::scenario_package::load(&root, 42, None, true).unwrap();
    scenario.streaming = Some(crate::Streaming {
        active_radius: 0,
        load_radius: 0,
    });
    let policy = crate::SavePolicy {
        checkpoint_interval: 4096,
        ..Default::default()
    };
    // Revision identities are not playable-owner authority. Keep scenario AI
    // outside client control, independently of its structural residency.
    let mut wire = Wire::for_actors(
        Engine::open_with_policy(&path, scenario, policy).unwrap(),
        [OWNER, OTHER],
    );
    assert_eq!(
        wire.clients.keys().copied().collect::<Vec<_>>(),
        vec![OWNER, OTHER],
        "never acquire control of scenario AI"
    );
    let initial = wire.checkpoint();
    if foreign_face {
        let actor = nothing_actor(&initial, ActorId(3));
        assert!(actor.is_object());
        assert_eq!(actor["ready_at"], 0);
        assert_eq!(actor["active_since"], 0);
        let lifecycle = initial["game"]["lifecycle"].as_u64().unwrap() as usize;
        let lifecycle = &initial["shared"]["lifecycles"][lifecycle];
        assert!(lifecycle["frozen"]
            .as_array()
            .unwrap()
            .contains(&json!(remote_region)));
        assert_eq!(lifecycle["stamps"]["3"], 0);
        let scopes = initial["shared"]["spatial"]["scopes"].as_array().unwrap();
        let study = scopes.iter().find(|s| s["id"] == "study").unwrap();
        assert_eq!(study["members"], json!([1]));
        assert!(study["primary"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r["cells"]["region"] == 1));
        assert!(study["rooms"][0]["runs"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["region"] == 2));
        assert_eq!(
            scopes.iter().find(|s| s["id"] == "neighbor").unwrap()["members"],
            json!([2]),
            "ordinary linked-region preparation installs its own primary scope"
        );
    } else {
        assert!(nothing_actor(&initial, ActorId(3)).is_null());
    }
    let remote = tor_world::Location {
        region: tor_world::RegionId(remote_region),
        position: tor_world::Position { x: 1, y: 1, z: 1 },
    };
    assert_eq!(
        room_light_world(&initial).is_lit(remote),
        if foreign_face { Some(false) } else { None }
    );
    if connected {
        let world = room_light_world(&initial);
        let near = tor_world::Location {
            region: tor_world::RegionId(2),
            position: tor_world::Position { x: 1, y: 1, z: 1 },
        };
        assert_eq!(
            world.is_lit(near),
            Some(false),
            "ordinary linked neighbor is loaded frozen"
        );
        assert_eq!(
            world.is_lit(tor_world::Location {
                region: tor_world::RegionId(3),
                ..near
            }),
            None,
            "a frozen linked neighbor is not an expansion root"
        );
        let scopes = initial["shared"]["spatial"]["scopes"].as_array().unwrap();
        assert_eq!(
            scopes.iter().find(|s| s["id"] == "study").unwrap()["members"],
            json!([1, 2, 3, 4, 5, 6])
        );
    }
    let snapshot = wire.snapshot(OWNER);
    let wand = snapshot
        .state
        .observation
        .inventory
        .iter()
        .find(|i| i.class == ItemClass::Wand)
        .unwrap()
        .id;
    let physical = nothing_wand_id(&wire, wand);
    assert_eq!(nothing_items(&initial)[&physical]["charges"], 3);
    let request = wire.request(
        OWNER,
        Action::Zap {
            item: wand,
            aim: DeviceAim::Direction {
                direction: Direction::East,
            },
        },
    );
    ack(wire.send(OWNER, "cold-room-light", request.clone()));
    wire.service.run_until_blocked();
    wire.drain(OWNER);
    wire.drain(OTHER);
    let started = ack(wire.send(OWNER, "cold-room-light", request.clone()));
    assert!(matches!(
        started,
        RequestReceipt::Admitted {
            phase: IntentionPhase::Started,
            ..
        }
    ));
    let paused = wire.checkpoint();
    assert_eq!(paused["game"]["tick"], initial["game"]["tick"]);
    assert_eq!(room_light_world(&paused).is_lit(remote), Some(false));
    if connected {
        let world = room_light_world(&paused);
        for region in 1..6 {
            for z in 0..2 {
                let east = tor_world::Location {
                    region: tor_world::RegionId(region),
                    position: tor_world::Position { x: 31, y: 2, z },
                };
                let west = tor_world::Location {
                    region: tor_world::RegionId(region + 1),
                    position: tor_world::Position { x: 0, y: 2, z },
                };
                assert_eq!(
                    world
                        .physics_neighbor(east, tor_world::Direction::East)
                        .unwrap()
                        .0,
                    west
                );
                assert_eq!(
                    world
                        .physics_neighbor(west, tor_world::Direction::West)
                        .unwrap()
                        .0,
                    east
                );
            }
        }
    }
    let actor = nothing_actor(&paused, ActorId(3)).clone();
    assert!(actor.is_object());
    if foreign_face {
        assert_eq!(&actor, nothing_actor(&initial, ActorId(3)));
    }
    assert_eq!(actor["ready_at"], 0);
    assert_eq!(actor["active_since"], 0);
    let lifecycle = paused["game"]["lifecycle"].as_u64().unwrap() as usize;
    assert_eq!(paused["shared"]["lifecycles"][lifecycle]["stamps"]["3"], 0);
    assert!(paused["shared"]["lifecycles"][lifecycle]["frozen"]
        .as_array()
        .unwrap()
        .contains(&json!(remote_region)));
    if connected {
        for region in 2..=6 {
            assert!(paused["shared"]["lifecycles"][lifecycle]["frozen"]
                .as_array()
                .unwrap()
                .contains(&json!(region)));
        }
    }
    assert!(paused["game"]["floor_activation"].is_null());
    wire.finish(OWNER, "cold-room-light", &request);
    let after = wire.checkpoint();
    assert_eq!(nothing_items(&after)[&physical]["charges"], 2);
    assert_eq!(after["game"]["tick"].as_u64().unwrap(), 100);
    assert_eq!(after["game"]["rules_rng"], paused["game"]["rules_rng"]);
    let mut world = room_light_world(&after);
    if foreign_face {
        assert!(
            world.is_lit(remote).is_some(),
            "active source retains its linked target loaded"
        );
    }
    if world.is_lit(remote).is_none() {
        // Read the real unpublished-to-disk immutable record carried by the
        // ordinary checkpoint. No private Game access or invented region state.
        let capture = crate::engine::Checkpoint::capture(&wire.service.engine);
        let record = capture
            .records
            .values()
            .map(|record| {
                serde_json::to_value(tor_simulation::RegionRecordData::capture(record)).unwrap()
            })
            .find(|record| record["world"]["region"]["id"] == remote_region)
            .expect("new detached remote record before4096-entry checkpoint interval");
        assert_eq!(record["actors"]["3"], actor);
        assert_eq!(record["stamps"]["3"], 0);
        let slice: tor_world::RegionSlice =
            serde_json::from_value(record["world"].clone()).unwrap();
        world.attach_region(slice).unwrap();
    } else {
        assert_eq!(nothing_actor(&after, ActorId(3)), &actor);
        let lifecycle = after["game"]["lifecycle"].as_u64().unwrap() as usize;
        assert_eq!(after["shared"]["lifecycles"][lifecycle]["stamps"]["3"], 0);
        assert!(after["shared"]["lifecycles"][lifecycle]["frozen"]
            .as_array()
            .unwrap()
            .contains(&json!(remote_region)));
    }
    assert_eq!(
        world.is_lit(remote),
        Some(true),
        "remote frozen surface must receive paid light"
    );
    if foreign_face {
        assert!(world.is_wall(remote));
        assert_eq!(
            world.is_lit(tor_world::Location {
                position: tor_world::Position { x: 0, y: 1, z: 1 },
                ..remote
            }),
            Some(false),
            "no illumination grant to foreign traversable primary"
        );
    }
    assert!(after["game"]["floor_activation"].is_null());
    wire.service.engine.flush().unwrap();
    drop(wire);
    std::fs::remove_dir_all(&root).unwrap();
    let reopened = Engine::open(&path, crate::Scenario::two_room(999)).unwrap();
    assert_eq!(checkpoint(&reopened), after);
}

#[test]
fn managed_room_light_loads_remote_same_room_surface_without_activating_actor() {
    managed_remote_room_light(false, false);
}
#[test]
fn managed_room_light_preserves_frozen_foreign_surface_without_claiming_primary_or_waking_ai() {
    managed_remote_room_light(true, false);
}

#[test]
fn managed_room_light_loads_physically_connected_far_room_member_without_waking_ai() {
    managed_remote_room_light(false, true);
}

fn paid_generated_room_light(maze: bool) {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("source");
    let path = directory.path().join("generated-light.db");
    let base = authored_room_light_source(&root, false);
    let source = base.package.as_ref().unwrap();
    let mut manifest = serde_json::to_value(&source.manifest).unwrap();
    manifest["spatial_scopes"] = json!({});
    manifest["characters"][0]["anchor"] = json!("dungeon/1/start");
    manifest["characters"][0]["loadout"] = json!([{"archetype":"ember","quantity":1}]);
    let recipe = crate::generation_recipe::GridRoomsPreset {
        version: 1,
        room_width: [30, 30],
        room_height: [30, 30],
        extra_connections: [0, 0],
        room_omissions: None,
        maze_rooms: maze.then_some(crate::generation_recipe::MazeRooms { one_in: 1 }),
        lighting: Some(crate::generation_recipe::RoomLighting {
            darkness_roll: 1,
            darkness_start: 0,
        }),
        secret_entrances: None,
        hidden_traps: None,
    }
    .to_recipe()
    .unwrap();
    manifest["generation_recipes"] = json!({"room":recipe});
    manifest["procedural_worlds"] = json!({"dungeon":{"kind":"grid_chain","version":1,"base_region":1,"grid":[2,1],"size":[32,32,2],"recipe":"room","domain":{"kind":"open","max_depth":2}}});
    let rules = toml::to_string(source.rules().unwrap().document()).unwrap();
    let package =
        Package::from_parts(serde_json::from_value(manifest).unwrap(), vec![], &rules).unwrap();
    package.check().unwrap();
    // The baseline wrote an authored region file. Do not let that stale file
    // collide with the genuine procedural family in the on-disk validator.
    std::fs::remove_dir_all(&root).unwrap();
    crate::scenario_package::write_package(&root, &package.manifest, &[], &rules).unwrap();
    crate::scenario_package::check(&root).unwrap();
    let scenario = crate::scenario_package::load(&root, 42, None, true).unwrap();
    let mut wire = Wire::new(Engine::open(&path, scenario).unwrap());
    let snapshot = wire.snapshot(OWNER);
    let wand = snapshot
        .state
        .observation
        .inventory
        .iter()
        .find(|i| i.class == ItemClass::Wand)
        .unwrap()
        .id;
    let physical = nothing_wand_id(&wire, wand);
    let before = wire.checkpoint();
    let at: tor_world::Location =
        serde_json::from_value(nothing_actor(&before, OWNER)["location"].clone()).unwrap();
    assert_eq!(room_light_world(&before).is_lit(at), Some(false));
    assert_eq!(wire.service.engine.next_actor(), Some(OWNER));
    wire.act(
        "generated-room-light",
        Action::Zap {
            item: wand,
            aim: DeviceAim::Direction {
                direction: Direction::West,
            },
        },
    );
    let after = wire.checkpoint();
    assert_eq!(nothing_items(&after)[&physical]["charges"], 2);
    assert_eq!(
        after["game"]["tick"].as_u64().unwrap(),
        before["game"]["tick"].as_u64().unwrap() + 100
    );
    assert_eq!(after["game"]["rules_rng"], before["game"]["rules_rng"]);
    assert_eq!(room_light_world(&after).is_lit(at), Some(!maze));
    assert!(nothing_identified(&wire.snapshot(OWNER), wand));
    wire.service.engine.flush().unwrap();
    drop(wire);
    std::fs::remove_dir_all(root).unwrap();
    let reopened = Engine::open(&path, crate::Scenario::two_room(999)).unwrap();
    assert_eq!(checkpoint(&reopened), after);
}
#[test]
fn actual_paid_generated_room_light_uses_retained_room_geometry() {
    paid_generated_room_light(false);
}
#[test]
fn actual_paid_generated_maze_room_light_is_paid_identified_noop() {
    paid_generated_room_light(true);
}
