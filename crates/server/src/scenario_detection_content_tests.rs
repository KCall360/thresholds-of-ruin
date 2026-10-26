//! Source metadata and retained-source construction, never actor-name inference.
use crate::scenario_package::{resolve_source, Package};
use serde_json::{json, Value};
use tor_simulation::content_ids::SpeciesId;
fn package() -> Package {
    let root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/rogue-exploration");
    let source = resolve_source(&root).unwrap();
    let rules = std::fs::read_to_string(root.join(&source.manifest.ruleset)).unwrap();
    let package = Package::from_parts(source.manifest, vec![], &rules).unwrap();
    package.check().unwrap();
    package
}
#[test]
fn normal_and_source_free_content_retain_every_explicit_species_detection_tuple() {
    let package = package();
    let normal = super::super::compiler::PreparedDefinitions::new_with_source(
        &package.manifest,
        1,
        package.rules().unwrap(),
        || None,
    )
    .unwrap();
    let bytes = crate::storage::schema::scenario::encode_content_package(&package).unwrap();
    let retained = crate::storage::schema::scenario::decode_content_package(&bytes).unwrap();
    assert!(retained.directory.is_none());
    let content = retained.content().unwrap();
    assert_eq!(package.manifest.creatures.species.len(), 27);
    for (id, species) in &package.manifest.creatures.species {
        let expected = species
            .detection_appearance
            .as_ref()
            .expect("production source explicit");
        let id = SpeciesId::new(id).unwrap();
        let normal = normal
            .creature_catalog()
            .content()
            .detection_appearance(&id)
            .unwrap();
        let restored = content.detection_appearance(&id).unwrap();
        assert_eq!(
            (normal.name.as_ref(), normal.asset.as_deref()),
            (expected.name.as_str(), expected.asset.as_deref())
        );
        assert_eq!(normal, restored);
    }
    assert_eq!(normal.creature_catalog().content(), &content);
}
#[test]
fn personal_name_changes_do_not_replace_species_detection_authority() {
    let original = package();
    let mut manifest = original.manifest.clone();
    manifest.characters[0].creature.as_mut().unwrap().name = "PRIVATE current owner".into();
    manifest
        .archetypes
        .get_mut("rogue.emu")
        .unwrap()
        .creature
        .as_mut()
        .unwrap()
        .name = "PRIVATE individual emu".into();
    let edited = Package::from_parts(manifest, vec![], &original.rules_source).unwrap();
    edited.check().unwrap();
    let content = edited.content().unwrap();
    for (id, name, asset) in [
        ("rogue.human", "human", "creature.humanoid.human"),
        ("rogue.emu", "emu", "creature.bird.emu"),
    ] {
        let label = content
            .detection_appearance(&SpeciesId::new(id).unwrap())
            .unwrap();
        assert_eq!(
            (label.name.as_ref(), label.asset.as_deref()),
            (name, Some(asset))
        );
    }
}
#[test]
fn saved_species_metadata_is_required_nullable_and_nested_tuple_is_closed() {
    let package = package();
    let bytes = crate::storage::schema::scenario::encode_content_package(&package).unwrap();
    crate::storage::schema::scenario::decode_content_package(&bytes).unwrap();
    let value: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        value["package"]["manifest"]["creatures"]["species"]["rogue.human"]["detection_appearance"],
        json!({"name":"human","asset":"creature.humanoid.human"})
    );
    for mutation in 0..5 {
        let mut invalid = value.clone();
        let species = &mut invalid["package"]["manifest"]["creatures"]["species"]["rogue.human"];
        match mutation {
            0 => {
                species
                    .as_object_mut()
                    .unwrap()
                    .remove("detection_appearance");
            }
            1 => {
                species["detection_appearance"]
                    .as_object_mut()
                    .unwrap()
                    .remove("name");
            }
            2 => {
                species["detection_appearance"]
                    .as_object_mut()
                    .unwrap()
                    .remove("asset");
            }
            3 => {
                species["detection_appearance"]["private_identity"] = json!("no");
            }
            _ => species["detection_appearance"] = json!(["human", "creature.humanoid.human"]),
        }
        assert!(
            crate::storage::schema::scenario::decode_content_package(
                &serde_json::to_vec(&invalid).unwrap()
            )
            .is_err(),
            "mutation {mutation}"
        );
    }
    // Author omission remains lawful; the saved mirror must still write null.
    let mut manifest = package.manifest.clone();
    manifest
        .creatures
        .species
        .get_mut("rogue.human")
        .unwrap()
        .detection_appearance = None;
    let unnamed = Package::from_parts(manifest, vec![], &package.rules_source).unwrap();
    unnamed.check().unwrap();
    let bytes = crate::storage::schema::scenario::encode_content_package(&unnamed).unwrap();
    let saved: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        saved["package"]["manifest"]["creatures"]["species"]["rogue.human"]["detection_appearance"],
        Value::Null
    );
    let retained = crate::storage::schema::scenario::decode_content_package(&bytes).unwrap();
    assert!(retained
        .content()
        .unwrap()
        .detection_appearance(&SpeciesId::new("rogue.human").unwrap())
        .is_none());
}
