//! Authored body ownership through paid Service admission and source-free restore.
//! No privileged mutation: the compiler, inventory, identification and scheduler
//! own every fixture transition. Core tests own combat rolls and package math.
use super::*;

const BASE: &str = "private.body.birth";
const FORM: &str = "private.body.changed";

fn source(root: &Path, blocked: bool) -> crate::Scenario {
    use tor_simulation::{
        abilities::InvocationKind,
        rules::{Effect, Reach, StrikeSource},
    };
    let baseline = scenario(false);
    let source = baseline.package.as_ref().unwrap();
    let mut rules = source.rules().unwrap().document().clone();
    let ray = rules
        .abilities
        .iter_mut()
        .find(|a| a.id.as_str() == "magic.ember_ray")
        .unwrap();
    ray.invocation = InvocationKind::DirectionalContactRay;
    ray.reach = Reach::UntilObstruction;
    let StrikeSource::Authored { strikes } = &mut ray.strikes else {
        panic!("authored ray")
    };
    assert_eq!(strikes.len(), 1);
    strikes[0].on_success.clear();
    strikes[0].on_failure = vec![Effect::BalefulPolymorph {
        forms: vec![tor_simulation::content_ids::BodyFormId::new(FORM).unwrap()],
    }];
    strikes[0].descriptors.clear();
    strikes[0].riders.clear();
    let rules = toml::to_string(
        tor_simulation::rules::Ruleset::validate(rules)
            .unwrap()
            .document(),
    )
    .unwrap();
    let mut manifest = serde_json::to_value(&source.manifest).unwrap();
    manifest["archetypes"]["ember"]["name"] = json!("wand of polymorph");
    manifest["archetypes"]["ember"]["identity"] = json!("fixture.polymorph");
    manifest["archetypes"]["ember"]["device"]["discovery"] = json!("discernible_effect");
    let mut changed = manifest["creatures"]["species"]["figure"].clone();
    changed["attributes"] =
        json!({"strength":3,"speed":0,"intellect":5,"willpower":5,"awareness":5,"presence":5});
    manifest["creatures"]["species"]["changed"] = changed;
    let racial = json!({"source":"racial","purchases":[]});
    let base_body = json!({"cells":[[0,0,0]],"eye":[0,0,0],"mass":80});
    let new_body = json!({"cells":if blocked {json!([[0,0,0],[8,0,0]])} else {json!([[0,0,0],[0,0,1]])},"eye":[0,0,0],"mass":120});
    manifest["creatures"]["body_packages"] = json!({
        BASE:{"species":"figure","body":base_body,"hit_dice":[racial.clone()],"polymorph_eligible":false},
        FORM:{"species":"changed","body":new_body,"hit_dice":[racial.clone(),racial.clone()],"polymorph_eligible":true}
    });
    let creature = &mut manifest["characters"][0]["creature"];
    creature["body_package"] = json!(BASE);
    creature["hit_dice"] = json!([racial,{"source":"warrior","purchases":[]}]);
    let mut regions = source.region_defs().unwrap();
    let mut region = serde_json::to_value(&regions[0]).unwrap();
    region["items"] = json!([
        {"id":20,"at":[1,2,0],"archetype":"identify","quantity":3},
        {"id":30,"at":[1,2,0],"archetype":"ember"}
    ]);
    regions[0] = serde_json::from_value(region).unwrap();
    let checked =
        Package::from_parts(serde_json::from_value(manifest).unwrap(), regions, &rules).unwrap();
    // A real positive compiler/build gate precedes the negative blocked/rejected
    // gameplay witness. A fixture setup failure is not a regression success.
    checked.check().unwrap();
    checked.build(42, false).unwrap();
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

fn actor(saved: &Value) -> &Value {
    let index = saved["game"]["actors"].as_u64().unwrap() as usize;
    &saved["shared"]["actors"][index][OWNER.0.to_string()]
}
fn creature(saved: &Value) -> &Value {
    &actor(saved)["combat"]["creature"]
}
fn charges(saved: &Value) -> u64 {
    let index = saved["game"]["items"].as_u64().unwrap() as usize;
    saved["shared"]["items"][index]["30"]["charges"]
        .as_u64()
        .unwrap()
}
fn personal(saved: &Value) -> Vec<(Value, Value)> {
    let build = &creature(saved)["build"];
    build["hd"]
        .as_array()
        .unwrap()
        .iter()
        .zip(build["choices"].as_array().unwrap())
        .filter(|(hd, _)| hd["owner"]["kind"] == "personal")
        .map(|(hd, choices)| (hd.clone(), choices.clone()))
        .collect()
}
fn identified(wire: &mut Wire, wand: ItemTarget) -> bool {
    wire.snapshot(OWNER)
        .state
        .observation
        .inventory
        .iter()
        .find(|row| row.id == wand)
        .unwrap()
        .identified
}
fn prepare(wire: &mut Wire) -> (ItemTarget, u64) {
    // The fixture owns exactly one physical wand, ID30.
    let wand = wire.take(ItemClass::Wand);
    assert_eq!(charges(&wire.checkpoint()), 3);
    assert!(!identified(wire, wand));
    let invalid = wire.request(
        OWNER,
        Action::Zap {
            item: wand,
            aim: DeviceAim::SelfTarget {},
        },
    );
    let untouched = wire.checkpoint();
    rejected(wire.send(OWNER, "concealed-self", invalid));
    assert_eq!(
        wire.checkpoint(),
        untouched,
        "unknown self aim is rejected without payment or RNG"
    );
    let scroll = wire.take(ItemClass::Scroll);
    let before = wire.checkpoint();
    wire.act("identify-read", Action::Read { item: scroll });
    let paid = wire.checkpoint();
    let read_ticks =
        paid["game"]["tick"].as_u64().unwrap() - before["game"]["tick"].as_u64().unwrap();
    assert!(read_ticks > 0);
    let choice = choice(&wire.snapshot(OWNER));
    wire.act(
        "identify-choice",
        Action::Identify {
            choice: choice.choice,
            item: Some(wand),
        },
    );
    assert!(identified(wire, wand));
    (wand, read_ticks)
}
fn assert_public_projection_is_private(snapshot: &Snapshot) {
    let public = serde_json::to_string(snapshot).unwrap();
    for secret in [
        BASE,
        FORM,
        "body_build",
        "personal_seed_offset",
        "purchase_highwater",
        "health_root",
    ] {
        assert!(
            !public.contains(secret),
            "ordinary snapshot disclosed {secret}: {public}"
        );
    }
}

#[test]
fn paid_self_polymorph_keeps_personal_ownership_and_reopens_without_source_or_reroll() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("source");
    let database = directory.path().join("polymorph.db");
    let mut wire = Wire::new(Engine::open(&database, source(&root, false)).unwrap());
    let (wand, read_ticks) = prepare(&mut wire);
    let before = wire.checkpoint();
    assert_eq!(
        creature(&before)["build"]["body_build"]["active"]["id"],
        BASE
    );
    assert_eq!(
        creature(&before)["build"]["hd"].as_array().unwrap().len(),
        2
    );
    assert_eq!(personal(&before).len(), 1);
    assert_eq!(
        personal(&before)[0].0["source"],
        json!({"kind":"class","id":"warrior"})
    );
    let request = wire.request(
        OWNER,
        Action::Zap {
            item: wand,
            aim: DeviceAim::SelfTarget {},
        },
    );
    let admitted = ack(wire.send(OWNER, "polymorph-once", request.clone()));
    let receipt = wire.finish(OWNER, "polymorph-once", &request);
    assert_eq!(admitted.entry_id(), receipt.entry_id());
    let saved = wire.checkpoint();
    assert_eq!(charges(&saved), 2);
    assert_eq!(
        saved["game"]["tick"].as_u64().unwrap() - before["game"]["tick"].as_u64().unwrap(),
        read_ticks
    );
    assert_eq!(
        creature(&saved)["build"]["body_build"]["active"]["id"],
        FORM
    );
    assert_eq!(creature(&saved)["build"]["body_build"]["base"]["id"], BASE);
    assert_eq!(creature(&saved)["build"]["hd"].as_array().unwrap().len(), 3);
    assert_eq!(
        personal(&saved),
        personal(&before),
        "stable class Health seed, owner and funded choices survive changed effective row index"
    );
    for field in [
        "earned_progress",
        "injury",
        "dead",
        "polymorph_return_pending",
    ] {
        assert_eq!(creature(&saved)[field], creature(&before)[field], "{field}");
    }
    for field in [
        "starting_skills",
        "intellect_learning",
        "binding",
        "strength_acquired",
    ] {
        assert_eq!(
            creature(&saved)["build"][field],
            creature(&before)["build"][field],
            "{field}"
        );
    }
    let attributes = &creature(&saved)["build"]["initial_attributes"];
    assert_eq!(attributes["strength"], 3);
    for attribute in ["intellect", "willpower", "awareness", "presence"] {
        assert_eq!(attributes[attribute], 0, "retained personal {attribute}");
    }
    assert_public_projection_is_private(&wire.snapshot(OWNER));
    assert_eq!(
        ack(wire.send(OWNER, "polymorph-once", request.clone())),
        receipt
    );
    assert_eq!(
        wire.checkpoint(),
        saved,
        "receipt retry does not draw, transform or pay twice"
    );
    wire.service.engine.flush().unwrap();
    let serial = wire.serial;
    drop(wire);
    std::fs::remove_dir_all(&root).unwrap();
    let engine = Engine::open(&database, crate::Scenario::two_room(999)).unwrap();
    assert_eq!(
        checkpoint(&engine),
        saved,
        "restore uses the saved package/ledger, not deleted authoring or a reroll"
    );
    let mut wire = Wire::new(engine);
    wire.serial = serial;
    assert_public_projection_is_private(&wire.snapshot(OWNER));
    let reopened = wire.checkpoint();
    assert_eq!(ack(wire.send(OWNER, "polymorph-once", request)), receipt);
    assert_eq!(wire.checkpoint(), reopened);
    let mut spectator = wire
        .service
        .connect(
            &Account {
                role: AccessRole::Spectator,
                user: "polymorph-observer".into(),
                token: "fixture".into(),
                actors: BTreeSet::from([OWNER]),
            },
            "headless".into(),
        )
        .unwrap();
    wire.service.handle(
        spectator.id,
        "observe".into(),
        Request::Attach { actor: OWNER },
    );
    wire.service
        .handle(spectator.id, "observe-snapshot".into(), Request::Snapshot);
    let observed = std::iter::from_fn(|| spectator.messages.try_recv_frame().ok())
        .filter_map(|frame| match decode_response(&frame.text).unwrap() {
            ServerMessage::Snapshot { snapshot, .. } => Some(snapshot),
            ServerMessage::Error { code, .. } => panic!("spectator attach: {code:?}"),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(!observed.is_empty(), "real observer snapshot required");
    for snapshot in observed {
        assert_public_projection_is_private(&snapshot);
    }
}

#[test]
fn paid_blocked_self_and_unknown_empty_ray_publish_no_form_or_private_roster() {
    let directory = tempfile::tempdir().unwrap();
    let mut wire = Wire::new(Engine::memory(source(directory.path(), true)).unwrap());
    let wand = wire.take(ItemClass::Wand);
    let before_miss = wire.checkpoint();
    wire.act(
        "empty-ray",
        Action::Zap {
            item: wand,
            aim: DeviceAim::Direction {
                direction: Direction::West,
            },
        },
    );
    let missed = wire.checkpoint();
    assert_eq!(charges(&missed), charges(&before_miss) - 1);
    assert_eq!(creature(&missed)["build"], creature(&before_miss)["build"]);
    assert!(
        !identified(&mut wire, wand),
        "a released but indiscernible ray cannot identify"
    );
    assert_public_projection_is_private(&wire.snapshot(OWNER));
    let scroll = wire.take(ItemClass::Scroll);
    wire.act("blocked-identify-read", Action::Read { item: scroll });
    let choice = choice(&wire.snapshot(OWNER));
    wire.act(
        "blocked-identify-choice",
        Action::Identify {
            choice: choice.choice,
            item: Some(wand),
        },
    );
    let before = wire.checkpoint();
    let request = wire.request(
        OWNER,
        Action::Zap {
            item: wand,
            aim: DeviceAim::SelfTarget {},
        },
    );
    ack(wire.send(OWNER, "blocked-once", request.clone()));
    let receipt = wire.finish(OWNER, "blocked-once", &request);
    let after = wire.checkpoint();
    assert_eq!(
        charges(&after),
        1,
        "one charge remains after the miss and blocked paid cast"
    );
    assert!(after["game"]["tick"].as_u64().unwrap() > before["game"]["tick"].as_u64().unwrap());
    assert_eq!(
        creature(&after)["build"],
        creature(&before)["build"],
        "no legal footprint: do not select a substitute or modify the body ledger"
    );
    assert_public_projection_is_private(&wire.snapshot(OWNER));
    assert_eq!(ack(wire.send(OWNER, "blocked-once", request)), receipt);
    assert_eq!(wire.checkpoint(), after);
}
