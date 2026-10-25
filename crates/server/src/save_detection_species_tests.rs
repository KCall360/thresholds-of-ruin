use super::{Borrowed, Owned};
#[test]
fn stored_detection_species_label_requires_nullable_asset_and_closed_named_shape() {
    let source = crate::creature_authoring::DetectionAppearance {
        name: "bat".into(),
        asset: None,
    };
    let encoded = serde_json::to_value(Borrowed(&source)).unwrap();
    assert_eq!(encoded, serde_json::json!({"name":"bat","asset":null}));
    let decoded: Owned<crate::creature_authoring::DetectionAppearance> =
        serde_json::from_value(encoded).unwrap();
    assert_eq!(decoded.0, source);
    for invalid in [
        r#"{"name":"bat"}"#,
        r#"{"name":"bat","asset":null,"actor":7}"#,
        r#"{"name":"bat","asset":null,"asset":null}"#,
        r#"["bat",null]"#,
    ] {
        assert!(
            serde_json::from_str::<Owned<crate::creature_authoring::DetectionAppearance>>(invalid)
                .is_err()
        );
    }
}
