use super::*;
use crate::appearance_catalog::{CreatureAppearance, PresentationCatalog};
use crate::content_ids::BodyFormId;
use crate::creatures::{BodyPackage, HdChoices, PlannedAdvancement};
use crate::progression::HdSource;
use crate::rules::{Effect, StrikeSource};

fn fixture(form: &str) -> (Arc<Content>, BodyFormId, Arc<BodyPackage>) {
    let species = crate::test_creatures::unmodified_species("body_catalog_species");
    let mut document = crate::test_creatures::ruleset().document().clone();
    let ability = document
        .abilities
        .iter_mut()
        .find(|ability| ability.id.as_str() == "spark_lance_fire")
        .unwrap();
    let StrikeSource::Authored { strikes } = &mut ability.strikes else {
        panic!("authored fixture");
    };
    strikes[0].on_success = vec![Effect::BalefulPolymorph {
        forms: vec![BodyFormId::new(form).unwrap()],
    }];
    let content = Content::new(
        Ruleset::validate(document).unwrap(),
        ScenarioCatalog {
            species: [(species.id.clone(), species.clone())].into(),
            templates: Default::default(),
        },
    )
    .unwrap();
    let id = BodyFormId::new("supported_body").unwrap();
    let package = Arc::new(BodyPackage {
        id: id.clone(),
        species: species.id,
        body: crate::BodySpec::default(),
        advancement: vec![PlannedAdvancement {
            source: HdSource::Racial,
            choices: HdChoices::default(),
        }],
    });
    (content, id, package)
}

#[test]
fn authored_roster_is_validated_after_packages_and_explicit_eligibility_are_installed() {
    let (original, id, package) = fixture("supported_body");
    assert!(original.body_package(&id).is_none());
    let packages =
        Content::with_body_packages(&original, [(id.clone(), package.clone())].into()).unwrap();
    assert_eq!(packages.body_package(&id), Some(&package));
    assert!(packages.polymorph_body_package(&id).is_none());
    assert_eq!(
        Content::with_polymorph_forms(&packages, BTreeSet::new()),
        Err(BuildError::InvalidDefinition)
    );
    let valid = Content::with_polymorph_forms(&packages, [id.clone()].into()).unwrap();
    assert_eq!(valid.polymorph_body_package(&id), Some(&package));
    assert!(
        Content::with_polymorph_forms(&packages, [BodyFormId::new("unknown").unwrap()].into())
            .is_err()
    );
    let (unknown, id, package) = fixture("missing_roster_form");
    let unknown = Content::with_body_packages(&unknown, [(id.clone(), package)].into()).unwrap();
    assert_eq!(
        Content::with_polymorph_forms(&unknown, [id].into()),
        Err(BuildError::InvalidDefinition)
    );
    assert!(
        original
            .body_package(&BodyFormId::new("supported_body").unwrap())
            .is_none(),
        "failed/finalized derivatives do not mutate source"
    );
}

#[test]
fn catalog_decorations_preserve_body_identity_and_explicit_eligibility() {
    let (original, id, package) = fixture("supported_body");
    let packages =
        Content::with_body_packages(&original, [(id.clone(), package.clone())].into()).unwrap();
    let enabled = Content::with_polymorph_forms(&packages, [id.clone()].into()).unwrap();
    let appearance = CreatureAppearance {
        name: Arc::from("ordinary creature"),
        asset: None,
    };
    let detected = Content::with_detection_appearances(
        &enabled,
        [(package.species.clone(), appearance.clone())].into(),
    )
    .unwrap();
    let presented = Content::with_presentation_catalog(
        &detected,
        PresentationCatalog::new(vec![], vec![]).unwrap(),
    );
    let ammunition = Content::with_trap_ammunition(&presented, BTreeMap::new()).unwrap();
    for decorated in [&detected, &presented, &ammunition] {
        assert!(Arc::ptr_eq(decorated.body_package(&id).unwrap(), &package));
        assert!(Arc::ptr_eq(
            decorated.polymorph_body_package(&id).unwrap(),
            &package
        ));
        assert_eq!(decorated.body_packages().count(), 1);
        assert_eq!(
            decorated.detection_appearance(&package.species),
            Some(&appearance)
        );
    }
    assert!(packages.polymorph_body_package(&id).is_none());
    assert!(
        Content::with_body_packages(&enabled, BTreeMap::new()).is_err(),
        "enabled form may not disappear through catalog replacement"
    );
}

#[test]
fn canonical_nonliving_and_incorporeal_facts_cannot_be_declared_eligible() {
    let (ordinary, id, package) = fixture("supported_body");
    let ordinary =
        Content::with_body_packages(&ordinary, [(id.clone(), package.clone())].into()).unwrap();
    let ordinary = Content::with_polymorph_forms(&ordinary, [id.clone()].into()).unwrap();
    assert!(ordinary.polymorph_body_package(&id).is_some());

    for (kind, incorporeal) in [("undead", false), ("construct", false), ("humanoid", true)] {
        let mut catalog = ordinary.catalog().clone();
        let mut excluded = catalog.species[&package.species].clone();
        excluded.id = crate::content_ids::SpeciesId::new("excluded_body_species").unwrap();
        excluded.kind = crate::content_ids::TypeId::new(kind).unwrap();
        if incorporeal {
            excluded
                .subtypes
                .insert(crate::content_ids::SubtypeId::new("incorporeal").unwrap());
        }
        catalog
            .species
            .insert(excluded.id.clone(), excluded.clone());
        let content = Content::new(ordinary.rules().clone(), catalog).unwrap();
        let mut forbidden = (*package).clone();
        forbidden.species = excluded.id;
        let forbidden = Arc::new(forbidden);
        // Original bodies may be nonliving: registering them is valid, but
        // asserting destination eligibility must fail eagerly.
        let registered =
            Content::with_body_packages(&content, [(id.clone(), forbidden.clone())].into())
                .unwrap();
        assert!(registered.body_package(&id).is_some());
        assert_eq!(
            Content::with_polymorph_forms(&registered, [id.clone()].into()),
            Err(BuildError::InvalidDefinition),
            "{kind}, incorporeal={incorporeal}"
        );
        // Replacing an already-eligible package cannot bypass the same check.
        let allowed =
            Content::with_body_packages(&content, [(id.clone(), package.clone())].into()).unwrap();
        let allowed = Content::with_polymorph_forms(&allowed, [id.clone()].into()).unwrap();
        assert_eq!(
            Content::with_body_packages(&allowed, [(id.clone(), forbidden)].into()),
            Err(BuildError::InvalidDefinition)
        );
        assert_eq!(allowed.polymorph_body_package(&id), Some(&package));
    }
}
