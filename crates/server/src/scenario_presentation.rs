//! One retained-source adapter for normal compilation and source-free restore.
//! Cosmetic rosters never instantiate subjects or consume simulation randomness.
use super::{fail, require, Archetype, Failure, ItemClass, Manifest};
use std::mem::size_of;
use std::sync::Arc;
use tor_simulation::appearance_catalog::{
    CreatureAppearance, ObjectPresentation, ObjectPresentationKind, PresentationCatalog,
};

type UsedPool<'a> = (&'a str, &'a super::AppearancePool, ItemClass);

#[derive(Debug)]
struct Layout<'a> {
    pools: Vec<UsedPool<'a>>,
    object_rows: usize,
    creature_rows: usize,
    // An allocation/work proof, not a new arbitrary source admission limit.
    text_bytes: usize,
    allocation_bytes: usize,
}

fn add(left: usize, right: usize) -> Result<usize, Failure> {
    left.checked_add(right)
        .ok_or_else(|| fail("Presentation catalog size overflow"))
}
fn bytes(count: usize, width: usize) -> Result<usize, Failure> {
    count
        .checked_mul(width)
        .ok_or_else(|| fail("Presentation catalog allocation overflow"))
}
fn text_size(name: &str, asset: Option<&str>) -> Result<usize, Failure> {
    add(name.len(), asset.map_or(0, str::len))
}
fn plain_item(author: &Archetype) -> Option<&str> {
    (author.creature.is_none() && author.appearance_pool.is_none())
        .then_some(author.name.as_deref())
        .flatten()
}

fn preflight(manifest: &Manifest) -> Result<Layout<'_>, Failure> {
    // Charge the complete borrowed directory before allocating it. Duplicate
    // archetype aliases never multiply a pool's authored labels or shared asset.
    let references = manifest
        .archetypes
        .values()
        .filter(|author| author.creature.is_none() && author.appearance_pool.is_some())
        .try_fold(0usize, |count, _| add(count, 1))?;
    let directory_bytes = bytes(references, size_of::<UsedPool<'_>>())?;
    let mut pools = Vec::new();
    pools
        .try_reserve_exact(references)
        .map_err(|_| fail("Presentation pool directory allocation failed"))?;
    for author in manifest
        .archetypes
        .values()
        .filter(|author| author.creature.is_none())
    {
        if let Some(key) = &author.appearance_pool {
            let pool = manifest
                .appearance_pools
                .get(key)
                .ok_or_else(|| fail("Unknown presentation appearance pool"))?;
            pools.push((key.as_str(), pool, author.class));
        }
    }
    // In-place unstable sort has no second full-row scratch allocation.
    pools.sort_unstable_by_key(|(key, _, _)| *key);
    for pair in pools.windows(2) {
        require(
            pair[0].0 != pair[1].0 || pair[0].2 == pair[1].2,
            "Appearance pool must have one physical class",
        )?;
    }
    pools.dedup_by_key(|(key, _, _)| *key);
    let mut object_rows = 0usize;
    let mut creature_rows = 0usize;
    let mut text_bytes = 0usize;
    for author in manifest.archetypes.values() {
        if let Some(creature) = &author.creature {
            creature_rows = add(creature_rows, 1)?;
            text_bytes = add(
                text_bytes,
                text_size(&creature.name, author.asset.as_deref())?,
            )?;
        } else if let Some(name) = plain_item(author) {
            object_rows = add(object_rows, 1)?;
            text_bytes = add(text_bytes, text_size(name, author.asset.as_deref())?)?;
        }
    }
    for (_, pool, _) in &pools {
        object_rows = add(object_rows, pool.appearances.len())?;
        text_bytes = add(text_bytes, pool.asset.as_deref().map_or(0, str::len))?;
        for name in &pool.appearances {
            text_bytes = add(text_bytes, name.len())?;
        }
    }
    // Core adds one neutral Misc if there are no item rows, and always Stairs.
    // Neutral rows deliberately contain empty text and no identifying asset.
    object_rows = add(object_rows.max(1), 1)?;
    creature_rows = creature_rows.max(1);
    let row_bytes = add(
        bytes(object_rows, size_of::<ObjectPresentation>())?,
        bytes(creature_rows, size_of::<CreatureAppearance>())?,
    )?;
    let allocation_bytes = add(add(directory_bytes, row_bytes)?, text_bytes)?;
    Ok(Layout {
        pools,
        object_rows,
        creature_rows,
        text_bytes,
        allocation_bytes,
    })
}

pub(super) fn compile(manifest: &Manifest) -> Result<Arc<PresentationCatalog>, Failure> {
    let layout = preflight(manifest)?;
    // Explicit proof is retained locally; ordinary Arc/string allocations remain
    // infallible Rust allocations. This does not promise an OOM recovery layer.
    let _accounted = (layout.text_bytes, layout.allocation_bytes);
    let mut creatures = Vec::new();
    let mut objects = Vec::new();
    creatures
        .try_reserve_exact(layout.creature_rows)
        .map_err(|_| fail("Presentation creature allocation failed"))?;
    objects
        .try_reserve_exact(layout.object_rows)
        .map_err(|_| fail("Presentation object allocation failed"))?;
    for author in manifest.archetypes.values() {
        if let Some(creature) = &author.creature {
            creatures.push(CreatureAppearance {
                name: Arc::from(creature.name.as_str()),
                asset: author.asset.as_deref().map(Arc::from),
            });
        } else if let Some(name) = plain_item(author) {
            objects.push(ObjectPresentation {
                name: Arc::from(name),
                asset: author.asset.as_deref().map(Arc::from),
                kind: ObjectPresentationKind::Item {
                    class: author.class.into(),
                },
            });
        }
    }
    for (_, pool, class) in layout.pools {
        let asset: Option<Arc<str>> = pool.asset.as_deref().map(Arc::from);
        for name in &pool.appearances {
            objects.push(ObjectPresentation {
                name: Arc::from(name.as_str()),
                asset: asset.clone(),
                kind: ObjectPresentationKind::Item {
                    class: class.into(),
                },
            });
        }
    }
    PresentationCatalog::new(creatures, objects)
        .map_err(|error| fail(format!("Invalid presentation catalog: {error:?}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scenario_package::{read_package, AppearancePool, Package};
    fn manifest() -> Manifest {
        let mut value: Manifest =
            toml::from_str(include_str!("../../../scenarios/two-room/scenario.toml")).unwrap();
        value.archetypes.clear();
        value.appearance_pools.clear();
        value
    }
    fn pooled(pool: &str) -> Archetype {
        Archetype {
            name: Some("SECRET IDENTITY MUST NOT ENTER PALETTE".into()),
            appearance_pool: Some(pool.into()),
            class: ItemClass::Potion,
            ..Default::default()
        }
    }
    fn checked_package(source: Manifest) -> Package {
        let root =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two-room");
        let template = read_package(&root).unwrap();
        let mut regions = template.region_defs().unwrap();
        for region in &mut regions {
            region.items.clear();
        }
        let package = Package::from_parts(source, regions, &template.rules_source).unwrap();
        package.check().unwrap();
        package
    }
    #[test]
    fn empty_palette_charges_both_neutral_object_and_stairs() {
        let package = checked_package(manifest());
        let source = &package.manifest;
        let plan = preflight(source).unwrap();
        assert_eq!(plan.object_rows, 2);
        assert_eq!(plan.creature_rows, 1);
        assert_eq!(plan.text_bytes, 0);
        assert_eq!(
            plan.allocation_bytes,
            2 * size_of::<ObjectPresentation>() + size_of::<CreatureAppearance>()
        );
        assert_eq!(
            compile(source).unwrap(),
            PresentationCatalog::new(vec![], vec![]).unwrap()
        );
        assert!(add(usize::MAX, 1).is_err());
        assert!(bytes(usize::MAX, 2).is_err());
    }
    #[test]
    fn independent_large_pools_are_not_rejected_at_generic_selector_limit() {
        let mut source = manifest();
        for prefix in ["amber", "violet"] {
            source.appearance_pools.insert(
                prefix.into(),
                AppearancePool {
                    appearances: (0..4096).map(|index| format!("{prefix} {index}")).collect(),
                    asset: Some("item.potion".into()),
                    confounding: false,
                },
            );
            source.archetypes.insert(prefix.into(), pooled(prefix));
        }
        let package = checked_package(source.clone());
        let plan = preflight(&package.manifest).unwrap();
        assert_eq!(plan.object_rows, 8193);
        assert_eq!(plan.pools.len(), 2);
        let before = compile(&source).unwrap();
        source
            .archetypes
            .insert("duplicate alias".into(), pooled("amber"));
        let aliased = preflight(&source).unwrap();
        assert_eq!(aliased.object_rows, plan.object_rows);
        assert_eq!(aliased.text_bytes, plan.text_bytes);
        assert_eq!(compile(&source).unwrap(), before);
        checked_package(source.clone());
        let mut invalid = source;
        invalid.archetypes.get_mut("duplicate alias").unwrap().class = ItemClass::Weapon;
        // This is a new authored negative, not corruption of pinned metadata.
        // Reconstruct its source receipt so the existing physical-class rule,
        // rather than an earlier integrity guard, owns the rejection.
        let negative = Package::from_parts(
            invalid,
            package.region_defs().unwrap(),
            &package.rules_source,
        )
        .unwrap();
        let failure = negative.check().unwrap_err();
        assert!(failure.message.contains("one physical class"), "{failure}");
    }
    #[test]
    fn full_tuple_dedup_and_pool_aliases_are_canonical_not_identity_based() {
        let mut source = manifest();
        for key in ["first", "second"] {
            source.archetypes.insert(
                key.into(),
                Archetype {
                    name: Some("token".into()),
                    class: ItemClass::Gem,
                    asset: Some("item.gem".into()),
                    ..Default::default()
                },
            );
        }
        let double = compile(&source).unwrap();
        source.archetypes.remove("first");
        assert_eq!(compile(&source).unwrap(), double);
        let expected = PresentationCatalog::new(
            vec![],
            vec![ObjectPresentation {
                name: Arc::from("token"),
                asset: Some(Arc::from("item.gem")),
                kind: ObjectPresentationKind::Item {
                    class: tor_simulation::ItemClass::Gem,
                },
            }],
        )
        .unwrap();
        assert_eq!(double, expected);
    }
    #[test]
    fn validated_creature_display_names_keep_full_utf8_bound_in_cosmetic_catalog() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../scenarios/rogue-exploration");
        let original = read_package(&root).unwrap();
        original.check().unwrap();
        let long_name = "é".repeat(128);
        let mut authored = original.manifest.clone();
        let creature = authored.archetypes.get_mut("rogue.emu").unwrap();
        creature.creature.as_mut().unwrap().name = long_name.clone();
        // Repin the legitimately edited authored fixture through its normal
        // constructor; never weaken the production content-integrity check.
        let package = Package::from_parts(
            authored,
            original.region_defs().unwrap(),
            &original.rules_source,
        )
        .unwrap();
        package.check().unwrap();
        let mut source = manifest();
        source.archetypes.insert(
            "long-creature".into(),
            package.manifest.archetypes["rogue.emu"].clone(),
        );
        assert_eq!(
            preflight(&source).unwrap().text_bytes,
            256 + "creature.bird.emu".len()
        );
        assert_eq!(
            compile(&source).unwrap(),
            PresentationCatalog::new(
                vec![CreatureAppearance {
                    name: Arc::from(long_name),
                    asset: Some(Arc::from("creature.bird.emu")),
                }],
                vec![]
            )
            .unwrap()
        );
    }

    #[test]
    fn normal_and_retained_source_free_content_share_exact_catalog() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../scenarios/rogue-exploration");
        let package = read_package(&root).unwrap();
        let normal = super::super::compiler::PreparedDefinitions::new_with_source(
            &package.manifest,
            1,
            package.rules().unwrap(),
            || None,
        )
        .unwrap();
        let retained = Package::from_parts(
            package.manifest.clone(),
            package.region_defs().unwrap(),
            &package.rules_source,
        )
        .unwrap();
        assert_eq!(
            normal.creature_catalog().content().presentation_catalog(),
            retained.content().unwrap().presentation_catalog()
        );
        assert_eq!(
            compile(&package.manifest).unwrap(),
            *retained.content().unwrap().presentation_catalog()
        );
    }
}

#[cfg(test)]
#[path = "scenario_detection_content_tests.rs"]
mod detection_content_tests;
