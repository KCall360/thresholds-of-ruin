//! Authoring and runtime agree on the one explicitly supported capability overlap.
use super::*;
use serde_json::json;

fn fixture() -> Package {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/tests/magic-items");
    let mut package = read_package(&path).unwrap();
    let item = package.manifest.archetypes.get_mut("ember").unwrap();
    item.equipment = Some(
        serde_json::from_value(json!({
            "slot": "weapon", "attack": {
                "skill": "heavy_weaponry", "bonus": 0, "wind_up": 60, "recovery": 40,
                "damage": {"primary": {"category": "impact", "sides": 3},
                    "components": [{"category": "impact", "amount": {
                        "type": "rolled", "count": 2, "sides": 3, "bonus": 0}}]}
            }, "modifiers": []
        }))
        .unwrap(),
    );
    package
}

#[test]
fn physical_device_authoring_accepts_singleton_weapon_and_rejects_other_combinations() {
    let package = fixture();
    assert!(package.appearance_mapping(42).is_ok());
    let item = &package.manifest.archetypes["ember"];
    assert!(item
        .equipment
        .as_ref()
        .unwrap()
        .compile(&package.rules().unwrap())
        .unwrap()
        .supports_device(tor_simulation::ItemClass::Wand));
    // Every mutation starts from the fully accepted source, not from another
    // invalid branch. No class or behavior fallback may admit hybrid armor,
    // consumables, projectile launchers, stacks or passive equipment effects.
    let source = serde_json::to_value(item).unwrap();
    for (field, value) in [
        ("stackable", json!(true)),
        ("class", json!("weapon")),
        ("class", json!("tool")),
        ("supply", json!("ration")),
        ("currency", json!("gold")),
        (
            "readable",
            json!({"identify": ["wand"], "discovery": "on_use"}),
        ),
        (
            "edible",
            json!({"nutrition": {"kind": "fixed", "amount": 10}}),
        ),
        (
            "projectile",
            json!({"family": "fixture.bow", "recovery": "recoverable",
            "damage": {"primary": {"category": "impact"}, "components": [
                {"category": "impact", "amount": {"type": "fixed", "value": 1}}]}}),
        ),
    ] {
        let mut changed = source.clone();
        changed[field] = value;
        let mut invalid = fixture();
        invalid
            .manifest
            .archetypes
            .insert("ember".into(), serde_json::from_value(changed).unwrap());
        assert!(
            invalid.appearance_mapping(42).is_err(),
            "forbidden capability: {field}"
        );
    }
    for (field, value) in [
        ("slot", json!("ring")),
        ("attack", json!(null)),
        (
            "armor",
            json!({"physical_reduction": 1, "material": "metal", "rustproof": false}),
        ),
        (
            "periodic_relocation",
            json!({"interval": 100, "chance_denominator": 2}),
        ),
        (
            "modifiers",
            json!([{"scope": "always", "op": {
            "kind": "add", "stat": {"kind": "health"}, "value": 1}}]),
        ),
    ] {
        let mut changed = source.clone();
        changed["equipment"][field] = value;
        let mut invalid = fixture();
        invalid
            .manifest
            .archetypes
            .insert("ember".into(), serde_json::from_value(changed).unwrap());
        assert!(
            invalid.appearance_mapping(42).is_err(),
            "forbidden equipment: {field}"
        );
    }
}

#[test]
fn physical_device_authored_curse_is_allowed_without_generated_weapon_enchantment() {
    let mut package = fixture();
    package
        .manifest
        .archetypes
        .get_mut("ember")
        .unwrap()
        .equipment_generation = Some(EquipmentGeneration::Cursed);
    assert!(package.appearance_mapping(42).is_ok());
    package
        .manifest
        .archetypes
        .get_mut("ember")
        .unwrap()
        .equipment_generation = Some(EquipmentGeneration::Weapon {
        curse_percent: 10,
        enchant_percent: 5,
        minimum: 1,
        maximum: 3,
    });
    assert!(package.appearance_mapping(42).is_err());
}
