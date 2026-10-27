use super::*;
use crate::scenario_package::{read_package, AppearancePool, ItemClass, Package};
use serde_json::json;
use std::path::Path;

fn equipment(staff: bool) -> EquipmentSpec {
    let amount = if staff {
        json!({"type":"rolled","count":2,"sides":3,"bonus":0})
    } else {
        json!({"type":"fixed","value":1})
    };
    let primary = if staff {
        json!({"category":"impact", "sides":3})
    } else {
        json!({"category":"impact"})
    };
    serde_json::from_value(json!({
        "slot":"weapon",
        "attack":{"skill":"heavy_weaponry","bonus":0,"wind_up":60,"recovery":40,
            "damage":{"primary":primary,
                "components":[{"category":"impact","amount":amount}]}},
        "modifiers":[]
    }))
    .unwrap()
}

fn fixture() -> Package {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/tests/magic-items");
    let source = read_package(&root).unwrap();
    let mut manifest = source.manifest.clone();
    manifest.appearance_pools.insert(
        "wands".into(),
        AppearancePool {
            appearances: vec!["copper wand".into(), "iron wand".into()],
            confounding: false,
            asset: Some("item.metal".into()),
        },
    );
    manifest.appearance_pools.insert(
        "staves".into(),
        AppearancePool {
            appearances: vec!["ash staff".into(), "oak staff".into()],
            confounding: false,
            asset: Some("item.wood".into()),
        },
    );
    let ember = manifest.archetypes.get_mut("ember").unwrap();
    ember.magical = true;
    ember.asset = Some("secret.embers".into());
    ember.forms = vec![
        ItemForm {
            name: "wand of embers".into(),
            appearance_pool: "wands".into(),
            equipment: equipment(false),
        },
        ItemForm {
            name: "staff of embers".into(),
            appearance_pool: "staves".into(),
            equipment: equipment(true),
        },
    ];
    let alias = ember.clone();
    manifest
        .archetypes
        .insert("ember_alias".into(), alias.clone());
    let mut other = alias;
    other.identity = Some("magic.other".into());
    other.name = Some("wand of another effect".into());
    other.forms[0].name = "wand of another effect".into();
    other.forms[1].name = "staff of another effect".into();
    manifest.archetypes.insert("other".into(), other);
    Package::from_parts(
        manifest,
        source.region_defs().unwrap(),
        &source.rules_source,
    )
    .unwrap()
}

fn resolve(package: &Package, key: &str, seed: u64) -> tor_simulation::ItemSpec {
    let index = package.index(seed).unwrap();
    index
        .definitions
        .population_item(key, 1, seed, &index.appearances)
        .unwrap()
}

#[test]
fn seeded_forms_bind_identity_aliases_instances_and_source_free_reopening() {
    let package = fixture();
    package.appearance_mapping(0).unwrap();
    let source_before = serde_json::to_value(&package.manifest).unwrap();
    let bytes = crate::storage::schema::scenario::encode_content_package(&package).unwrap();
    let restored = crate::storage::schema::scenario::decode_content_package(&bytes).unwrap();
    assert!(restored.directory.is_none());
    let mut names = BTreeSet::new();
    let mut appearances = BTreeSet::new();
    // Independent fixed vectors, not values calculated by the selector under test.
    let expected = [
        (false, "iron wand"),
        (false, "copper wand"),
        (false, "copper wand"),
        (true, "ash staff"),
        (true, "ash staff"),
        (false, "iron wand"),
        (true, "oak staff"),
        (false, "copper wand"),
    ];
    for seed in 0..8 {
        let item = resolve(&package, "ember", seed);
        let alias = resolve(&package, "ember_alias", seed);
        assert_eq!(item.name, alias.name);
        assert_eq!(item.appearance, alias.appearance);
        assert_eq!(item.equipment, alias.equipment);
        assert_eq!(
            item.device,
            package.manifest.archetypes["ember"]
                .device
                .as_ref()
                .map(|device| device.compile(&package.rules().unwrap()).unwrap())
        );
        assert_eq!(item, resolve(&restored, "ember", seed));
        assert_eq!(item, resolve(&package, "ember", seed));
        assert_eq!(item.class, tor_simulation::ItemClass::Wand);
        assert!(item.concealed && item.magical && !item.stackable);
        let staff = item.name == "staff of embers";
        assert_eq!((staff, item.appearance.as_str()), expected[seed as usize]);
        assert_eq!(
            item.equipment,
            Some(equipment(staff).compile(&package.rules().unwrap()).unwrap())
        );
        assert_eq!(
            item.asset.as_deref(),
            Some(if staff { "item.wood" } else { "item.metal" })
        );
        assert!(if staff {
            ["ash staff", "oak staff"].contains(&item.appearance.as_str())
        } else {
            ["copper wand", "iron wand"].contains(&item.appearance.as_str())
        });
        let other = resolve(&package, "other", seed);
        if item.asset == other.asset {
            assert_ne!(item.appearance, other.appearance);
        }
        names.insert(item.name);
        appearances.insert(item.appearance);
    }
    assert_eq!(
        names,
        BTreeSet::from(["wand of embers".to_owned(), "staff of embers".to_owned()])
    );
    assert_eq!(appearances.len(), 4);
    assert_eq!(
        serde_json::to_value(&package.manifest).unwrap(),
        source_before
    );
}

#[test]
fn alternate_pools_are_public_cosmetics_not_identity_names_or_assets() {
    use std::sync::Arc;
    use tor_simulation::appearance_catalog::{
        ObjectPresentation, ObjectPresentationKind, PresentationCatalog,
    };
    let mut manifest = fixture().manifest;
    manifest.creatures = Default::default();
    manifest
        .archetypes
        .retain(|key, _| key == "ember" || key == "ember_alias");
    let expected = [
        ("ash staff", "item.wood"),
        ("oak staff", "item.wood"),
        ("copper wand", "item.metal"),
        ("iron wand", "item.metal"),
    ]
    .into_iter()
    .map(|(name, asset)| ObjectPresentation {
        name: Arc::from(name),
        asset: Some(Arc::from(asset)),
        kind: ObjectPresentationKind::Item {
            class: tor_simulation::ItemClass::Wand,
        },
    })
    .collect();
    assert_eq!(
        super::super::presentation::compile(&manifest).unwrap(),
        PresentationCatalog::new(vec![], expected).unwrap()
    );
    let source = fixture();
    let palette = source.palette(std::iter::empty::<&String>());
    assert!(palette.contains("item.metal") && palette.contains("item.wood"));
    assert!(!palette.contains("secret.embers"));
}

#[test]
fn form_vocabulary_is_bounded_named_and_identity_policy_is_exact() {
    let package = fixture();
    let base = package.manifest.archetypes["ember"].clone();
    validate(&base).unwrap();
    for count in [1, 17] {
        let mut invalid = base.clone();
        invalid.forms = (0..count)
            .map(|index| ItemForm {
                name: format!("form {index}"),
                ..base.forms[0].clone()
            })
            .collect();
        assert!(validate(&invalid).is_err());
    }
    for field in ["name", "appearance_pool", "equipment"] {
        let mut missing = serde_json::to_value(&base.forms[0]).unwrap();
        missing.as_object_mut().unwrap().remove(field);
        assert!(
            serde_json::from_value::<ItemForm>(missing).is_err(),
            "{field}"
        );
    }
    let mut unknown = serde_json::to_value(&base.forms[0]).unwrap();
    unknown["forms"] = json!([]);
    assert!(serde_json::from_value::<ItemForm>(unknown).is_err());
    for field in ["name", "pool", "equipment", "order"] {
        let mut manifest = package.manifest.clone();
        let forms = &mut manifest.archetypes.get_mut("ember_alias").unwrap().forms;
        match field {
            "name" => forms[1].name = "different staff".into(),
            "pool" => forms[1].appearance_pool = "wands".into(),
            "equipment" => forms[1].equipment = equipment(false),
            "order" => forms.reverse(),
            _ => unreachable!(),
        }
        let invalid = Package::from_parts(
            manifest,
            package.region_defs().unwrap(),
            &package.rules_source,
        )
        .unwrap();
        assert!(
            invalid
                .check_item_identities()
                .unwrap_err()
                .message
                .contains("One identity"),
            "{field}"
        );
        let bytes = crate::storage::schema::scenario::encode_content_package(&invalid).unwrap();
        assert!(
            crate::storage::schema::scenario::decode_content_package(&bytes).is_err(),
            "{field}"
        );
    }
}

#[test]
fn unselected_bad_form_and_cross_form_pool_leaks_reject_for_every_seed() {
    let package = fixture();
    for mutation in [
        "missing_pool",
        "device_armor",
        "small_pool",
        "mixed_pool",
        "duplicate_name",
    ] {
        let mut manifest = package.manifest.clone();
        manifest.archetypes.remove("ember_alias");
        match mutation {
            "missing_pool" => {
                manifest.archetypes.get_mut("ember").unwrap().forms[1].appearance_pool =
                    "missing".into()
            }
            "device_armor" => {
                manifest.archetypes.get_mut("ember").unwrap().forms[1]
                    .equipment
                    .slot = "body_armor".into()
            }
            "small_pool" => {
                manifest
                    .appearance_pools
                    .get_mut("staves")
                    .unwrap()
                    .appearances
                    .truncate(1);
            }
            "mixed_pool" => {
                manifest.archetypes.get_mut("other").unwrap().class = ItemClass::Weapon;
            }
            "duplicate_name" => {
                let forms = &mut manifest.archetypes.get_mut("ember").unwrap().forms;
                forms[1].name = forms[0].name.clone();
            }
            _ => unreachable!(),
        }
        let invalid = Package::from_parts(
            manifest,
            package.region_defs().unwrap(),
            &package.rules_source,
        )
        .unwrap();
        for seed in [0, 1, 42] {
            assert!(
                invalid.appearance_mapping(seed).is_err(),
                "{mutation}, seed {seed}"
            );
        }
    }
}
