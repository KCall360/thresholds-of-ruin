use tor_world::{Extent, Location, Position, Region, RegionId, World};
fn at(x: i32, y: i32, z: i32) -> Location {
    Location {
        region: RegionId(1),
        position: Position { x, y, z },
    }
}
fn world() -> World {
    World::new(
        vec![Region {
            id: RegionId(1),
            name: "test".into(),
            bounds: Extent::new(40, 7, 3).unwrap(),
        }],
        vec![],
    )
    .unwrap()
}
#[test]
fn lighting_overrides_validate_persist_and_preserve_geometry() {
    let mut w = world();
    let eye = at(2, 3, 1);
    let target = at(14, 3, 1);
    let geometric = w.eye_scene(eye, 0, 16);
    let witness = w.geometry_snapshot();
    let perception = w.perception_snapshot();
    w.set_region_light(RegionId(1), false).unwrap();
    assert_ne!(w.perception_snapshot(), perception);
    assert_eq!(w.is_lit(target), Some(false));
    assert!(w.illuminated_eye_scene(eye, 0, 16).is_empty());
    w.set_cell_light(target, true).unwrap();
    assert_eq!(w.geometry_snapshot(), witness);
    assert!(w.eye_scene_cached(eye, 0, 16));
    assert_eq!(w.eye_scene(eye, 0, 16), geometric);
    assert_eq!(
        w.illuminated_eye_scene(eye, 0, 16)
            .iter()
            .map(|c| c.location)
            .collect::<Vec<_>>(),
        vec![target]
    );
    let clone = w.clone();
    assert_eq!(w.perception_snapshot(), clone.perception_snapshot());
    w.set_cell_light(target, false).unwrap();
    assert_ne!(w.perception_snapshot(), clone.perception_snapshot());
    assert!(w.illuminated_eye_scene(eye, 0, 16).is_empty());
    assert_eq!(
        clone.illuminated_eye_scene(eye, 0, 16).len(),
        1,
        "clones do not share illumination revisions"
    );
    let before = w.clone();
    assert!(w.set_cell_light(at(99, 0, 0), true).is_err());
    assert_eq!(w, before);
    assert!(w.set_region_light(RegionId(99), false).is_err());
    assert_eq!(w, before);
    w.set_cell_light(target, true).unwrap();
    w.set_wall(target, true).unwrap();
    assert_eq!(w.is_lit(target), Some(true));
    w.set_wall(target, false).unwrap();
    let bytes = serde_json::to_vec(&w).unwrap();
    let mut restored: World = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(restored, w);
    let slice = restored.detach_region(RegionId(1)).unwrap();
    assert_eq!(restored.is_lit(target), None);
    let saved = serde_json::to_vec(&slice).unwrap();
    restored
        .attach_region(serde_json::from_slice(&saved).unwrap())
        .unwrap();
    assert_eq!(restored, w);
    assert_eq!(restored.is_lit(target), Some(true));
}
#[test]
fn darkness_is_transparent_but_opaque_terrain_blocks_lit_targets() {
    let mut w = world();
    w.set_region_light(RegionId(1), false).unwrap();
    let eye = at(2, 3, 1);
    let target = at(14, 3, 1);
    w.set_cell_light(target, true).unwrap();
    assert!(w
        .illuminated_eye_scene(eye, 0, 16)
        .iter()
        .any(|c| c.location == target));
    for y in 0..7 {
        for z in 0..3 {
            w.set_wall(at(8, y, z), true).unwrap();
        }
    }
    assert!(!w
        .illuminated_eye_scene(eye, 0, 16)
        .iter()
        .any(|c| c.location == target));
}
#[test]
fn illumination_uses_exact_range_and_local_awareness_ignores_occlusion() {
    let mut w = world();
    let eye = at(2, 3, 1);
    w.set_region_light(RegionId(1), false).unwrap();
    w.set_cell_light(at(18, 3, 1), true).unwrap();
    w.set_cell_light(at(19, 3, 1), true).unwrap();
    let seen = w.illuminated_eye_scene(eye, 0, 16);
    assert!(seen.iter().any(|c| c.location == at(18, 3, 1)));
    assert!(!seen.iter().any(|c| c.location == at(19, 3, 1)));
    w.set_wall(at(3, 3, 1), true).unwrap();
    w.set_wall(at(2, 4, 1), true).unwrap();
    let local = w.neighborhood_scene(eye, 0);
    assert_eq!(local.len(), 27);
    assert!(local.iter().any(|c| c.location == at(3, 4, 2)));
    assert!(!local.iter().any(|c| c.location == at(4, 3, 1)));
}

#[test]
fn lighting_and_local_awareness_follow_rotated_physical_joins_and_reload() {
    use tor_world::{Direction, Passage};
    let mut w = world();
    w.add_region(Region {
        id: RegionId(2),
        name: "joined".into(),
        bounds: Extent::new(7, 7, 3).unwrap(),
    })
    .unwrap();
    let eye = at(39, 3, 1);
    let target = Location {
        region: RegionId(2),
        position: Position { x: 3, y: 0, z: 1 },
    };
    w.connect(
        Passage {
            from: eye,
            direction: Direction::East,
            to: target,
        },
        1,
    )
    .unwrap();
    w.set_region_light(RegionId(1), false).unwrap();
    w.set_region_light(RegionId(2), false).unwrap();
    for frame in 0..24 {
        let local = w.neighborhood_scene(eye, frame);
        assert!(local.iter().any(|c| c.location == target));
        let crossing = local.iter().find(|c| c.location == target).unwrap();
        assert_eq!([crossing.offset.x, crossing.offset.y, crossing.offset.z], {
            let (x, y, z) = Direction::East
                .rotated(tor_world::inverse_rotation(frame))
                .delta();
            [x, y, z]
        });
        assert_eq!(crossing.rotation, tor_world::compose_rotation(frame, 1));
        w.set_cell_light(target, true).unwrap();
        let expected: Vec<_> = w
            .eye_scene(eye, frame, 16)
            .into_iter()
            .filter(|c| w.is_lit(c.location) == Some(true))
            .collect();
        assert_eq!(w.illuminated_eye_scene(eye, frame, 16), expected);
        w.set_cell_light(target, false).unwrap();
        assert!(w.illuminated_eye_scene(eye, frame, 16).is_empty());
    }
    w.set_wall(target, true).unwrap();
    assert!(w
        .neighborhood_scene(eye, 0)
        .iter()
        .any(|c| c.location == target && c.wall));
    let slice = w.detach_region(RegionId(2)).unwrap();
    assert!(!w
        .neighborhood_scene(eye, 0)
        .iter()
        .any(|c| c.location == target));
    w.attach_region(slice).unwrap();
    assert!(w
        .neighborhood_scene(eye, 0)
        .iter()
        .any(|c| c.location == target && c.wall));
}

#[test]
fn compact_world_checkpoints_do_not_coalesce_different_lighting() {
    #[derive(serde::Serialize, serde::Deserialize)]
    struct States {
        #[serde(with = "tor_world::checkpoint_worlds")]
        worlds: Vec<World>,
    }
    let lit = world();
    let mut dark = lit.clone();
    dark.set_region_light(RegionId(1), false).unwrap();
    let mut spotted = dark.clone();
    spotted.set_cell_light(at(2, 3, 1), true).unwrap();
    let states = States {
        worlds: vec![lit, dark, spotted],
    };
    let restored: States = serde_json::from_slice(&serde_json::to_vec(&states).unwrap()).unwrap();
    assert_eq!(restored.worlds, states.worlds);
}

#[test]
fn slab_bounds_remain_exact_across_rotations_and_height_changing_joins() {
    use tor_world::{Direction, Passage};
    let mut w = world();
    w.add_region(Region {
        id: RegionId(2),
        name: "joined".into(),
        bounds: Extent::new(40, 7, 3).unwrap(),
    })
    .unwrap();
    let destination = Location {
        region: RegionId(2),
        position: Position { x: 0, y: 3, z: 1 },
    };
    w.connect_area(
        Passage {
            from: at(39, 3, 1),
            direction: Direction::East,
            to: destination,
        },
        0,
        1,
        1,
    )
    .unwrap();
    for frame in 0..24 {
        let eye = at(37, 3, 1);
        let expected = w.eye_scene_reference(eye, frame, 16);
        assert_eq!(
            w.eye_scene_uncached(eye, frame, 16),
            expected,
            "frame {frame}"
        );
        assert_eq!(w.illuminated_eye_scene_uncached(eye, frame, 16), expected);
    }
    // A join changes the height chart, so slab clipping must fall back.
    w.connect(
        Passage {
            from: at(39, 4, 2),
            direction: Direction::East,
            to: Location {
                region: RegionId(2),
                position: Position { x: 0, y: 4, z: 0 },
            },
        },
        0,
    )
    .unwrap();
    let eye = at(37, 4, 1);
    assert_eq!(
        w.eye_scene_uncached(eye, 0, 16),
        w.eye_scene_reference(eye, 0, 16)
    );
    let slice = w.detach_region(RegionId(2)).unwrap();
    assert_eq!(w.eye_scene(eye, 0, 16), w.eye_scene_reference(eye, 0, 16));
    w.attach_region(slice).unwrap();
    assert_eq!(w.eye_scene(eye, 0, 16), w.eye_scene_reference(eye, 0, 16));
}

#[test]
fn slab_proof_cap_does_not_expand_streaming_dependencies() {
    use tor_world::{Direction, Passage};
    let regions = (1..=33)
        .map(|id| Region {
            id: RegionId(id),
            name: String::new(),
            bounds: Extent::new(1, 1, 1).unwrap(),
        })
        .collect();
    let mut w = World::new(regions, vec![]).unwrap();
    let cell = |id| Location {
        region: RegionId(id),
        position: Position { x: 0, y: 0, z: 0 },
    };
    for id in 1..33 {
        w.connect(
            Passage {
                from: cell(id),
                direction: Direction::East,
                to: cell(id + 1),
            },
            0,
        )
        .unwrap();
    }
    assert_eq!(
        w.eye_scene(cell(1), 0, 16),
        w.eye_scene_reference(cell(1), 0, 16)
    );
    let dependencies = w.eye_scene_regions(cell(1), 0, 16).unwrap();
    assert!(dependencies.len() < 33);
    assert!(!dependencies.contains(&RegionId(33)));
}

#[test]
fn transparent_volume_shortcut_preserves_rays_and_blocker_invalidation() {
    let mut w = World::new(vec![], vec![]).unwrap();
    w.add_chamber(Region {
        id: RegionId(1),
        name: String::new(),
        bounds: Extent::new(20, 7, 4).unwrap(),
    })
    .unwrap();
    let eye = at(2, 3, 1);
    for frame in [0, 5, 17] {
        assert_eq!(
            w.eye_scene(eye, frame, 16),
            w.eye_scene_reference(eye, frame, 16)
        );
    }
    w.place_door(at(8, 3, 1), 1, false, 2).unwrap();
    assert_eq!(w.eye_scene(eye, 0, 16), w.eye_scene_reference(eye, 0, 16));
    w.set_door(at(8, 3, 1), true);
    assert_eq!(w.eye_scene(eye, 0, 16), w.eye_scene_reference(eye, 0, 16));
    for y in 0..7 {
        for z in 0..4 {
            w.set_wall(at(9, y, z), true).unwrap();
        }
    }
    assert_eq!(w.eye_scene(eye, 0, 16), w.eye_scene_reference(eye, 0, 16));
    assert!(!w
        .eye_scene(eye, 0, 16)
        .iter()
        .any(|cell| cell.location == at(17, 3, 1)));
}

#[test]
fn batch_adjacency_preserves_steps_on_rotated_joins_doors_and_stairs() {
    use tor_world::{Direction, Passage};
    let mut w = World::new(vec![], vec![]).unwrap();
    for (id, width, depth) in [(1, 7, 15), (2, 15, 7)] {
        w.add_chamber(Region {
            id: RegionId(id),
            name: String::new(),
            bounds: Extent::new(width, depth, 2).unwrap(),
        })
        .unwrap();
    }
    let second = |x, y, z| Location {
        region: RegionId(2),
        position: Position { x, y, z },
    };
    w.connect_area(
        Passage {
            from: at(6, 0, 0),
            direction: Direction::East,
            to: second(14, 0, 0),
        },
        1,
        15,
        2,
    )
    .unwrap();
    w.connect(
        Passage {
            from: at(1, 1, 0),
            direction: Direction::Up,
            to: second(13, 3, 0),
        },
        0,
    )
    .unwrap();
    w.place_door(at(2, 4, 0), 1, false, 2).unwrap();
    for open in [false, true] {
        w.set_door(at(2, 4, 0), open);
        let mut adjacency = w.adjacency();
        for (id, width, depth) in [(1, 7, 15), (2, 15, 7)] {
            for x in 0..width {
                for y in 0..depth {
                    for z in 0..2 {
                        let from = Location {
                            region: RegionId(id),
                            position: Position { x, y, z },
                        };
                        if !w.walkable(from) {
                            continue;
                        }
                        for direction in [
                            Direction::North,
                            Direction::East,
                            Direction::South,
                            Direction::West,
                            Direction::Up,
                            Direction::Down,
                        ] {
                            let got = adjacency
                                .resolve(from, direction)
                                .filter(|(to, _)| w.walkable(*to));
                            let expected = w
                                .step(from, direction)
                                .map(|to| (to, w.crossing_rotation(from, direction)));
                            assert_eq!(got, expected, "{from:?} {direction:?}");
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn finite_slab_proof_invalidates_without_pinning_remote_regions() {
    use tor_world::{Direction, Passage};
    let mut w = world();
    for id in 2..=4 {
        w.add_region(Region {
            id: RegionId(id),
            name: String::new(),
            bounds: Extent::new(40, 7, 3).unwrap(),
        })
        .unwrap();
    }
    for id in 1..4 {
        w.connect(
            Passage {
                from: Location {
                    region: RegionId(id),
                    position: Position { x: 39, y: 3, z: 1 },
                },
                direction: Direction::East,
                to: Location {
                    region: RegionId(id + 1),
                    position: Position { x: 0, y: 3, z: 1 },
                },
            },
            0,
        )
        .unwrap();
    }
    let eye = at(2, 3, 1);
    w.eye_scene(eye, 0, 16);
    assert_eq!(
        w.eye_scene_regions(eye, 0, 16).unwrap(),
        vec![RegionId(1), RegionId(2)]
    );
    assert!(w.eye_scene_cached(eye, 0, 16));
    let far = w.detach_region(RegionId(4)).unwrap();
    assert!(
        !w.eye_scene_cached(eye, 0, 16),
        "proof witnesses still invalidate the cache"
    );
    assert_eq!(w.eye_scene(eye, 0, 16), w.eye_scene_reference(eye, 0, 16));
    assert_eq!(
        w.eye_scene_regions(eye, 0, 16).unwrap(),
        vec![RegionId(1), RegionId(2)]
    );
    w.attach_region(far).unwrap();
    assert_eq!(w.eye_scene(eye, 0, 16), w.eye_scene_reference(eye, 0, 16));
}
