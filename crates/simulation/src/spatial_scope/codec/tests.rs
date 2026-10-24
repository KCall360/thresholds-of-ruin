// Standalone codec tests; no Game/source-free Service claims.
use super::*;
use serde_json::{json, Value};

fn source_scope() -> Scope {
    // Typed structural >64-member witness, NOT a canonical authored package.
    Scope {
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
    json!({"scopes":[{"id":"large-members","members":(0..65).collect::<Vec<u64>>(),"primary":[],"rooms":[]}],"directories":[[0]]})
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
        serde_json::to_vec(&json!({"id":"","members":[],"primary":[],"rooms":[]}))
            .unwrap()
            .len(),
        46
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
