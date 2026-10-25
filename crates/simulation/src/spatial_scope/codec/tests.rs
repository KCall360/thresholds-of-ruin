// Standalone codec tests; no Game/source-free Service claims.
use super::*;
use serde_json::{json, Value};

fn source_scope() -> Scope {
    // Typed structural >64-member witness, NOT a canonical authored package.
    Scope {
        mapping: crate::spatial_scope::mapping::MappingCertification::Uncertified,
        id: "large-members".into(),
        members: (0..65).map(tor_world::RegionId).collect(),
        primary: vec![],
        rooms: vec![],
    }
}
fn limits() -> DecodeLimits {
    DecodeLimits {
        charge_bytes: 64 * 1024,
        root_references: 1,
        directories: 1,
    }
}
fn source_json() -> Value {
    json!({"scopes":[{"id":"large-members","members":(0..65).collect::<Vec<u64>>(),"primary":[],"rooms":[],"mapping":{"status":"uncertified"}}],"directories":[[0]]})
}
fn decode(value: &Value) -> Result<DecodedPool, serde_json::Error> {
    let bytes = serde_json::to_vec(value).unwrap();
    let mut de = serde_json::Deserializer::from_slice(&bytes);
    let pool = PoolSeed(limits()).deserialize(&mut de)?;
    de.end()?;
    Ok(pool)
}
#[test]
fn maximum_width_closed_shapes_match_independent_charge_skeletons() {
    let run =
        json!({"region":u64::MAX,"z":i32::MIN,"y":i32::MIN,"first_x":i32::MIN,"last_x":i32::MIN});
    let membership = json!({"primary":{"kind":"room","id":u32::MAX},"doorway":u32::MAX});
    let passage = json!({"primary":{"kind":"passage","id":u32::MAX},"doorway":null});
    assert_eq!(serde_json::to_vec(&run).unwrap().len(), 106);
    assert_eq!(serde_json::to_vec(&membership).unwrap().len(), 64);
    assert_eq!(serde_json::to_vec(&passage).unwrap().len(), 61);
    assert_eq!(
        serde_json::to_vec(&json!({"cells":run,"membership":membership}))
            .unwrap()
            .len(),
        194
    );
    assert_eq!(
        serde_json::to_vec(&json!({"room":u32::MAX,"runs":[]}))
            .unwrap()
            .len(),
        29
    );
    assert_eq!(
        serde_json::to_vec(&json!({"id":"","members":[],"primary":[],"rooms":[],"mapping":{"status":"uncertified"}}))
            .unwrap()
            .len(),
        81
    );
    assert_eq!(
        serde_json::to_vec(&json!({"scopes":[],"directories":[]}))
            .unwrap()
            .len(),
        30
    );
    for label in ["\"\\", "\u{0}\u{1}\u{1f}", "é界", "plain"] {
        assert!(serde_json::to_vec(label).unwrap().len() <= 2 + 6 * label.len());
    }
    // These controls validate arithmetic only. Later compare REAL EncodedPool
    // serializer max operands/leaf adapters, not merely these independent Values.
}
#[test]
fn standalone_named_pool_admits_more_than_sixty_four_members() {
    let scope = source_scope();
    let budget = super::super::registry::Budget {
        scopes: 1,
        members: 65,
        rooms: 0,
        passages: 0,
        runs: 0,
        expanded_refs: 0,
        label_bytes: scope.id.len(),
    };
    assert_eq!(scope.validate(&budget).unwrap().members, 65);
    let decoded = decode(&source_json()).expect("valid named standalone pool");
    let actual = decoded
        .validate_roots(&[0])
        .expect("all pool entries referenced by valid root");
    assert_eq!(&*actual.scopes[0], &scope);
    assert_eq!(actual.directories, vec![vec![0]]);
    assert_eq!(actual.roots, vec![0]);
    assert_eq!(
        serde_json::to_value(EncodedPool(&actual)).unwrap(),
        source_json()
    );
}
#[test]
fn standalone_pool_rejects_positional_missing_duplicate_and_unknown_fields() {
    // Positive must succeed before malformed cases are credited. At no-op
    // scaffold this is an intended valid-decode red, not parser qualification.
    let positive = source_json();
    assert!(decode(&positive).is_ok());
    assert!(decode(&json!([positive["scopes"], positive["directories"]])).is_err());
    let mut missing = positive.clone();
    missing.as_object_mut().unwrap().remove("directories");
    assert!(decode(&missing).is_err());
    let mut unknown = positive.clone();
    unknown["extra"] = json!(0);
    assert!(decode(&unknown).is_err());
    let raw = r#"{"scopes":[],"scopes":[],"directories":[]}"#;
    assert!(PoolSeed(limits())
        .deserialize(&mut serde_json::Deserializer::from_str(raw))
        .is_err());
}
// Next after valid codec red: all nested object malformed variants, required-null
// doorway, exact charge and one-over sibling entry, unread-tail seed sentinel,
// semantic duplicate/index alias, rooted sharing and omitted-root negatives.

// Standalone strictness/budget controls; no Game or source-free Service claims.
fn decode_with(value: &Value, limits: DecodeLimits) -> Result<DecodedPool, serde_json::Error> {
    let bytes = serde_json::to_vec(value).unwrap();
    let mut de = serde_json::Deserializer::from_slice(&bytes);
    let pool = PoolSeed(limits).deserialize(&mut de)?;
    de.end()?;
    Ok(pool)
}
fn rich_json(label: &str) -> Value {
    let run =
        json!({"region":u64::MAX,"z":i32::MIN,"y":i32::MIN,"first_x":i32::MIN,"last_x":i32::MIN});
    json!({"scopes":[{"id":label,"members":[u64::MAX],"primary":[{"cells":run.clone(),"membership":{"primary":{"kind":"room","id":u32::MAX},"doorway":u32::MAX}}],"rooms":[{"room":u32::MAX,"runs":[run]}],"mapping":{"status":"uncertified"}}],"directories":[[0]]})
}
#[test]
fn actual_writer_maximum_operands_fit_exact_conservative_charge() {
    let label = "\"\\é界";
    let value = rich_json(label);
    // 30pool+21root+47scope+21member+195primary+30room+107run+75mapping+3dir+11index.
    let charge = 540 + 6 * label.len() as u64;
    let exact = DecodeLimits {
        charge_bytes: charge,
        root_references: 1,
        directories: 1,
    };
    let pool = decode_with(&value, exact)
        .unwrap()
        .validate_roots(&[0])
        .unwrap();
    let encoded = serde_json::to_vec(&EncodedPool(&pool)).unwrap();
    assert_eq!(serde_json::from_slice::<Value>(&encoded).unwrap(), value);
    assert!(encoded.len() as u64 <= charge - 21);
    assert!(decode_with(
        &value,
        DecodeLimits {
            charge_bytes: charge - 1,
            ..exact
        }
    )
    .is_err());
}
#[test]
fn every_nested_named_boundary_rejects_shape_and_field_mutations() {
    let base = rich_json("valid");
    assert!(decode(&base).unwrap().validate_roots(&[0]).is_ok());
    for path in [
        "",
        "/scopes/0",
        "/scopes/0/primary/0",
        "/scopes/0/primary/0/cells",
        "/scopes/0/primary/0/membership",
        "/scopes/0/primary/0/membership/primary",
        "/scopes/0/rooms/0",
        "/scopes/0/rooms/0/runs/0",
    ] {
        let fields = base.pointer(path).unwrap().as_object().unwrap();
        let mut positional = base.clone();
        *positional.pointer_mut(path).unwrap() = Value::Array(fields.values().cloned().collect());
        assert!(decode(&positional).is_err(), "positional {path}");
        for key in fields.keys() {
            let mut missing = base.clone();
            missing
                .pointer_mut(path)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .remove(key);
            assert!(decode(&missing).is_err(), "missing {path}/{key}");
        }
        let mut unknown = base.clone();
        unknown.pointer_mut(path).unwrap()["extra"] = json!(0);
        assert!(decode(&unknown).is_err(), "unknown {path}");
    }
    let mut nullable = base.clone();
    nullable["scopes"][0]["primary"][0]["membership"]["doorway"] = Value::Null;
    assert!(decode(&nullable).unwrap().validate_roots(&[0]).is_ok());
    // Duplicate nested fields use raw JSON, never a Value map which erases them.
    let raw = serde_json::to_string(&base).unwrap();
    for (needle, replacement) in [
        (
            "\"first_x\":-2147483648",
            "\"first_x\":-2147483648,\"first_x\":0",
        ),
        (
            "\"doorway\":4294967295",
            "\"doorway\":4294967295,\"doorway\":null",
        ),
        ("\"kind\":\"room\"", "\"kind\":\"room\",\"kind\":\"room\""),
    ] {
        assert!(raw.contains(needle));
        let malformed = raw.replacen(needle, replacement, 1);
        assert!(PoolSeed(limits())
            .deserialize(&mut serde_json::Deserializer::from_str(&malformed))
            .is_err());
    }
}
#[test]
fn shared_payload_subsets_are_valid_but_historical_identity_alternatives_are_not() {
    let mut a = source_json()["scopes"][0].clone();
    a["id"] = json!("a");
    a["members"] = json!([1]);
    let mut b = a.clone();
    b["id"] = json!("b");
    b["members"] = json!([2]);
    let value = json!({"scopes":[a.clone(),b.clone()],"directories":[[0],[0,1]]});
    let two = DecodeLimits {
        root_references: 2,
        directories: 2,
        ..limits()
    };
    let pool = decode_with(&value, two)
        .unwrap()
        .validate_roots(&[0, 1])
        .unwrap();
    assert_eq!(pool.scopes.len(), 2);
    assert_eq!(pool.directories, vec![vec![0], vec![0, 1]]);
    // Distinct roots cannot reassign immutable region ownership, even when
    // conflicting payloads never appear in the same directory.
    b["members"] = json!([1]);
    let alias = json!({"scopes":[a,b],"directories":[[0],[1]]});
    assert!(decode_with(&alias, two)
        .unwrap()
        .validate_roots(&[0, 1])
        .is_err());
    assert!(decode_with(&value, two)
        .unwrap()
        .validate_roots(&[0])
        .is_err());
    assert!(decode_with(&value, two)
        .unwrap()
        .validate_roots(&[0, 2])
        .is_err());
    assert!(decode_with(&value, two)
        .unwrap()
        .validate_roots(&[0, 0])
        .is_err());
}
#[test]
fn budget_exhaustion_rejects_before_reading_nested_excess_value() {
    // Empty named pool framing30 + reservedroot21 =51. Any scope costs47;
    // reject BEFORE parsing the deliberately invalid first nested value.
    let small = DecodeLimits {
        charge_bytes: 51,
        root_references: 1,
        directories: 1,
    };
    let raw = r#"{"scopes":[{"id": THIS_MUST_NOT_BE_PARSED"#;
    let error = PoolSeed(small)
        .deserialize(&mut serde_json::Deserializer::from_str(raw))
        .unwrap_err();
    assert!(
        error.to_string().contains("spatial charge exceeds limit"),
        "{error}"
    );
    let none = DecodeLimits {
        directories: 0,
        ..limits()
    };
    let raw = r#"{"scopes":[],"directories":[THIS_MUST_NOT_BE_PARSED"#;
    let error = PoolSeed(none)
        .deserialize(&mut serde_json::Deserializer::from_str(raw))
        .unwrap_err();
    assert!(
        error.to_string().contains("spatial sequence exceeds limit"),
        "{error}"
    );
}
// Additional owner acceptance before any strictness claim: all numeric domains,
// swapped property order, exact sibling aggregate limits, label pre-copy sentinel,
// misleading size_hint custom SeqAccess, semantic duplicate payload/directory,
// checked u32 index conversion and current-only whole-root parser field rejection.
#[test]
fn primary_kind_is_string_only_and_integer_domains_remain_closed() {
    let room = rich_json("valid");
    assert!(decode(&room).is_ok());
    let mut passage = room.clone();
    passage["scopes"][0]["primary"][0]["membership"]["primary"]["kind"] = json!("passage");
    passage["scopes"][0]["primary"][0]["membership"]["doorway"] = Value::Null;
    passage["scopes"][0]["rooms"] = json!([]);
    assert!(decode(&passage).unwrap().validate_roots(&[0]).is_ok());
    for (base, kind) in [(&room, "room"), (&passage, "passage")] {
        let mut alternate = base.clone();
        let mut tagged = serde_json::Map::new();
        tagged.insert(kind.to_owned(), Value::Null);
        alternate["scopes"][0]["primary"][0]["membership"]["primary"]["kind"] =
            Value::Object(tagged);
        assert!(decode(&alternate).is_err());
    }
    for (path, value) in [
        ("/scopes/0/primary/0/membership/primary/id", json!(0)),
        (
            "/scopes/0/primary/0/membership/primary/id",
            json!(4294967296_u64),
        ),
        ("/scopes/0/primary/0/membership/primary/id", json!(true)),
        ("/scopes/0/primary/0/membership/doorway", json!(0)),
        ("/scopes/0/primary/0/cells/region", json!(-1)),
        ("/scopes/0/primary/0/cells/z", json!(1.5)),
        ("/scopes/0/primary/0/cells/first_x", json!("0")),
        (
            "/scopes/0/primary/0/membership/primary/kind",
            json!("unknown"),
        ),
    ] {
        let mut malformed = room.clone();
        *malformed.pointer_mut(path).unwrap() = value;
        assert!(decode(&malformed).is_err(), "{path}");
    }
}
#[test]
fn sequence_size_hint_never_allocates_and_exhausted_seed_never_reads_scalar() {
    struct Empty;
    impl<'de> serde::de::SeqAccess<'de> for Empty {
        type Error = serde::de::value::Error;
        fn next_element_seed<T: DeserializeSeed<'de>>(
            &mut self,
            _seed: T,
        ) -> Result<Option<T::Value>, Self::Error> {
            Ok(None)
        }
        fn size_hint(&self) -> Option<usize> {
            Some(usize::MAX)
        }
    }
    let mut meter = Meter { used: 0, limit: 0 };
    let result = ListSeed::<u32> {
        meter: &mut meter,
        maximum: None,
        marker: std::marker::PhantomData,
    }
    .deserialize(serde::de::value::SeqAccessDeserializer::new(Empty))
    .unwrap();
    assert!(result.is_empty());
    assert_eq!(result.capacity(), 0);
    struct Bomb;
    impl<'de> serde::Deserializer<'de> for Bomb {
        type Error = serde::de::value::Error;
        fn deserialize_any<V: serde::de::Visitor<'de>>(
            self,
            _visitor: V,
        ) -> Result<V::Value, Self::Error> {
            panic!("excess scalar read")
        }
        serde::forward_to_deserialize_any! {bool i8 i16 i32 i64 u8 u16 u32 u64 f32 f64 char str string bytes byte_buf option unit unit_struct newtype_struct seq tuple tuple_struct map struct enum identifier ignored_any}
    }
    let error = EntrySeed::<u32> {
        meter: &mut meter,
        marker: std::marker::PhantomData,
    }
    .deserialize(Bomb)
    .unwrap_err();
    assert!(error.to_string().contains("spatial charge exceeds limit"));
}

#[test]
fn deferred_actual_root_charge_uses_remaining_allowance_before_authority() {
    let value = source_json();
    // 30pool+47scope+6*13label+21*65members+3dir+11index+75mapping =1609.
    let body = 1609;
    for (ceiling, succeeds) in [(body + 21, true), (body + 20, false)] {
        let decoded = decode_with(
            &value,
            DecodeLimits {
                charge_bytes: ceiling,
                root_references: 0,
                directories: 1,
            },
        )
        .unwrap();
        assert_eq!(decoded.consumed_charge, body);
        let deferred = DeferredPool { decoded };
        let result = deferred.candidate().finalize([0_usize].into_iter());
        assert_eq!(result.is_ok(), succeeds);
        // A new private consumed handle may independently validate an immutable
        // pool in another RestoreContext; this is not a global one-shot token.
        assert_eq!(
            deferred.candidate().finalize([0_usize].into_iter()).is_ok(),
            succeeds
        );
    }
}

#[test]
fn codec_foreign_surface_references_share_without_aliasing_primary_ownership() {
    let mut value = rich_json("first");
    value["scopes"][0]["members"] = json!([1]);
    value["scopes"][0]["primary"][0]["cells"]["region"] = json!(1);
    value["scopes"][0]["rooms"][0]["runs"][0]["region"] = json!(2);
    let mut second = value["scopes"][0].clone();
    second["id"] = json!("second");
    second["members"] = json!([2]);
    second["primary"][0]["cells"]["region"] = json!(2);
    value["scopes"].as_array_mut().unwrap().push(second);
    value["directories"] = json!([[0, 1]]);
    let pool = decode(&value).unwrap().validate_roots(&[0]).unwrap();
    assert_eq!(serde_json::to_value(EncodedPool(&pool)).unwrap(), value);
    let mut escaped = value.clone();
    escaped["scopes"][0]["primary"][0]["cells"]["region"] = json!(2);
    assert!(decode(&escaped).is_err());
    let mut malformed = value.clone();
    malformed["scopes"][0]["rooms"][0]["runs"][0]["first_x"] = json!(0);
    assert!(decode(&malformed).is_err());
    let mut alias = value.clone();
    alias["scopes"][1]["members"] = json!([1]);
    alias["scopes"][1]["primary"][0]["cells"]["region"] = json!(1);
    assert_eq!(
        decode(&alias).unwrap().validate_roots(&[0]).unwrap_err(),
        PoolError::Structure
    );
    let mut undeclared = value;
    undeclared["scopes"][0]["dependencies"] = json!([2]);
    assert!(
        decode(&undeclared).is_err(),
        "no second mutable dependency directory"
    );
}
