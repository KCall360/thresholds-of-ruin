use super::*;
fn fixture() -> Value {
    json!({"version":1,"hz":50,"viewport":[640,480],
        "actors":[{"id":"woman","model":"Human","seed":7,"body":{"woman":true},
        "outfit":"royal-princess","equipment":[],"position":[0,0,0],"yaw":0}],
        "cameras":[{"id":"front","position":[0,1.5,4],"target":[0,0.5,0],"fov_degrees":40}],
        "events":[],"captures":[{"tick":0,"name":"front","camera":"front"}]})
}
fn parse(value: Value) -> Result<Script, String> {
    Script::parse(&serde_json::to_vec(&value).unwrap())
}
#[test]
fn script_resolves_exact_compositions_and_rejects_fallbacks() {
    let script = parse(fixture()).unwrap();
    assert!(script.actors[0].look().unwrap().woman());
    for (field, value) in [
        ("model", json!("unknown")),
        ("outfit", json!("unknown")),
        ("equipment", json!(["shield"])),
        ("model", json!("Dragon")),
        ("body", json!({"woman":false})),
    ] {
        let mut v = fixture();
        v["actors"][0][field] = value;
        assert!(parse(v).is_err(), "{field}");
    }
}
#[test]
fn script_rejects_unknown_fields_duplicates_references_and_paths() {
    let mut v = fixture();
    v["typo"] = json!(1);
    assert!(parse(v).is_err());
    let mut v = fixture();
    v["actors"][0]["body"]["typo"] = json!(1);
    assert!(parse(v).is_err());
    let mut v = fixture();
    let a = v["actors"][0].clone();
    v["actors"].as_array_mut().unwrap().push(a);
    assert!(parse(v).is_err());
    for name in ["../escape", "a/b", "a\\b", "", "C:evil"] {
        let mut v = fixture();
        v["captures"][0]["name"] = json!(name);
        assert!(parse(v).is_err());
    }
    let mut v = fixture();
    v["captures"][0]["camera"] = json!("missing");
    assert!(parse(v).is_err());
    let mut v = fixture();
    v["cameras"][0]["position"] = v["cameras"][0]["target"].clone();
    assert!(parse(v).is_err());
}
#[test]
fn events_validate_order_and_compositions_in_sequence() {
    let mut v = fixture();
    v["captures"][0]["tick"] = json!(10);
    v["events"] = json!([
        {"tick":1,"actor":"woman","operation":{"type":"dress","outfit":"leotard"}},
        {"tick":1,"actor":"woman","operation":{"type":"equipment","items":["breastplate"]}},
        {"tick":2,"actor":"woman","operation":{"type":"action","name":"jump"}}]);
    let script = parse(v.clone()).unwrap();
    assert_eq!(script.events.len(), 3);
    v["events"][2]["tick"] = json!(0);
    assert!(parse(v).is_err());
    let mut v = fixture();
    v["events"] =
        json!([{"tick":0,"actor":"missing","operation":{"type":"velocity","value":[0,0,1]}}]);
    assert!(parse(v).is_err());
}
#[test]
fn script_clock_holds_all_capture_phases_and_advances_once() {
    let script = parse(fixture()).unwrap();
    let mut app = App::new();
    app.add_plugins(bevy::time::TimePlugin);
    app.insert_resource(Run {
        script,
        output: PathBuf::new(),
        tick: 0,
        next_capture: 0,
        phase: Phase::Pose,
        clock: Time::default(),
        deadline: Instant::now() + Duration::from_secs(60),
        manifest: json!({}),
        error: None,
    });
    app.add_systems(First, clock.after(TimeSystem));
    for phase in [Phase::Pose, Phase::Camera, Phase::Pending] {
        app.world_mut().resource_mut::<Run>().phase = phase;
        for _ in 0..5 {
            app.update();
            assert_eq!(app.world().resource::<Time>().delta(), Duration::ZERO);
        }
        assert_eq!(app.world().resource::<Run>().tick, 0);
    }
    app.world_mut().resource_mut::<Run>().phase = Phase::Advance;
    app.update();
    assert_eq!(app.world().resource::<Run>().tick, 1);
    assert_eq!(
        app.world().resource::<Time>().delta(),
        Duration::from_millis(20)
    );
    app.update();
    assert_eq!(app.world().resource::<Run>().tick, 1);
    assert_eq!(app.world().resource::<Time>().delta(), Duration::ZERO);
}

#[test]
fn script_rejects_record_bounds_and_unsupported_operations() {
    for (key, value) in [
        ("version", json!(2)),
        ("hz", json!(0)),
        ("hz", json!(241)),
        ("viewport", json!([319, 480])),
        ("viewport", json!([640, 2049])),
        ("actors", json!([])),
        ("cameras", json!([])),
        ("captures", json!([])),
    ] {
        let mut v = fixture();
        v[key] = value;
        assert!(parse(v).is_err(), "{key}");
    }
    for name in ["CON", "nul", "LPT1", "COM9"] {
        let mut v = fixture();
        v["captures"][0]["name"] = json!(name);
        assert!(parse(v).is_err());
    }
    for op in [
        json!({"type":"action","name":"jump"}),
        json!({"type":"action","name":"unknown"}),
        json!({"type":"velocity","value":[100,0,0]}),
        json!({"type":"teleport","position":[0,0,0],"yaw":361}),
        json!({"type":"unknown"}),
    ] {
        let mut v = fixture();
        v["events"] = json!([{"tick":0,"actor":"woman","operation":op}]);
        assert!(parse(v).is_err());
    }
    let mut v = fixture();
    let capture = v["captures"][0].clone();
    v["captures"].as_array_mut().unwrap().push(capture);
    assert!(parse(v).is_err());
    let mut v = fixture();
    v["captures"][0]["tick"] = json!(6001);
    assert!(parse(v).is_err());
    let mut v = fixture();
    v["actors"][0]["equipment"] = json!(["helm", "helm"]);
    assert!(parse(v).is_err());
    let mut v = fixture();
    v["actors"][0]["body"]["bust"] = json!(5);
    assert!(parse(v).is_err());
    assert!(Script::parse(&vec![b' '; 1_048_577]).is_err());
}
#[test]
fn director_timeout_records_failure_without_advancing_or_claiming_captures() {
    let output = std::env::temp_dir().join(format!("tor-review-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&output).unwrap();
    let mut app = App::new();
    app.add_event::<AppExit>();
    app.insert_resource(Readiness::default());
    app.insert_resource(Run {
        script: parse(fixture()).unwrap(),
        output: output.clone(),
        tick: 0,
        next_capture: 0,
        phase: Phase::Pending,
        clock: Time::default(),
        deadline: Instant::now() - Duration::from_secs(1),
        manifest: json!({"status":"running","captures":[]}),
        error: None,
    });
    app.add_systems(Update, capture);
    app.update();
    let state = app.world().resource::<Run>();
    assert_eq!(state.phase, Phase::Finished);
    assert_eq!(state.tick, 0);
    assert_eq!(state.next_capture, 0);
    let report: Value =
        serde_json::from_slice(&std::fs::read(output.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(report["status"], "failed");
    assert_eq!(report["captures"], json!([]));
    std::fs::remove_file(output.join("manifest.json")).unwrap();
    std::fs::remove_dir(output).unwrap();
}

#[test]
fn composition_events_preserve_motion_and_skip_identical_requests() {
    let mut spec = parse(fixture()).unwrap().actors.remove(0);
    let mut velocity = Vec3::X;
    let mut transform = Transform::from_xyz(2.0, 0.0, 3.0);
    let mut anim = Anim::new(spec.seed, transform.translation);
    anim.action = Some((creatures::Move::Jump, 0.3));
    let dress = Operation::Dress {
        outfit: "leotard".into(),
    };
    assert!(apply_operation(
        &mut spec,
        &mut velocity,
        &mut anim,
        &mut transform,
        &dress
    ));
    assert!(!apply_operation(
        &mut spec,
        &mut velocity,
        &mut anim,
        &mut transform,
        &dress
    ));
    let gear = Operation::Equipment {
        items: vec!["breastplate".into()],
    };
    assert!(apply_operation(
        &mut spec,
        &mut velocity,
        &mut anim,
        &mut transform,
        &gear
    ));
    assert!(!apply_operation(
        &mut spec,
        &mut velocity,
        &mut anim,
        &mut transform,
        &gear
    ));
    assert_eq!(spec.outfit, "leotard");
    assert_eq!(spec.equipment, ["breastplate"]);
    assert_eq!(velocity, Vec3::X);
    assert_eq!(transform.translation, Vec3::new(2.0, 0.0, 3.0));
    assert!(matches!(anim.action, Some((creatures::Move::Jump, age)) if age == 0.3));
}

#[test]
fn motion_events_change_boundary_state_without_integrating_time() {
    let mut spec = parse(fixture()).unwrap().actors.remove(0);
    let mut velocity = Vec3::ZERO;
    let mut transform = Transform::default();
    let mut anim = Anim::new(spec.seed, Vec3::ZERO);
    anim.action = Some((creatures::Move::Jump, 0.3));
    for operation in [
        Operation::Velocity {
            value: [3.0, 0.0, 0.0],
        },
        Operation::Teleport {
            position: [2.0, 0.0, 4.0],
            yaw: 90.0,
        },
        Operation::Velocity {
            value: [1.0, 0.0, 0.0],
        },
    ] {
        assert!(!apply_operation(
            &mut spec,
            &mut velocity,
            &mut anim,
            &mut transform,
            &operation
        ));
    }
    assert_eq!(velocity, Vec3::X);
    assert_eq!(transform.translation, Vec3::new(2.0, 0.0, 4.0));
    assert!(anim.action.is_none());
    assert_eq!(anim.yaw, 90_f32.to_radians());
    assert_eq!(anim.target_yaw, Some(anim.yaw));
}

#[test]
fn action_events_restart_and_cancel_without_changing_composition_or_motion() {
    let mut spec = parse(fixture()).unwrap().actors.remove(0);
    let mut velocity = Vec3::X;
    let mut transform = Transform::default();
    let mut anim = Anim::new(spec.seed, Vec3::ZERO);
    anim.action = Some((creatures::Move::Jump, 0.3));
    for name in ["jump", "cancel"] {
        assert!(!apply_operation(
            &mut spec,
            &mut velocity,
            &mut anim,
            &mut transform,
            &Operation::Action { name: name.into() }
        ));
        if name == "jump" {
            assert!(matches!(anim.action, Some((creatures::Move::Jump, 0.0))));
        } else {
            assert!(anim.action.is_none());
        }
    }
    assert_eq!(spec.outfit, "royal-princess");
    assert!(spec.equipment.is_empty());
    assert_eq!(velocity, Vec3::X);
    assert_eq!(transform.translation, Vec3::ZERO);
}

#[test]
fn capture_names_are_unique_on_case_insensitive_filesystems() {
    let mut value = fixture();
    value["captures"]
        .as_array_mut()
        .unwrap()
        .push(json!({"tick":0,"name":"FRONT","camera":"front"}));
    assert!(
        parse(value.clone()).is_err(),
        "front.png and FRONT.png collide on Windows"
    );
    value["captures"][1]["name"] = json!("SIDE");
    assert!(
        parse(value).is_ok(),
        "uppercase names remain valid when distinct"
    );
}

#[test]
fn live_timeline_does_not_depend_on_capture_requests() {
    let mut value = fixture();
    value.as_object_mut().unwrap().remove("captures");
    value["events"] =
        json!([{"tick":7500,"actor":"woman","operation":{"type":"action","name":"jump"}}]);
    let bytes = serde_json::to_vec(&value).unwrap();
    assert!(
        Script::parse_live(&bytes).is_ok(),
        "150-second live event needs no screenshot"
    );
    assert!(
        Script::parse(&bytes).is_err(),
        "capture still requires captures"
    );
    value["captures"] = json!([{"tick":0,"name":"unused","camera":"front"}]);
    let bytes = serde_json::to_vec(&value).unwrap();
    assert!(
        Script::parse_live(&bytes).is_ok(),
        "ignored screenshot cannot truncate live events"
    );
    assert!(
        Script::parse(&bytes).is_err(),
        "capture events must precede its last image"
    );
    value["events"][0]["tick"] = json!(30001);
    assert!(Script::parse_live(&serde_json::to_vec(&value).unwrap()).is_err());
}
