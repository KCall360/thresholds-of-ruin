//! Newly reconstructed content compilation checks; shared status runtime is reused.
use tor_simulation::content_ids::{AbilityId, SpeciesId};

#[test]
fn rogue_chill_is_a_purchased_checked_status_signature_with_no_fake_damage() {
    let (catalog, manifest) = super::rogue_roster::catalog();
    let definition =
        &catalog.content().catalog().species[&SpeciesId::new("rogue.ice_monster").unwrap()];
    let strike = &definition.melee.strikes()[0];
    assert!(strike.check.is_some());
    assert!(strike.on_success.is_empty());
    assert!(strike.on_failure.is_empty());
    let build = manifest.archetypes["rogue.ice_monster"]
        .creature
        .as_ref()
        .unwrap();
    let trained = catalog.prepare(build).unwrap().instantiate(42).unwrap();
    let ability = AbilityId::new("rogue.freezing_touch").unwrap();
    assert!(trained.derive().unwrap().abilities.contains(&ability));
    let mut untrained = build.clone();
    untrained.hit_dice[0].purchases.clear();
    let untrained = catalog
        .prepare(&untrained)
        .unwrap()
        .instantiate(42)
        .unwrap();
    assert!(!untrained.derive().unwrap().abilities.contains(&ability));
    let signature =
        serde_json::to_value(catalog.content().rules().ability(&ability).unwrap()).unwrap();
    assert_eq!(signature["strikes"]["kind"], "natural_melee");
    let rider = &signature["strikes"]["additional_riders"][0];
    assert_eq!(rider["trigger"], "success");
    assert_eq!(rider["effects"][0]["duration"], 200);
}
