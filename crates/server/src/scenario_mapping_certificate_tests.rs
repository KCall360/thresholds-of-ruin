// STAGED ONLY. Register as a cfg(test) child of scenario_spatial_runtime.rs.
// Uses current APIs/JSON so the first source-decoder assertion can compile
// before production implementation. No execution is claimed by this file.
use super::*;
use serde_json::{json, Value};

fn source() -> Value {
    json!({
        "members": [1],
        "rooms": {"1": [{"region":1,"min":[0,0,0],"max":[6,4,1]}]},
        "passages": [], "doorways": []
    })
}

fn certificate(up: &str) -> Value {
    json!({"room_frames":[{"room":1,"region":1,"up":up}]})
}

fn decode(value: Value) -> Result<spatial_authoring::AuthoredSpatialScope, serde_json::Error> {
    serde_json::from_value(value)
}

fn certified_package(up: &str) -> Package {
    let base = super::tests::package();
    base.check().unwrap();
    let mut manifest = serde_json::to_value(&base.manifest).unwrap();
    manifest["spatial_scopes"]["study"]["mapping"] = certificate(up);
    let parsed = serde_json::from_value(manifest);
    assert!(
        parsed.is_ok(),
        "valid explicit source certificate rejected: {parsed:?}"
    );
    let package = Package::from_parts(
        parsed.unwrap(),
        base.region_defs().unwrap(),
        &base.rules_source,
    )
    .unwrap();
    package
        .check()
        .expect("valid complete construction certificate");
    package
}

#[test]
fn omitted_mapping_preserves_authoring_and_serializes_explicit_uncertified_state() {
    let ordinary = decode(source()).expect("existing source remains valid");
    let encoded = serde_json::to_value(ordinary).unwrap();
    assert!(
        encoded.as_object().unwrap().contains_key("mapping"),
        "source serialization must distinguish explicit uncertified state"
    );
    assert_eq!(encoded["mapping"], Value::Null);
    let mut explicit = source();
    explicit["mapping"] = Value::Null;
    assert!(decode(explicit).is_ok());
    // Existing fully checked package is also lawful without metadata.
    let ordinary = super::tests::package();
    ordinary.check().unwrap();
    SpatialAudit::new(Some(&ordinary), 42)
        .unwrap()
        .check(&ordinary.build(42, true).unwrap())
        .unwrap();
}

#[test]
fn all_six_explicit_construction_axes_are_lawful_not_only_local_up() {
    decode(source()).expect("uncertified positive control");
    for up in ["north", "east", "south", "west", "up", "down"] {
        let mut value = source();
        value["mapping"] = certificate(up);
        let parsed = decode(value.clone());
        assert!(
            parsed.is_ok(),
            "valid {up} construction certificate rejected: {parsed:?}"
        );
        let parsed = parsed.unwrap();
        assert_eq!(serde_json::to_value(&parsed).unwrap(), value);
        let authored = toml::to_string(&parsed).unwrap();
        let roundtrip: spatial_authoring::AuthoredSpatialScope = toml::from_str(&authored).unwrap();
        assert_eq!(serde_json::to_value(roundtrip).unwrap(), value);
    }
}

#[test]
fn certified_passage_only_scope_has_a_complete_empty_frame_set() {
    let mut value = json!({"members":[1],"rooms":{},"passages":[
        {"region":1,"z":0,"y":0,"first_x":0,"last_x":2}
    ],"doorways":[]});
    decode(value.clone()).unwrap();
    value["mapping"] = json!({"room_frames":[]});
    let parsed = decode(value.clone());
    assert!(
        parsed.is_ok(),
        "zero room incidences require zero frames: {parsed:?}"
    );
    assert_eq!(serde_json::to_value(parsed.unwrap()).unwrap(), value);
}

#[test]
fn certificate_records_are_closed_named_and_duplicate_safe_after_valid_positive() {
    let mut good = source();
    good["mapping"] = certificate("up");
    decode(good.clone()).expect("valid certificate before decoder negatives");
    for bad in [
        Value::Null,
        json!([]),
        json!("up"),
        json!({}),
        json!({"room":1,"region":1}),
        json!({"room":1,"region":1,"up":"north_east"}),
        json!({"room":1,"region":1,"up":"Up"}),
        json!({"room":1,"region":1,"up":0}),
        json!({"room":1,"region":1,"up":"up","gravity":[0,0,-1]}),
    ] {
        let mut value = good.clone();
        value["mapping"]["room_frames"][0] = bad;
        assert!(decode(value).is_err());
    }
    for bad in [
        json!([]),
        json!("up"),
        json!({}),
        json!({"room_frames":null}),
        json!({"room_frames":[],"transforms":[]}),
    ] {
        let mut value = good.clone();
        value["mapping"] = bad;
        assert!(decode(value).is_err());
    }
    let good = serde_json::to_string(&good).unwrap();
    for duplicate in [
        good.replacen("\"room\":1", "\"room\":1,\"room\":1", 1),
        good.replacen("\"up\":\"up\"", "\"up\":\"up\",\"up\":\"up\"", 1),
        good.replacen("\"room_frames\":", "\"room_frames\":[],\"room_frames\":", 1),
    ] {
        assert_ne!(duplicate, good);
        assert!(
            serde_json::from_str::<spatial_authoring::AuthoredSpatialScope>(&duplicate).is_err()
        );
    }
}

#[test]
fn supplied_partial_foreign_and_duplicate_frames_fail_source_validation() {
    let good = certified_package("up");
    let frames = [
        json!([]),
        json!([{"room":1,"region":2,"up":"up"}]),
        json!([{"room":2,"region":1,"up":"up"}]),
        json!([{"room":0,"region":1,"up":"up"}]),
        json!([{"room":1,"region":1,"up":"up"},{"room":1,"region":1,"up":"up"}]),
        json!([{"room":1,"region":1,"up":"up"},{"room":1,"region":1,"up":"east"}]),
    ];
    for frames in frames {
        let mut manifest = serde_json::to_value(&good.manifest).unwrap();
        manifest["spatial_scopes"]["study"]["mapping"]["room_frames"] = frames;
        let invalid = Package::from_parts(
            serde_json::from_value(manifest).unwrap(),
            good.region_defs().unwrap(),
            &good.rules_source,
        )
        .unwrap();
        // Reconstructing the package preserves source integrity: this failure
        // must not be a stale pinned source hash from mutating a checked package.
        let error = invalid
            .check()
            .expect_err("bad supplied certificate must fail even if unused");
        assert!(
            error.message.to_lowercase().contains("mapping"),
            "{error:?}"
        );
        assert!(!error.message.to_lowercase().contains("hash"), "{error:?}");
    }
}

#[test]
fn source_free_content_retains_certificate_and_recompiles_identical_scope() {
    let package = certified_package("east");
    let game = package.build(42, true).unwrap();
    SpatialAudit::new(Some(&package), 42)
        .unwrap()
        .check(&game)
        .unwrap();
    let bytes = crate::storage::schema::scenario::encode_content_package(&package).unwrap();
    let restored = crate::storage::schema::scenario::decode_content_package(&bytes).unwrap();
    assert!(
        restored.directory.is_none(),
        "source-free input must not retain a path dependency"
    );
    restored.check().unwrap();
    let original = package
        .compile_authored_spatial("study", 42, &mut 0, &mut 0)
        .unwrap();
    let recertified = restored
        .compile_authored_spatial("study", 42, &mut 0, &mut 0)
        .unwrap();
    assert_eq!(
        original, recertified,
        "complete scope equality includes certified oriented faces"
    );
    SpatialAudit::new(Some(&restored), 42)
        .unwrap()
        .check(&game)
        .unwrap();
}

#[test]
fn saved_certificate_requires_explicit_nullable_field_and_closed_nested_records() {
    let ordinary = super::tests::package();
    let good = crate::storage::schema::scenario::encode_content_package(&ordinary).unwrap();
    crate::storage::schema::scenario::decode_content_package(&good).unwrap();
    let mut value: Value = serde_json::from_slice(&good).unwrap();
    let scope = &mut value["package"]["manifest"]["spatial_scopes"]["study"];
    assert_eq!(scope["mapping"], Value::Null);
    assert!(scope.as_object_mut().unwrap().remove("mapping").is_some());
    assert!(crate::storage::schema::scenario::decode_content_package(
        &serde_json::to_vec(&value).unwrap()
    )
    .is_err());

    let certified = certified_package("up");
    let bytes = crate::storage::schema::scenario::encode_content_package(&certified).unwrap();
    crate::storage::schema::scenario::decode_content_package(&bytes).unwrap();
    let good: Value = serde_json::from_slice(&bytes).unwrap();
    for key in ["room", "region", "up"] {
        let mut invalid = good.clone();
        invalid["package"]["manifest"]["spatial_scopes"]["study"]["mapping"]["room_frames"][0]
            .as_object_mut()
            .unwrap()
            .remove(key)
            .unwrap();
        assert!(crate::storage::schema::scenario::decode_content_package(
            &serde_json::to_vec(&invalid).unwrap()
        )
        .is_err());
    }
}
