use super::*;

fn fixture() -> (PropLibrary, Assets<Mesh>) {
    let mut materials = Assets::<StandardMaterial>::default();
    (
        PropLibrary::new(PropMaterials::new(&mut materials)),
        Assets::default(),
    )
}

fn triangle() -> Parts {
    let mut parts = Parts::default();
    parts.matte.tri([Vec3::ZERO, Vec3::X, Vec3::Y], Vec3::ONE);
    parts
}

#[test]
fn art_contract_idle_cache_is_bounded_without_evicting_visible_geometry() {
    let (mut library, mut meshes) = fixture();
    let visible = library.get("visible".into(), &mut meshes, triangle);
    for i in 0..1100 {
        drop(library.get(format!("idle-{i}"), &mut meshes, triangle));
    }
    assert!(
        // The insertion being returned is live during trimming. After its caller
        // drops it, periodic pruning accounts for that last newly idle model.
        library.cache.len() <= 1026,
        "idle recipe keys must not grow without bound"
    );
    let reused = library.get("visible".into(), &mut meshes, || {
        panic!("live geometry was evicted")
    });
    assert_eq!(visible.0[0].0.id(), reused.0[0].0.id());
}

#[test]
fn art_contract_empty_recipe_keys_also_obey_the_idle_budget() {
    let (mut library, mut meshes) = fixture();
    for i in 0..1100 {
        drop(library.get(format!("empty-{i}"), &mut meshes, Parts::default));
    }
    assert!(library.cache.len() <= 1024);
}

#[test]
fn art_contract_payload_budget_does_not_count_live_geometry_as_idle() {
    let (mut library, mut meshes) = fixture();
    let visible = library.get("visible".into(), &mut meshes, triangle);
    drop(library.get("old".into(), &mut meshes, triangle));
    // Accounting fixture avoids allocating a hundred megabytes to exercise eviction.
    library.cache.get_mut("old").unwrap().payload_bytes = WARM_PAYLOAD_LIMIT + 1;
    library.payload_bytes = library
        .cache
        .values()
        .map(|entry| entry.payload_bytes)
        .sum();
    let usage = library.trim();
    assert_eq!(usage.live_models, 1);
    assert_eq!(usage.warm_models, 0);
    assert!(meshes.contains(visible.0[0].0.id()));
    assert!(!library.cache.contains_key("old"));
}
