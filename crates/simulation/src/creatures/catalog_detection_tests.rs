use super::*;
use crate::appearance_catalog::{CreatureAppearance, PresentationCatalog};
fn content() -> Arc<Content> {
    let species = crate::test_creatures::species();
    Content::new(
        crate::test_creatures::ruleset(),
        ScenarioCatalog {
            species: [(species.id.clone(), species)].into(),
            templates: Default::default(),
        },
    )
    .unwrap()
}
#[test]
fn explicit_species_detection_labels_do_not_change_cosmetic_roster_or_get_lost_by_decoration() {
    let original = content();
    let id = original.catalog.species.keys().next().unwrap().clone();
    assert!(original.detection_appearance(&id).is_none());
    let label = CreatureAppearance {
        name: Arc::from("species label, not a personal name"),
        asset: Some(Arc::from("creature.bat")),
    };
    let decorated =
        Content::with_detection_appearances(&original, [(id.clone(), label.clone())].into())
            .unwrap();
    assert_eq!(
        decorated.presentation_catalog(),
        original.presentation_catalog()
    );
    let decorated = Content::with_trap_ammunition(&decorated, Default::default()).unwrap();
    let decorated = Content::with_presentation_catalog(
        &decorated,
        PresentationCatalog::new(vec![], vec![]).unwrap(),
    );
    assert_eq!(decorated.detection_appearance(&id), Some(&label));
    assert!(original.detection_appearance(&id).is_none());
}
#[test]
fn detection_species_labels_are_source_owned_and_share_strict_cosmetic_grammar() {
    let content = content();
    let known = content.catalog.species.keys().next().unwrap().clone();
    for (name, asset, valid) in [
        ("a".repeat(256), Some("creature.bat"), true),
        ("a".repeat(257), None, false),
        ("bad\nname".into(), None, false),
        ("bat".into(), Some("creature..bat"), false),
    ] {
        let result = Content::with_detection_appearances(
            &content,
            [(
                known.clone(),
                CreatureAppearance {
                    name: Arc::from(name),
                    asset: asset.map(Arc::from),
                },
            )]
            .into(),
        );
        assert_eq!(result.is_ok(), valid);
    }
    assert_eq!(
        Content::with_detection_appearances(
            &content,
            [(
                SpeciesId::new("unknown_detection_species").unwrap(),
                CreatureAppearance {
                    name: Arc::from("bat"),
                    asset: None
                }
            )]
            .into()
        ),
        Err(BuildError::InvalidDefinition)
    );
}
