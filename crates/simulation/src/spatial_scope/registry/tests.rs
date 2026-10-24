use super::*;
use crate::spatial_scope::{PassageId, PrimarySpace};
use tor_world::Position;

fn room(id: u32) -> RoomId {
    RoomId::new(id).unwrap()
}
fn member(id: u32) -> Membership {
    Membership::new(PrimarySpace::Room(room(id)), None).unwrap()
}
fn run(region: u64, z: i32, first_x: i32, last_x: i32) -> CellRun {
    CellRun {
        region: RegionId(region),
        z,
        y: 0,
        first_x,
        last_x,
    }
}
fn at(region: u64, x: i32, z: i32) -> Location {
    Location {
        region: RegionId(region),
        position: Position { x, y: 0, z },
    }
}
fn scope() -> Scope {
    Scope {
        id: "fixture/floor1".into(),
        members: vec![RegionId(1), RegionId(2)],
        primary: vec![run(1, 0, 0, 2), run(1, 1, 0, 2), run(2, 0, 0, 2)]
            .into_iter()
            .map(|cells| PrimaryRun {
                cells,
                membership: member(1),
            })
            .collect(),
        rooms: vec![RoomSurfaces {
            room: room(1),
            runs: vec![
                run(1, -1, 0, 2),
                run(1, 0, 0, 2),
                run(1, 1, 0, 2),
                run(2, 0, 0, 2),
            ],
        }],
    }
}
fn budget() -> Budget {
    Budget {
        scopes: 4,
        members: 8,
        rooms: 8,
        runs: 64,
        expanded_refs: 256,
        label_bytes: 128,
    }
}

#[test]
fn run_width_and_membership_use_true_z_and_wide_checked_arithmetic() {
    assert_eq!(
        run(1, 1, i32::MIN, i32::MAX).expanded_len(),
        Some(1_u64 << 32)
    );
    assert_eq!(run(1, 1, 2, 1).expanded_len(), None);
    assert!(run(1, 1, 0, 2).contains(at(1, 1, 1)));
    assert!(!run(1, 1, 0, 2).contains(at(1, 1, 0)));
    assert!(!run(1, 1, 0, 2).contains(at(2, 1, 1)));
}
#[test]
fn exact_reference_budget_counts_both_volume_and_surface_ownership() {
    let scope = scope();
    let exact = Budget {
        scopes: 1,
        members: 2,
        rooms: 1,
        runs: 7,
        expanded_refs: 21,
        label_bytes: scope.id.len(),
    };
    assert_eq!(
        scope.validate(&exact),
        Ok(Usage {
            scopes: 1,
            members: 2,
            rooms: 1,
            runs: 7,
            expanded_refs: 21,
            label_bytes: scope.id.len()
        })
    );
    for limit in [
        Budget { runs: 6, ..exact },
        Budget {
            expanded_refs: 20,
            ..exact
        },
        Budget {
            label_bytes: exact.label_bytes - 1,
            ..exact
        },
        Budget {
            members: 1,
            ..exact
        },
        Budget { rooms: 0, ..exact },
        Budget { scopes: 0, ..exact },
    ] {
        assert_eq!(scope.validate(&limit), Err(Error::Capacity));
    }
}
#[test]
fn primary_overlap_unmerged_and_dangling_room_are_rejected() {
    let base = scope();
    assert!(base.validate(&budget()).is_ok());
    let mut overlap = base.clone();
    overlap.primary.insert(
        1,
        PrimaryRun {
            cells: run(1, 0, 2, 3),
            membership: member(1),
        },
    );
    assert_eq!(overlap.validate(&budget()), Err(Error::Structure));
    let mut split = base.clone();
    split.primary[0].cells.last_x = 0;
    split.primary.insert(
        1,
        PrimaryRun {
            cells: run(1, 0, 1, 2),
            membership: member(1),
        },
    );
    assert_eq!(split.validate(&budget()), Err(Error::Structure));
    let mut dangling = base;
    dangling.primary[0].membership = member(2);
    assert_eq!(dangling.validate(&budget()), Err(Error::Structure));
}
#[test]
fn separate_room_surface_owners_may_share_cells_but_not_self_overlap() {
    let mut shared = scope();
    shared.primary.insert(
        1,
        PrimaryRun {
            cells: run(1, 0, 4, 6),
            membership: member(2),
        },
    );
    shared.rooms[0].runs[1].last_x = 3;
    shared.rooms.push(RoomSurfaces {
        room: room(2),
        runs: vec![run(1, 0, 3, 6)],
    });
    assert!(shared.validate(&budget()).is_ok());
    assert_eq!(shared.validate(&budget()).unwrap().expanded_refs, 29);
    let mut overlap = shared.clone();
    overlap.rooms[1].runs.push(run(1, 0, 6, 7));
    assert_eq!(overlap.validate(&budget()), Err(Error::Structure));
    let mut split = shared;
    split.rooms[1].runs = vec![run(1, 0, 3, 4), run(1, 0, 5, 6)];
    assert_eq!(split.validate(&budget()), Err(Error::Structure));
}
#[test]
fn doorway_membership_stays_distinct_and_lookup_is_scope_and_height_exact() {
    let mut source = scope();
    let passage = PassageId::new(1).unwrap();
    let door = Membership::new(PrimarySpace::Room(room(1)), Some(passage)).unwrap();
    let corridor = Membership::new(PrimarySpace::Passage(passage), None).unwrap();
    source.primary[0].cells.last_x = 1;
    source.primary.insert(
        1,
        PrimaryRun {
            cells: run(1, 0, 2, 2),
            membership: door,
        },
    );
    source.primary.insert(
        2,
        PrimaryRun {
            cells: run(1, 0, 3, 3),
            membership: corridor,
        },
    );
    assert!(source.validate(&budget()).is_ok());
    let mut registry = Registry::default();
    registry.register(source, &budget()).unwrap();
    assert_eq!(registry.lookup("fixture/floor1", at(1, 2, 0)), Some(door));
    assert_eq!(
        registry.lookup("fixture/floor1", at(1, 3, 0)),
        Some(corridor)
    );
    assert_eq!(
        registry.lookup("fixture/floor1", at(1, 2, 1)),
        Some(member(1))
    );
    assert_eq!(registry.lookup("fixture/floor2", at(1, 2, 0)), None);
    assert_eq!(registry.lookup("fixture/floor1", at(3, 2, 0)), None);
    assert_eq!(registry.lookup("fixture/floor1", at(1, 2, 2)), None);
}
#[test]
fn registration_is_idempotent_conflict_checked_and_failure_atomic() {
    let source = scope();
    assert!(source.validate(&budget()).is_ok());
    let mut registry = Registry::default();
    registry.register(source.clone(), &budget()).unwrap();
    assert_eq!(registry.lookup(&source.id, at(1, 0, 0)), Some(member(1)));
    let original = registry.clone();
    registry.register(source.clone(), &budget()).unwrap();
    assert_eq!(registry, original);
    let mut conflict = source.clone();
    conflict.primary[0].cells.last_x = 1;
    assert!(conflict.validate(&budget()).is_ok());
    assert_eq!(registry.register(conflict, &budget()), Err(Error::Conflict));
    assert_eq!(registry, original);
    let mut alias = source.clone();
    alias.id = "alias".into();
    assert_eq!(registry.register(alias, &budget()), Err(Error::Conflict));
    assert_eq!(registry, original);
    assert_eq!(
        registry.register(
            source,
            &Budget {
                runs: 6,
                ..budget()
            }
        ),
        Err(Error::Capacity)
    );
    assert_eq!(registry, original);
}
#[test]
fn disjoint_scopes_reuse_local_ids_but_share_one_aggregate_budget() {
    let first = scope();
    let mut second = first.clone();
    second.id = "fixture/floor2".into();
    for region in &mut second.members {
        region.0 += 2;
    }
    for run in &mut second.primary {
        run.cells.region.0 += 2;
    }
    for room in &mut second.rooms {
        for run in &mut room.runs {
            run.region.0 += 2;
        }
    }
    assert!(first.validate(&budget()).is_ok());
    assert!(second.validate(&budget()).is_ok());
    let mut registry = Registry::default();
    registry.register(first, &budget()).unwrap();
    assert_eq!(
        registry.lookup("fixture/floor1", at(1, 0, 0)),
        Some(member(1))
    );
    let before = registry.clone();
    assert_eq!(
        registry.register(
            second.clone(),
            &Budget {
                runs: 13,
                ..budget()
            }
        ),
        Err(Error::Capacity)
    );
    assert_eq!(registry, before);
    registry
        .register(
            second,
            &Budget {
                runs: 14,
                expanded_refs: 42,
                ..budget()
            },
        )
        .unwrap();
    assert_eq!(
        registry.lookup("fixture/floor2", at(3, 0, 0)),
        Some(member(1))
    );
    assert_eq!(registry.lookup("fixture/floor1", at(3, 0, 0)), None);
    assert_eq!(registry.lookup("fixture/floor2", at(1, 0, 0)), None);
}
