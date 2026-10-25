// STAGED ONLY: install as spatial_scope::mapping::tests after the API-only red
// scaffold compiles. Missing symbols are preflight failures, not first-red proof.
// API frozen with the source owner on 2026-10-25; no execution is claimed here.
use crate::spatial_scope::{
    mapping::{
        self, CertificateBudget, FaceBudget, MappingCertification, MappingRoomFrame,
        MappingUnavailable,
    },
    registry::{Budget, CellRun, PrimaryRun, RoomSurfaces, Scope},
    Membership, PassageId, PrimarySpace, RoomId,
};
use std::collections::{BTreeMap, BTreeSet};
use tor_world::{Direction, Extent, Location, Passage, Position, Region, RegionId, World};

const SIX: [Direction; 6] = [
    Direction::North,
    Direction::East,
    Direction::South,
    Direction::West,
    Direction::Up,
    Direction::Down,
];
fn at(region: u64, x: i32, y: i32, z: i32) -> Location {
    Location {
        region: RegionId(region),
        position: Position { x, y, z },
    }
}
fn room(value: u32) -> RoomId {
    RoomId::new(value).unwrap()
}
fn frame(owner: u32, region: u64, up: Direction) -> MappingRoomFrame {
    MappingRoomFrame {
        room: room(owner),
        region: RegionId(region),
        up,
    }
}
fn cell_run(p: Location) -> CellRun {
    CellRun {
        region: p.region,
        z: p.position.z,
        y: p.position.y,
        first_x: p.position.x,
        last_x: p.position.x,
    }
}
fn certificate_budget() -> CertificateBudget {
    CertificateBudget {
        work: 1_000_000,
        allocation_bytes: 64 * 1024 * 1024,
        cache_bytes: 64 * 1024 * 1024,
    }
}
fn face_budget() -> FaceBudget {
    FaceBudget {
        work: 1_000_000,
        allocation_bytes: 64 * 1024 * 1024,
        faces: 100_000,
    }
}
fn chamber_world(count: u64, dimensions: [i32; 3]) -> World {
    let mut world = World::new(vec![], vec![]).unwrap();
    for id in 1..=count {
        world
            .add_chamber(Region {
                id: RegionId(id),
                name: format!("mapping-{id}"),
                bounds: Extent::new(dimensions[0], dimensions[1], dimensions[2]).unwrap(),
            })
            .unwrap();
    }
    world
}
fn box_cells(region: u64, min: [i32; 3], max: [i32; 3]) -> Vec<Location> {
    (min[2]..=max[2])
        .flat_map(|z| {
            (min[1]..=max[1]).flat_map(move |y| (min[0]..=max[0]).map(move |x| at(region, x, y, z)))
        })
        .collect()
}
fn runs(cells: impl IntoIterator<Item = Location>) -> Vec<CellRun> {
    let keys: BTreeSet<_> = cells
        .into_iter()
        .map(|p| (p.region, p.position.z, p.position.y, p.position.x))
        .collect();
    let mut result: Vec<CellRun> = Vec::new();
    for (region, z, y, x) in keys {
        if let Some(last) = result.last_mut().filter(|last| {
            (last.region, last.z, last.y) == (region, z, y) && last.last_x.checked_add(1) == Some(x)
        }) {
            last.last_x = x;
        } else {
            result.push(CellRun {
                region,
                z,
                y,
                first_x: x,
                last_x: x,
            });
        }
    }
    result
}
// Build the pre-existing primary/lighting source facts, NOT a mapping oracle.
// World remains the physical neighbor authority. Every expected mapping face
// below is independently named or constructed from literal boundary planes.
fn source_scope(
    world: &World,
    interiors: &[(u32, Vec<Location>)],
    doors: &[(Location, u32)],
    passages: &[Location],
) -> Scope {
    let passage = PassageId::new(1).unwrap();
    let mut primary = BTreeMap::new();
    let mut rooms = Vec::new();
    for (owner, cells) in interiors {
        assert!(cells.iter().all(|p| world.walkable(*p)));
        let membership = Membership::new(PrimarySpace::Room(room(*owner)), None).unwrap();
        let mut surfaces: BTreeSet<_> = cells.iter().copied().collect();
        for p in cells {
            assert!(primary.insert(*p, membership).is_none());
            for direction in SIX {
                if let Some((q, _)) = world.physics_neighbor(*p, direction) {
                    if world.is_wall(q) {
                        surfaces.insert(q);
                    }
                }
            }
        }
        rooms.push(RoomSurfaces {
            room: room(*owner),
            runs: runs(surfaces),
        });
    }
    for (p, owner) in doors {
        primary.insert(
            *p,
            Membership::new(PrimarySpace::Room(room(*owner)), Some(passage)).unwrap(),
        );
    }
    for p in passages {
        assert!(primary
            .insert(
                *p,
                Membership::new(PrimarySpace::Passage(passage), None).unwrap()
            )
            .is_none());
    }
    let members = primary
        .keys()
        .map(|p| p.region)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let mut rows: Vec<_> = primary
        .into_iter()
        .map(|(p, membership)| PrimaryRun {
            cells: cell_run(p),
            membership,
        })
        .collect();
    rows.sort_by_key(|row| {
        (
            row.cells.region,
            row.cells.z,
            row.cells.y,
            row.cells.first_x,
        )
    });
    let mut merged: Vec<PrimaryRun> = Vec::new();
    for row in rows {
        if let Some(last) = merged.last_mut().filter(|last| {
            last.membership == row.membership
                && (last.cells.region, last.cells.z, last.cells.y)
                    == (row.cells.region, row.cells.z, row.cells.y)
                && last.cells.last_x.checked_add(1) == Some(row.cells.first_x)
        }) {
            last.cells.last_x = row.cells.last_x;
        } else {
            merged.push(row);
        }
    }
    rooms.sort_by_key(|entry| entry.room);
    let scope = Scope {
        id: "mapping-native".into(),
        members,
        primary: merged,
        rooms,
        mapping: MappingCertification::Uncertified,
    };
    scope
        .validate(&Budget {
            scopes: 1,
            members: 100,
            rooms: 100,
            passages: 100,
            runs: 100_000,
            expanded_refs: 1_000_000,
            label_bytes: 100,
        })
        .unwrap();
    scope
}
fn certify(world: &World, scope: &mut Scope, frames: &[MappingRoomFrame], exterior: &[CellRun]) {
    scope.mapping = mapping::certify(world, scope, frames, exterior, certificate_budget())
        .expect("complete valid source certificate");
    assert!(matches!(scope.mapping, MappingCertification::Certified(_)));
}
fn faces(world: &World, scope: &Scope) -> BTreeSet<(RoomId, Location, Direction)> {
    let raw = mapping::wall_faces(world, scope, face_budget()).unwrap();
    let result: BTreeSet<_> = raw
        .iter()
        .map(|face| (face.room, face.location, face.normal))
        .collect();
    assert_eq!(
        raw.len(),
        result.len(),
        "oriented faces must not be duplicated"
    );
    result
}

#[test]
fn absent_metadata_is_unavailable_but_explicit_empty_passage_certificate_is_available() {
    let world = chamber_world(1, [2, 1, 1]);
    let mut scope = source_scope(&world, &[], &[], &[at(1, 0, 0, 0), at(1, 1, 0, 0)]);
    assert!(matches!(
        mapping::wall_faces(&world, &scope, face_budget()),
        Err(MappingUnavailable::Uncertified)
    ));
    certify(&world, &mut scope, &[], &[]);
    let MappingCertification::Certified(saved) = &scope.mapping else {
        unreachable!()
    };
    assert!(saved.room_frames.is_empty() && saved.exterior_doorways.is_empty());
    assert!(faces(&world, &scope).is_empty());
}

#[test]
fn explicit_sideways_construction_keeps_multilevel_walls_independent_of_gravity() {
    let mut world = chamber_world(1, [3, 2, 3]);
    let mut scope = source_scope(&world, &[(1, box_cells(1, [0, 0, 0], [2, 1, 2]))], &[], &[]);
    certify(&world, &mut scope, &[frame(1, 1, Direction::East)], &[]);
    let mut expected = BTreeSet::new();
    for x in 0..3 {
        for z in 0..3 {
            expected.insert((room(1), at(1, x, -1, z), Direction::South));
            expected.insert((room(1), at(1, x, 2, z), Direction::North));
        }
    }
    for x in 0..3 {
        for y in 0..2 {
            expected.insert((room(1), at(1, x, y, -1), Direction::Up));
            expected.insert((room(1), at(1, x, y, 3), Direction::Down));
        }
    }
    assert_eq!(expected.len(), 30);
    assert_eq!(faces(&world, &scope), expected);
    world.set_gravity(RegionId(1), [0, 0, 0]).unwrap();
    world.set_cell_gravity(at(1, 0, 0, 0), [0, -1, 0]).unwrap();
    world.set_cell_gravity(at(1, 2, 1, 2), [1, 0, 0]).unwrap();
    assert_eq!(
        faces(&world, &scope),
        expected,
        "construction is not live gravity"
    );
    assert!(mapping::wall_faces(
        &world,
        &scope,
        FaceBudget {
            faces: 30,
            ..face_budget()
        }
    )
    .is_ok());
    assert!(
        matches!(
            mapping::wall_faces(
                &world,
                &scope,
                FaceBudget {
                    faces: 29,
                    ..face_budget()
                }
            ),
            Err(MappingUnavailable::Capacity)
        ),
        "never truncate a preparation result"
    );
}

#[test]
fn a_shared_ceiling_and_wall_cell_keeps_only_its_independent_wall_face() {
    let mut world = chamber_world(1, [3, 3, 3]);
    for p in box_cells(1, [0, 0, 0], [2, 2, 2]) {
        world.set_wall(p, true).unwrap();
    }
    let lower = at(1, 1, 1, 1);
    let upper = at(1, 0, 1, 2);
    for p in [lower, upper] {
        world.set_wall(p, false).unwrap();
    }
    let shared = at(1, 1, 1, 2);
    assert_eq!(
        world.physics_neighbor(lower, Direction::Up),
        Some((shared, 0))
    );
    assert_eq!(
        world.physics_neighbor(upper, Direction::East),
        Some((shared, 0))
    );
    let mut scope = source_scope(&world, &[(1, vec![lower, upper])], &[], &[]);
    certify(&world, &mut scope, &[frame(1, 1, Direction::Up)], &[]);
    let actual = faces(&world, &scope);
    assert!(actual.contains(&(room(1), shared, Direction::West)));
    assert!(!actual.contains(&(room(1), shared, Direction::Down)));
    assert_eq!(actual.iter().filter(|(_, p, _)| *p == shared).count(), 1);
}

#[test]
fn exterior_doorway_subset_is_excluded_but_internal_doorway_remains_shell_basis() {
    let world = chamber_world(1, [5, 1, 1]);
    let internal = at(1, 1, 0, 0);
    let exterior = at(1, 2, 0, 0);
    let mut scope = source_scope(
        &world,
        &[(1, vec![at(1, 0, 0, 0), internal])],
        &[(internal, 1), (exterior, 1)],
        &[at(1, 3, 0, 0), at(1, 4, 0, 0)],
    );
    let basis = scope.primary.clone();
    certify(
        &world,
        &mut scope,
        &[frame(1, 1, Direction::Up)],
        &[cell_run(exterior)],
    );
    let MappingCertification::Certified(saved) = &scope.mapping else {
        unreachable!()
    };
    assert_eq!(saved.exterior_doorways, vec![cell_run(exterior)]);
    assert_eq!(
        scope.primary, basis,
        "certification must not rewrite doorway authority"
    );
    let actual = faces(&world, &scope);
    assert!(actual.contains(&(room(1), at(1, 1, -1, 0), Direction::South)));
    assert!(!actual.contains(&(room(1), at(1, 2, -1, 0), Direction::South)));
    assert!(!actual
        .iter()
        .any(|(_, p, _)| *p == internal || *p == exterior));
    for invalid in [
        vec![cell_run(at(1, 0, 0, 0))],
        vec![cell_run(at(1, 3, 0, 0))],
        vec![cell_run(exterior), cell_run(exterior)],
    ] {
        assert!(matches!(
            mapping::certify(
                &world,
                &scope,
                &[frame(1, 1, Direction::Up)],
                &invalid,
                certificate_budget()
            ),
            Err(mapping::Error::InvalidSource)
        ));
    }
}

#[test]
fn rotated_same_room_join_requires_both_exact_incidence_frames() {
    let mut world = chamber_world(2, [2, 2, 2]);
    let from = at(1, 1, 0, 0);
    let to = at(2, 0, 0, 0);
    world
        .connect_portal_area(
            Passage {
                from,
                direction: Direction::East,
                to,
            },
            8,
            1,
            1,
        )
        .unwrap();
    assert_eq!(Direction::Up.rotated(8), Direction::North);
    assert_eq!(world.crossing_rotation(from, Direction::East), 8);
    let interior = [
        box_cells(1, [0, 0, 0], [1, 1, 1]),
        box_cells(2, [0, 0, 0], [1, 1, 1]),
    ]
    .concat();
    let mut scope = source_scope(&world, &[(1, interior)], &[], &[]);
    certify(
        &world,
        &mut scope,
        &[frame(1, 1, Direction::Up), frame(1, 2, Direction::North)],
        &[],
    );
    let MappingCertification::Certified(saved) = &scope.mapping else {
        unreachable!()
    };
    assert_eq!(
        saved
            .room_frames
            .iter()
            .map(|f| (f.room, f.region, f.up))
            .collect::<Vec<_>>(),
        vec![
            (room(1), RegionId(1), Direction::Up),
            (room(1), RegionId(2), Direction::North)
        ]
    );
    assert!(faces(&world, &scope).contains(&(room(1), at(2, 1, 1, -1), Direction::Up)));
    assert!(!faces(&world, &scope).contains(&(room(1), at(2, 1, -1, 1), Direction::South)));
    for bad in [
        vec![frame(1, 1, Direction::Up)],
        vec![frame(1, 1, Direction::Up), frame(1, 2, Direction::Up)],
        vec![
            frame(1, 1, Direction::Up),
            frame(1, 2, Direction::North),
            frame(1, 2, Direction::North),
        ],
    ] {
        assert!(matches!(
            mapping::certify(&world, &scope, &bad, &[], certificate_budget()),
            Err(mapping::Error::InvalidSource)
        ));
    }
}

#[test]
fn foreign_solid_face_transforms_inward_normal_without_inventing_membership_or_frame() {
    let mut world = chamber_world(2, [2, 2, 2]);
    let target = at(2, 0, 0, 0);
    world.set_wall(target, true).unwrap();
    let from = at(1, 1, 0, 0);
    world
        .connect_boundary_area(
            Passage {
                from,
                direction: Direction::East,
                to: target,
            },
            1,
            1,
            1,
        )
        .unwrap();
    assert_eq!(
        world.physics_neighbor(from, Direction::East),
        Some((target, 1))
    );
    let mut scope = source_scope(&world, &[(1, box_cells(1, [0, 0, 0], [1, 1, 1]))], &[], &[]);
    certify(&world, &mut scope, &[frame(1, 1, Direction::Up)], &[]);
    assert_eq!(scope.members, vec![RegionId(1)]);
    assert!(scope.primary.iter().all(|row| !row.cells.contains(target)));
    assert!(faces(&world, &scope).contains(&(room(1), target, Direction::North)));
    assert!(!faces(&world, &scope).contains(&(room(1), target, Direction::West)));
    assert!(matches!(
        mapping::certify(
            &world,
            &scope,
            &[frame(1, 1, Direction::Up), frame(1, 2, Direction::Up)],
            &[],
            certificate_budget()
        ),
        Err(mapping::Error::InvalidSource)
    ));
}

fn opaque_world() -> World {
    let mut world = chamber_world(2, [3, 1, 1]);
    for region in 1..=2 {
        for x in [0, 2] {
            world.set_wall(at(region, x, 0, 0), true).unwrap();
        }
    }
    world
}
#[test]
fn every_opaque_explicit_join_is_checked_even_after_a_valid_first_join_and_cache_exhaustion() {
    let mut world = opaque_world();
    world
        .connect_boundary_area(
            Passage {
                from: at(1, 2, 0, 0),
                direction: Direction::East,
                to: at(2, 0, 0, 0),
            },
            0,
            1,
            1,
        )
        .unwrap();
    let scope = source_scope(
        &world,
        &[(1, vec![at(1, 1, 0, 0), at(2, 1, 0, 0)])],
        &[],
        &[],
    );
    let frames = [frame(1, 1, Direction::Up), frame(1, 2, Direction::Up)];
    assert!(matches!(
        mapping::certify(&world, &scope, &frames, &[], certificate_budget()),
        Ok(MappingCertification::Certified(_))
    ));
    assert_eq!(
        world.physics_neighbor(at(1, 2, 0, 0), Direction::East),
        None
    );
    world
        .connect_boundary_area(
            Passage {
                from: at(1, 0, 0, 0),
                direction: Direction::West,
                to: at(2, 2, 0, 0),
            },
            8,
            1,
            1,
        )
        .unwrap();
    assert_eq!(world.exits(RegionId(1)).count(), 2);
    assert_eq!(
        world.physics_neighbor(at(1, 0, 0, 0), Direction::West),
        None
    );
    for cache_bytes in [64 * 1024 * 1024, 0] {
        assert!(
            matches!(
                mapping::certify(
                    &world,
                    &scope,
                    &frames,
                    &[],
                    CertificateBudget {
                        cache_bytes,
                        ..certificate_budget()
                    }
                ),
                Err(mapping::Error::InvalidSource)
            ),
            "later opaque contradiction cannot become unavailable"
        );
    }
}

#[test]
fn opaque_self_region_join_cannot_hide_a_rotated_frame_contradiction() {
    for rotation in [0, 8] {
        let mut world = opaque_world();
        world
            .connect_boundary_area(
                Passage {
                    from: at(1, 2, 0, 0),
                    direction: Direction::East,
                    to: at(1, 0, 0, 0),
                },
                rotation,
                1,
                1,
            )
            .unwrap();
        let scope = source_scope(&world, &[(1, vec![at(1, 1, 0, 0)])], &[], &[]);
        let actual = mapping::certify(
            &world,
            &scope,
            &[frame(1, 1, Direction::Up)],
            &[],
            certificate_budget(),
        );
        if rotation == 0 {
            assert!(matches!(actual, Ok(MappingCertification::Certified(_))));
        } else {
            assert!(matches!(actual, Err(mapping::Error::InvalidSource)));
        }
    }
}

#[test]
fn transport_stairs_do_not_align_frames_but_physical_vertical_joins_do() {
    for physical in [false, true] {
        let mut world = chamber_world(2, [2, 2, 2]);
        let link = Passage {
            from: at(1, 0, 0, 1),
            direction: Direction::Up,
            to: at(2, 0, 0, 0),
        };
        if physical {
            world.connect_portal_area(link, 0, 1, 1).unwrap();
        } else {
            world.connect_area(link, 0, 1, 1).unwrap();
        }
        assert_eq!(world.is_stair(at(1, 0, 0, 1), Direction::Up), !physical);
        let interior = [
            box_cells(1, [0, 0, 0], [1, 1, 1]),
            box_cells(2, [0, 0, 0], [1, 1, 1]),
        ]
        .concat();
        let scope = source_scope(&world, &[(1, interior)], &[], &[]);
        let actual = mapping::certify(
            &world,
            &scope,
            &[frame(1, 1, Direction::Up), frame(1, 2, Direction::North)],
            &[],
            certificate_budget(),
        );
        if physical {
            assert!(matches!(actual, Err(mapping::Error::InvalidSource)));
        } else {
            assert!(matches!(actual, Ok(MappingCertification::Certified(_))));
        }
    }
}

#[test]
fn two_rooms_in_one_region_require_distinct_incidence_rows_not_one_region_frame() {
    let mut world = chamber_world(1, [3, 1, 1]);
    world.set_wall(at(1, 1, 0, 0), true).unwrap();
    let mut scope = source_scope(
        &world,
        &[(1, vec![at(1, 0, 0, 0)]), (2, vec![at(1, 2, 0, 0)])],
        &[],
        &[],
    );
    certify(
        &world,
        &mut scope,
        &[frame(1, 1, Direction::Up), frame(2, 1, Direction::East)],
        &[],
    );
    assert!(faces(&world, &scope).contains(&(room(1), at(1, 0, -1, 0), Direction::South)));
    assert!(faces(&world, &scope).contains(&(room(2), at(1, 2, 0, -1), Direction::Up)));
    assert!(matches!(
        mapping::certify(
            &world,
            &scope,
            &[frame(1, 1, Direction::Up)],
            &[],
            certificate_budget()
        ),
        Err(mapping::Error::InvalidSource)
    ));
}

#[test]
fn only_valid_complete_certificates_can_become_explicit_capacity_unavailable() {
    let world = chamber_world(1, [2, 2, 2]);
    let mut scope = source_scope(&world, &[(1, box_cells(1, [0, 0, 0], [1, 1, 1]))], &[], &[]);
    let valid = [frame(1, 1, Direction::Up)];
    certify(&world, &mut scope, &valid, &[]);
    let no_cache = CertificateBudget {
        cache_bytes: 0,
        ..certificate_budget()
    };
    scope.mapping = mapping::certify(&world, &scope, &valid, &[], no_cache).unwrap();
    assert!(matches!(
        scope.mapping,
        MappingCertification::CapacityUnavailable
    ));
    assert!(matches!(
        mapping::wall_faces(&world, &scope, face_budget()),
        Err(MappingUnavailable::Capacity)
    ));
    for malformed in [
        vec![],
        vec![frame(1, 1, Direction::NorthEast)],
        vec![frame(2, 1, Direction::Up)],
        vec![frame(1, 2, Direction::Up)],
        vec![frame(1, 1, Direction::Up), frame(1, 1, Direction::Up)],
    ] {
        assert!(
            matches!(
                mapping::certify(&world, &scope, &malformed, &[], no_cache),
                Err(mapping::Error::InvalidSource)
            ),
            "cache exhaustion must not mask bad metadata"
        );
    }
    assert!(matches!(
        mapping::certify(
            &world,
            &scope,
            &valid,
            &[],
            CertificateBudget {
                work: 0,
                ..certificate_budget()
            }
        ),
        Err(mapping::Error::Work)
    ));
    assert!(matches!(
        mapping::certify(
            &world,
            &scope,
            &valid,
            &[],
            CertificateBudget {
                allocation_bytes: 0,
                ..certificate_budget()
            }
        ),
        Err(mapping::Error::Allocation)
    ));
}

#[test]
fn exterior_only_member_incidence_requires_no_invented_room_frame() {
    let mut world = chamber_world(2, [2, 1, 1]);
    world
        .connect_portal_area(
            Passage {
                from: at(1, 1, 0, 0),
                direction: Direction::East,
                to: at(2, 0, 0, 0),
            },
            0,
            1,
            1,
        )
        .unwrap();
    let external = at(2, 0, 0, 0);
    let mut scope = source_scope(
        &world,
        &[(1, vec![at(1, 0, 0, 0), at(1, 1, 0, 0)])],
        &[(external, 1)],
        &[at(2, 1, 0, 0)],
    );
    certify(
        &world,
        &mut scope,
        &[frame(1, 1, Direction::Up)],
        &[cell_run(external)],
    );
    let actual = faces(&world, &scope);
    assert!(!actual.iter().any(|(_, p, _)| p.region == RegionId(2)));
    assert!(actual.contains(&(room(1), at(1, 1, -1, 0), Direction::South)));
}

#[test]
fn closing_an_internal_multilevel_door_does_not_erase_source_wall_faces() {
    let mut world = chamber_world(1, [1, 1, 2]);
    let bottom = at(1, 0, 0, 0);
    world.place_door(bottom, 1, true, 2).unwrap();
    let mut scope = source_scope(
        &world,
        &[(1, vec![bottom, at(1, 0, 0, 1)])],
        &[(bottom, 1), (at(1, 0, 0, 1), 1)],
        &[],
    );
    certify(&world, &mut scope, &[frame(1, 1, Direction::Up)], &[]);
    let certificate = scope.mapping.clone();
    let open_faces = faces(&world, &scope);
    assert_eq!(open_faces.len(), 8);
    assert!(open_faces.contains(&(room(1), at(1, -1, 0, 1), Direction::East)));
    world.set_door(bottom, false);
    for z in 0..2 {
        let occupied = at(1, 0, 0, z);
        assert!(world.opaque(occupied));
        assert!(!world.is_wall(occupied));
        assert_eq!(world.physics_neighbor(occupied, Direction::West), None);
    }
    assert_eq!(
        faces(&world, &scope),
        open_faces,
        "source geometry must not depend on movement permission from a closed doorway"
    );
    assert_eq!(scope.mapping, certificate);
    world.set_door(bottom, true);
    assert_eq!(faces(&world, &scope), open_faces);
}

#[test]
fn passage_only_certificate_and_faces_still_charge_every_structural_visit() {
    let world = chamber_world(1, [1, 1, 1]);
    let mut scope = source_scope(&world, &[], &[], &[at(1, 0, 0, 0)]);
    let empty = mapping::MappingFrames {
        room_frames: vec![],
        exterior_doorways: vec![],
    };
    // One primary row, even though it is Passage and produces no room frame.
    for (work, succeeds) in [(0, false), (1, true)] {
        assert_eq!(
            mapping::validate_structure(
                &scope,
                &empty,
                CertificateBudget {
                    work,
                    ..certificate_budget()
                }
            )
            .is_ok(),
            succeeds
        );
    }
    // One primary row plus one member with an empty exit iterator. None may
    // become free simply because the output certificate has no room entries.
    for work in [0, 1] {
        assert_eq!(
            mapping::certify(
                &world,
                &scope,
                &[],
                &[],
                CertificateBudget {
                    work,
                    ..certificate_budget()
                }
            ),
            Err(mapping::Error::Work)
        );
    }
    scope.mapping = mapping::certify(
        &world,
        &scope,
        &[],
        &[],
        CertificateBudget {
            work: 2,
            ..certificate_budget()
        },
    )
    .unwrap();
    // Status read + exit-count slot + exitless member + both primary passes.
    for work in [0, 4] {
        assert_eq!(
            mapping::wall_faces(
                &world,
                &scope,
                FaceBudget {
                    work,
                    ..face_budget()
                }
            ),
            Err(MappingUnavailable::Capacity)
        );
    }
    assert!(mapping::wall_faces(
        &world,
        &scope,
        FaceBudget {
            work: 5,
            ..face_budget()
        }
    )
    .unwrap()
    .is_empty());
}

#[test]
fn opaque_unrelated_exits_are_precharged_for_every_geometry_request_in_both_passes() {
    let mut world = opaque_world();
    let mut scope = source_scope(&world, &[(1, vec![at(1, 1, 0, 0)])], &[], &[]);
    certify(&world, &mut scope, &[frame(1, 1, Direction::Up)], &[]);
    let expected = BTreeSet::from([
        (room(1), at(1, 0, 0, 0), Direction::East),
        (room(1), at(1, 2, 0, 0), Direction::West),
        (room(1), at(1, 1, -1, 0), Direction::South),
        (room(1), at(1, 1, 1, 0), Direction::North),
    ]);
    assert_eq!(faces(&world, &scope), expected);
    // Exact fixed scaffold/lookup/cardinal/count/sort/dedup costs for one
    // interior cell and four faces with no explicit exits: 53 work units.
    assert!(mapping::wall_faces(
        &world,
        &scope,
        FaceBudget {
            work: 52,
            ..face_budget()
        }
    )
    .is_err());
    assert_eq!(
        mapping::wall_faces(
            &world,
            &scope,
            FaceBudget {
                work: 53,
                ..face_budget()
            }
        )
        .unwrap()
        .len(),
        4
    );
    world
        .connect_boundary_area(
            Passage {
                from: at(1, 2, 0, 0),
                direction: Direction::East,
                to: at(2, 0, 0, 0),
            },
            0,
            1,
            1,
        )
        .unwrap();
    assert_eq!(world.exits(RegionId(1)).count(), 1);
    assert!(world.opaque(at(1, 2, 0, 0)));
    assert_eq!(
        faces(&world, &scope),
        expected,
        "unrelated opaque exit changes no faces"
    );
    // One charged exit-count visit + eight hidden rim scans (four lateral
    // requests in each of two passes). No repeated uncharged count() call.
    for work in [53, 61] {
        assert_eq!(
            mapping::wall_faces(
                &world,
                &scope,
                FaceBudget {
                    work,
                    ..face_budget()
                }
            ),
            Err(MappingUnavailable::Capacity)
        );
    }
    assert_eq!(
        mapping::wall_faces(
            &world,
            &scope,
            FaceBudget {
                work: 62,
                ..face_budget()
            }
        )
        .unwrap()
        .len(),
        4
    );
}

// Projection integration contract (owned by scope_projection/chart, not this
// geometry producer): rotate each MappingFace.normal with its owning cell's
// resolved frame, without translation. A component rejected as inconsistent
// must publish neither a convenient position nor an orientation. These tests
// establish region-local inward normals only, not downstream chart closure.
