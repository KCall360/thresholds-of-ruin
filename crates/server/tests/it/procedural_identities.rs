//! Stable procedural identity intervals are reserved before any terrain exists.
use crate::support;
use std::collections::BTreeMap;
use tor_server::procedural_world::{Domain, FamilyKind, WorldFamily};
use tor_server::scenario_package::{Package, RegionDef};

fn family(base_region: u64, domain: Domain) -> WorldFamily {
    WorldFamily {
        kind: FamilyKind::GridChain,
        version: 1,
        base_region,
        domain,
        grid: [2, 3],
        size: [26, 7, 2],
        recipe: "exploration".into(),
        zone: None,
    }
}

fn package(families: BTreeMap<String, WorldFamily>, authored: &[u64]) -> Package {
    let template = support::load("explicit-grid-groups", 42);
    let mut manifest = template.package.as_ref().unwrap().manifest.clone();
    manifest.generation_groups.clear();
    manifest.stair_pairs.clear();
    manifest.characters[0].anchor = format!("{}/start", families["caves"].base_region);
    manifest.procedural_worlds = families;
    let regions: Vec<RegionDef> = authored
        .iter()
        .map(|id| {
            toml::from_str(&format!(
                "id = {id}\nname = 'Authored room'\nsize = [5, 5, 2]\nchamber = true\n"
            ))
            .unwrap()
        })
        .collect();
    let rules =
        std::fs::read_to_string(support::package("explicit-grid-groups").join("rules.toml"))
            .unwrap();
    // Rebuild the certificate together with every manifest change; mutating an
    // assembled package would only exercise the unrelated integrity guard.
    Package::from_parts(manifest, regions, &rules).unwrap()
}

#[test]
fn adjacent_family_intervals_are_valid_but_shared_boundary_identities_are_rejected() {
    let first = family(100, Domain::Finite { count: 2 });
    assert_eq!(first.last_region(), Some(111));
    for start in [88, 112] {
        let second = family(start, Domain::Finite { count: 2 });
        package(
            BTreeMap::from([("caves".into(), first.clone()), ("neighbor".into(), second)]),
            &[],
        )
        .check()
        .unwrap();
    }
    for start in [89, 100, 111] {
        let second = family(start, Domain::Finite { count: 2 });
        let error = package(
            BTreeMap::from([("caves".into(), first.clone()), ("neighbor".into(), second)]),
            &[],
        )
        .check()
        .unwrap_err();
        assert!(
            error.message.contains("identity ranges overlap"),
            "{start}: {error}"
        );
    }
}

#[test]
fn authored_ids_may_border_a_family_but_cannot_occupy_any_reserved_member() {
    let families = BTreeMap::from([("caves".into(), family(100, Domain::Finite { count: 2 }))]);
    package(families.clone(), &[99, 112]).check().unwrap();
    for identity in [100, 105, 111] {
        let error = package(families.clone(), &[identity]).check().unwrap_err();
        assert!(
            error
                .message
                .contains("identity range overlaps authored region"),
            "{identity}: {error}"
        );
    }
}

#[test]
fn seeded_finite_reservations_include_tail_depths_absent_from_a_particular_seed() {
    let mut seeded = family(100, Domain::SeededFinite { min: 1, max: 8 });
    seeded.grid = [1, 1];
    let tail = seeded.last_region().unwrap();
    assert_eq!(tail, 107);
    let shorter_seed = (0..64)
        .find(|seed| seeded.depth_limit("caves", *seed) < 8)
        .expect("the seed-dependent domain has a shorter instance");
    assert!(!seeded.materializable("caves", shorter_seed, tail));
    assert!(seeded
        .describe("caves", shorter_seed, tail)
        .unwrap()
        .is_none());
    let families = BTreeMap::from([("caves".into(), seeded.clone())]);
    let authored = package(families, &[tail]).check().unwrap_err();
    assert!(authored
        .message
        .contains("identity range overlaps authored region"));
    let neighbor = family(tail, Domain::Finite { count: 1 });
    let overlapping = package(
        BTreeMap::from([("caves".into(), seeded), ("neighbor".into(), neighbor)]),
        &[],
    )
    .check()
    .unwrap_err();
    assert!(overlapping.message.contains("identity ranges overlap"));
}

#[test]
fn open_reservations_extend_beyond_the_operational_budget_to_the_last_logical_depth() {
    let mut open = family(100, Domain::Open { max_depth: 2 });
    open.grid = [1, 1];
    let last = 100 + u64::from(u32::MAX) - 1;
    assert_eq!(open.last_region(), Some(last));
    assert_eq!(open.address(last), Some((u32::MAX, 0)));
    assert!(!open.materializable("caves", 42, 102));
    for identity in [102, last] {
        let error = package(
            BTreeMap::from([("caves".into(), open.clone())]),
            &[identity],
        )
        .check()
        .unwrap_err();
        assert!(error
            .message
            .contains("identity range overlaps authored region"));
        let error = package(
            BTreeMap::from([
                ("caves".into(), open.clone()),
                (
                    "neighbor".into(),
                    family(identity, Domain::Finite { count: 1 }),
                ),
            ]),
            &[],
        )
        .check()
        .unwrap_err();
        assert!(error.message.contains("identity ranges overlap"));
    }
    package(
        BTreeMap::from([
            ("caves".into(), open),
            (
                "neighbor".into(),
                family(last + 1, Domain::Finite { count: 1 }),
            ),
        ]),
        &[],
    )
    .check()
    .unwrap();
}

#[test]
fn largest_safe_interval_keeps_exclusive_group_ends_representable() {
    let valid = family(u64::MAX - 12, Domain::Finite { count: 2 });
    valid.check().unwrap();
    assert_eq!(valid.last_region(), Some(u64::MAX - 1));
    assert_eq!(valid.address(u64::MAX - 1), Some((2, 5)));
    assert_eq!(valid.address(u64::MAX), None);
    let group = valid.group("caves", u64::MAX, 2).unwrap();
    assert_eq!(group.members.len(), 6);
    assert_eq!(group.members.last(), Some(&(u64::MAX - 1)));
    assert!(valid
        .describe("caves", u64::MAX, u64::MAX - 1)
        .unwrap()
        .is_some());
    for base in [u64::MAX - 11, u64::MAX - 10, u64::MAX] {
        assert!(family(base, Domain::Finite { count: 2 }).check().is_err());
    }
    let reserved = 6 * u64::from(u32::MAX);
    let valid_open = family(u64::MAX - reserved, Domain::Open { max_depth: 1 });
    valid_open.check().unwrap();
    assert_eq!(valid_open.last_region(), Some(u64::MAX - 1));
    assert!(
        family(u64::MAX - reserved + 1, Domain::Open { max_depth: 1 })
            .check()
            .is_err()
    );
}
