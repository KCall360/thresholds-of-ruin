//! Nonvisual disclosure contracts. These use existing public scene/condition APIs
//! so the pre-implementation failure is missing disclosure, not missing symbols.
use super::*;
use crate::attributes::{Attribute, Attributes};
use crate::perception::{contract_fixtures as fixtures, ActorDisclosure, PerceptionValue};
use tor_world::{Concealment, Extent, Passage, World};

fn point(region: u64, x: i32, y: i32, z: i32) -> Location {
    Location {
        region: RegionId(region),
        position: Position { x, y, z },
    }
}
fn fixture(awareness: u16, subject: &[PerceptionValue]) -> Game {
    let authored = fixtures::fixture_with(&[], subject, |document, owner, _| {
        // Species defaults obey the acquisition cap. Higher effective values
        // must use an ordinary source modifier, not an invalid base attribute.
        assert!(awareness <= document.constants.derived_attribute_cap);
        let base = awareness.min(document.constants.attribute_cap);
        let mut values = owner.default_attributes.values();
        values.awareness = base;
        owner.default_attributes = Attributes::new(values).unwrap();
        if awareness > base {
            owner.modifiers.push(crate::rules::Modifier {
                scope: crate::rules::Scope::Always,
                op: crate::rules::ModifierOp::Add {
                    stat: crate::rules::Stat::Attribute {
                        attribute: Attribute::Awareness,
                    },
                    value: i64::from(awareness - base),
                },
            });
        }
        document.conditions.push(serde_json::from_value(serde_json::json!({
            "id":"aware_boost", "family":"aware_boost", "name":"Aware", "lifecycle":"timed",
            "timing":{"kind":"actor_active","on_expiry":{"kind":"remove"}},
            "modifiers":[{"scope":"always","op":{"kind":"add","stat":{"kind":"attribute","attribute":"awareness"},"value":2}}],
            "stacking":{"kind":"longest"}, "source_scoped":false, "descriptors":[],
            "capacity":"reject", "restrictions":[], "preparation":"preserve"
        })).unwrap());
    });
    let mut world = World::new(vec![], vec![]).unwrap();
    for region in 1..=2 {
        world
            .add_chamber(Region {
                id: RegionId(region),
                name: format!("awareness {region}"),
                bounds: Extent::new(45, 5, 3).unwrap(),
            })
            .unwrap();
    }
    let mut game = Game::new(world, 42, authored.content().clone());
    assert_eq!(
        fixtures::spawn(&mut game, "fixture_perception_owner", point(1, 20, 2, 0)),
        fixtures::OWNER
    );
    assert_eq!(
        fixtures::spawn(&mut game, "fixture_perception_subject", point(1, 25, 2, 0)),
        fixtures::SUBJECT
    );
    assert_eq!(
        game.creature(fixtures::OWNER)
            .unwrap()
            .derived()
            .attributes
            .get(Attribute::Awareness),
        awareness
    );
    assert_eq!(
        restore(&game),
        game,
        "valid current fixture before sensory changes"
    );
    game
}
fn locations(game: &Game) -> BTreeSet<Location> {
    game.scene(fixtures::OWNER)
        .unwrap()
        .into_iter()
        .filter(|c| c.physical)
        .map(|c| c.location)
        .collect()
}
fn dark(game: &mut Game) {
    game.world.set_region_light(RegionId(1), false).unwrap();
    game.world.set_region_light(RegionId(2), false).unwrap();
}
fn restore(game: &Game) -> Game {
    let mut shared = crate::checkpoint::SharedState::default();
    let saved = game.checkpoint(&mut shared);
    let saved = serde_json::from_slice(&serde_json::to_vec(&saved).unwrap()).unwrap();
    let shared = serde_json::from_slice(&serde_json::to_vec(&shared).unwrap()).unwrap();
    Game::restore_checkpoint(saved, &shared, game.content().clone()).unwrap()
}

#[test]
fn maximum_body_nonvisual_scene_is_complete_bounded_and_reuses_geometry() {
    let content = fixture(7, &[]).content().clone();
    let mut world = World::new(vec![], vec![]).unwrap();
    world
        .add_chamber(Region {
            id: RegionId(1),
            name: "maximum body envelope".into(),
            bounds: Extent::new(49, 49, 49).unwrap(),
        })
        .unwrap();
    world.set_region_light(RegionId(1), false).unwrap();
    let mut game = Game::new(world, 42, content);
    let origin = point(1, 24, 24, 24);
    assert_eq!(
        fixtures::spawn(&mut game, "fixture_perception_owner", origin),
        fixtures::OWNER
    );
    let mut body = Vec::new();
    for x in [-8, -3, 3, 8] {
        for y in [-8, -3, 3, 8] {
            for z in [-8, -3, 3, 8] {
                body.push([x, y, z]);
            }
        }
    }
    body[63] = [0, 0, 0];
    assert_eq!(body.len(), 64);
    game.set_body(
        fixtures::OWNER,
        crate::BodySpec {
            cells: body.clone(),
            eye: [0, 0, 0],
            mass: 80,
        },
    )
    .expect("64 distinct in-bounds physical cells are a valid body");
    assert_eq!(
        restore(&game),
        game,
        "valid maximum-body checkpoint baseline"
    );

    let mut expected = BTreeSet::new();
    for &[ox, oy, oz] in &body {
        for x in -16_i32..=16 {
            for y in -16_i32..=16 {
                for z in -16_i32..=16 {
                    if x.abs() + y.abs() + z.abs() <= 16 {
                        expected.insert(Position {
                            x: ox + x,
                            y: oy + y,
                            z: oz + z,
                        });
                    }
                }
            }
        }
    }
    assert_eq!(expected.len(), 52_858, "independent integer-union witness");
    let before = game.clone();
    let scene = game.scene(fixtures::OWNER).unwrap();
    assert!(scene.iter().all(|cell| cell.physical));
    assert_eq!(
        scene
            .iter()
            .map(|cell| cell.offset)
            .collect::<BTreeSet<_>>(),
        expected
    );
    assert_eq!(
        scene.len(),
        expected.len(),
        "one result per body-chart offset"
    );
    assert!(scene.len() <= crate::limits::PUBLIC_AIM_OCCURRENCES);
    for offset in &body {
        assert!(game.world.eye_scene_cached(
            point(1, 24 + offset[0], 24 + offset[1], 24 + offset[2]),
            0,
            16
        ));
    }
    let work = crate::diagnostics::work_counts();
    // scene() deliberately recomputes; streaming_scene() consumes its cache.
    assert_eq!(game.streaming_scene(fixtures::OWNER).unwrap(), scene);
    let requested = crate::streaming::RegionTransition {
        active: BTreeSet::from([RegionId(1)]),
        loaded: BTreeSet::from([RegionId(1)]),
    };
    let baseline = game.settle_counted(&requested);
    assert_eq!(baseline.0, requested);
    assert_eq!(baseline.1.actors, 1);
    assert_eq!(baseline.1.reaches, 1);
    assert_eq!(crate::diagnostics::work_counts().scenes, work.scenes);
    assert_eq!(
        game, before,
        "pure queries cannot alter gameplay or random state"
    );
    for id in 2..=129 {
        game.world
            .add_chamber(Region {
                id: RegionId(id),
                name: format!("unrelated {id}"),
                bounds: Extent::new(1, 1, 1).unwrap(),
            })
            .unwrap();
    }
    assert_eq!(
        game.settle_counted(&requested),
        baseline,
        "64-origin view pins and deterministic work do not grow with unrelated regions"
    );
    assert_eq!(game.scene(fixtures::OWNER).unwrap(), scene);
}

#[test]
fn maximum_body_nonvisual_portal_frames_keep_repeated_locations_without_self_overlap() {
    let content = fixture(7, &[]).content().clone();
    let mut world = World::new(vec![], vec![]).unwrap();
    for (id, width, depth) in [(1, 2, 4), (2, 4, 2)] {
        world
            .add_chamber(Region {
                id: RegionId(id),
                name: format!("body loop {id}"),
                bounds: Extent::new(width, depth, 4).unwrap(),
            })
            .unwrap();
        world.set_region_light(RegionId(id), false).unwrap();
    }
    world
        .connect_portal_area(
            Passage {
                from: point(1, 1, 0, 0),
                direction: Direction::East,
                to: point(2, 3, 0, 0),
            },
            1,
            4,
            4,
        )
        .unwrap();
    world
        .connect_portal_area(
            Passage {
                from: point(2, 0, 1, 0),
                direction: Direction::South,
                to: point(1, 0, 3, 0),
            },
            3,
            4,
            4,
        )
        .unwrap();
    let mut game = Game::new(world, 42, content);
    let origin = point(1, 0, 0, 0);
    assert_eq!(
        fixtures::spawn(&mut game, "fixture_perception_owner", origin),
        fixtures::OWNER
    );
    let mut cells = Vec::new();
    for x in 0..4 {
        for y in 0..4 {
            for z in 0..4 {
                cells.push([x, y, z]);
            }
        }
    }
    let mut body = crate::BodySpec {
        cells,
        eye: [0, 0, 0],
        mass: 80,
    };
    game.set_body(fixtures::OWNER, body.clone()).unwrap();
    let occupied = game.body_cells(origin, 0, &body).unwrap();
    assert_eq!(occupied.len(), 64);
    assert_eq!(
        occupied
            .iter()
            .map(|(at, _)| *at)
            .collect::<BTreeSet<_>>()
            .len(),
        64
    );
    assert_eq!(
        occupied
            .iter()
            .map(|(_, frame)| *frame)
            .collect::<BTreeSet<_>>(),
        BTreeSet::from([0, 1]),
        "the body really crosses a rotated physical aperture"
    );
    assert_eq!(
        restore(&game),
        game,
        "valid mixed-frame body checkpoint baseline"
    );
    let scene = game.scene(fixtures::OWNER).unwrap();
    for x in [0, 8] {
        assert!(scene.iter().any(|cell| cell.physical
            && cell.location == origin
            && cell.offset == Position { x, y: 0, z: 0 }
            && cell.rotation == 0));
    }
    assert_eq!(
        scene
            .iter()
            .map(|cell| cell.offset)
            .collect::<BTreeSet<_>>()
            .len(),
        scene.len()
    );
    assert!(scene.len() <= 61_619);
    let requested = crate::streaming::RegionTransition {
        active: BTreeSet::from([RegionId(1)]),
        loaded: BTreeSet::from([RegionId(1)]),
    };
    let (settled, work) = game.settle_counted(&requested);
    assert_eq!(
        settled,
        crate::streaming::RegionTransition {
            active: BTreeSet::from([RegionId(1), RegionId(2)]),
            loaded: BTreeSet::from([RegionId(1), RegionId(2)]),
        }
    );
    assert_eq!(work.actors, 1);
    assert_eq!(work.reaches, 1);
    for &(at, frame) in &occupied {
        assert!(game.world.eye_scene_cached(at, frame, 16));
    }
    let counts = crate::diagnostics::work_counts();
    assert_eq!(game.streaming_scene(fixtures::OWNER).unwrap(), scene);
    assert_eq!(crate::diagnostics::work_counts().scenes, counts.scenes);
    body.cells.reverse();
    game.set_body(fixtures::OWNER, body).unwrap();
    assert_eq!(
        game.scene(fixtures::OWNER).unwrap(),
        scene,
        "authored body-cell order cannot choose a different portal occurrence"
    );
    assert_eq!(restore(&game).scene(fixtures::OWNER).unwrap(), scene);
}

#[test]
fn awareness_range_table_uses_five_foot_cells_and_the_unchanged_sight_ceiling() {
    assert_eq!(tor_world::CELL_SIZE_FEET, 5);
    assert_eq!(SIGHT_RANGE, 16);
    // Independent user-approved table: 100ft is capped by the normal 80ft limit.
    for (awareness, radius) in [
        (0, 1),
        (1, 2),
        (2, 2),
        (3, 4),
        (4, 4),
        (5, 10),
        (6, 10),
        (7, 16),
        (8, 16),
        (9, 16),
        (20, 16),
    ] {
        let mut game = fixture(awareness, &[]);
        assert!(
            locations(&game).contains(&point(1, 36, 2, 0)),
            "valid lit baseline"
        );
        dark(&mut game);
        let disclosed = locations(&game);
        assert!(
            disclosed.contains(&point(1, 20 + radius, 2, 0)),
            "Awareness {awareness} boundary"
        );
        assert!(
            !disclosed.contains(&point(1, 21 + radius, 2, 0)),
            "Awareness {awareness} beyond boundary"
        );
        assert!(
            game.scene_snapshot(fixtures::OWNER).unwrap().1.is_empty(),
            "nonvisual cells are never eye sight"
        );
    }
}

#[test]
fn blindness_and_darkness_share_one_nonvisual_range_without_a_second_penalty() {
    let mut obscured = fixture(3, &[]);
    dark(&mut obscured);
    let expected = locations(&obscured);
    assert!(expected.contains(&point(1, 24, 2, 0)));
    let mut blind = fixture(3, &[]);
    fixtures::apply(&mut blind, fixtures::OWNER, "fixture_blind", 1000);
    assert_eq!(locations(&blind), expected);
    dark(&mut blind);
    assert_eq!(locations(&blind), expected);
    assert!(blind.scene_snapshot(fixtures::OWNER).unwrap().1.is_empty());
}

#[test]
fn awareness_extends_geometry_not_the_unoccluded_neighbourhood_cube() {
    let mut game = fixture(3, &[]);
    dark(&mut game);
    assert!(
        locations(&game).contains(&point(1, 24, 2, 0)),
        "open positive baseline"
    );
    for y in 0..5 {
        for z in 0..3 {
            game.world.set_wall(point(1, 22, y, z), true).unwrap();
        }
    }
    let disclosed = locations(&game);
    assert!(disclosed.contains(&point(1, 22, 2, 0)), "near wall surface");
    assert!(
        !disclosed.contains(&point(1, 23, 2, 0)),
        "cannot perceive through a wall"
    );
    assert!(
        disclosed.contains(&point(1, 21, 3, 1)),
        "legacy immediate diagonal remains"
    );
    assert!(
        !disclosed.contains(&point(1, 21, 4, 2)),
        "range is Manhattan, not an expanded cube"
    );
}

#[test]
fn effective_awareness_modifier_invalidates_warm_scene_and_expiry_restores_it() {
    let mut game = fixture(1, &[]);
    dark(&mut game);
    let baseline = game.streaming_scene(fixtures::OWNER).unwrap();
    assert!(!baseline.iter().any(|c| c.location == point(1, 24, 2, 0)));
    let old_epoch = game.actors.visibility_epoch();
    fixtures::apply(&mut game, fixtures::OWNER, "aware_boost", 100);
    assert_ne!(
        game.actors.visibility_epoch(),
        old_epoch,
        "AI encounter witnesses must invalidate too"
    );
    assert_eq!(
        game.creature(fixtures::OWNER)
            .unwrap()
            .derived()
            .attributes
            .get(Attribute::Awareness),
        3
    );
    assert!(game
        .streaming_scene(fixtures::OWNER)
        .unwrap()
        .iter()
        .any(|c| c.location == point(1, 24, 2, 0)));
    let restored = restore(&game);
    assert_eq!(locations(&restored), locations(&game));
    assert_eq!(restored, game);
    game.advance_physics(100).unwrap();
    assert_eq!(
        game.creature(fixtures::OWNER)
            .unwrap()
            .derived()
            .attributes
            .get(Attribute::Awareness),
        1
    );
    assert_eq!(game.streaming_scene(fixtures::OWNER).unwrap(), baseline);
}

#[test]
fn nonvisual_range_uses_each_body_cell_without_extending_contact() {
    let mut game = fixture(1, &[]);
    game.set_body(
        fixtures::OWNER,
        crate::BodySpec {
            cells: vec![[0, 0, 0], [1, 0, 0]],
            eye: [0, 0, 0],
            mass: 80,
        },
    )
    .unwrap();
    dark(&mut game);
    let disclosed = locations(&game);
    assert!(disclosed.contains(&point(1, 23, 2, 0)));
    assert!(!disclosed.contains(&point(1, 24, 2, 0)));
    assert!(!game.touches_supply(fixtures::OWNER, point(1, 23, 2, 0)));
}

#[test]
fn nonvisual_projection_preserves_guise_invisibility_and_secret_authority() {
    for (values, expected) in [
        (vec![], ActorDisclosure::Creature),
        (vec![fixtures::guise()], ActorDisclosure::ApparentObject),
        (
            vec![PerceptionValue::Invisible { enabled: true }],
            ActorDisclosure::Hidden,
        ),
    ] {
        let mut game = fixture(5, &values);
        dark(&mut game);
        let door = point(1, 24, 3, 0);
        assert_eq!(game.place_authored_door(77, door, false, 1).unwrap(), 77);
        game.conceal_door(door, Concealment { dc: 0 }).unwrap();
        assert!(
            game.world.checkpoint_valid(game.next_door_id),
            "the authored door identity must be below the saved high-water mark"
        );
        assert_eq!(restore(&game), game, "valid concealed-door baseline");
        let before = game.actors[&fixtures::OWNER].discovery.clone();
        let rng = game.rules_rng;
        let view = game.observe(fixtures::OWNER).unwrap();
        assert_eq!(
            game.classified_actors(fixtures::OWNER)
                .unwrap()
                .get(&fixtures::SUBJECT)
                .map_or(ActorDisclosure::Hidden, |a| a.disclosure),
            expected
        );
        assert_eq!(
            view.visible_actors
                .iter()
                .any(|a| a.id == fixtures::SUBJECT),
            expected == ActorDisclosure::Creature
        );
        assert_eq!(
            view.apparent_objects
                .iter()
                .any(|a| a.subject == fixtures::SUBJECT),
            expected == ActorDisclosure::ApparentObject
        );
        assert!(game.concealed_door_wall(fixtures::OWNER, door));
        assert!(!game.genuinely_sees_actor(fixtures::OWNER, fixtures::SUBJECT, 16));
        assert!(game
            .ability_plan(
                fixtures::OWNER,
                &fixtures::id("fixture_sight"),
                fixtures::SUBJECT
            )
            .is_err());
        game.reconcile_passive_discovery_for(fixtures::OWNER)
            .unwrap();
        assert_eq!(game.actors[&fixtures::OWNER].discovery, before);
        assert_eq!(game.rules_rng, rng);
        let restored = restore(&game);
        assert_eq!(restored.observe(fixtures::OWNER).unwrap(), view);
    }
}

#[test]
fn nonvisual_physical_portal_preserves_rotation_and_abstract_stairs_do_not_extend_it() {
    let mut game = fixture(3, &[]);
    game.teleport(fixtures::OWNER, point(1, 43, 2, 0)).unwrap();
    game.world
        .connect(
            Passage {
                from: point(1, 44, 2, 0),
                direction: Direction::East,
                to: point(2, 20, 0, 0),
            },
            1,
        )
        .unwrap();
    dark(&mut game);
    let scene = game.scene(fixtures::OWNER).unwrap();
    assert!(scene.iter().any(|c| c.location == point(2, 20, 2, 0)
        && c.offset == Position { x: 4, y: 0, z: 0 }
        && c.rotation == 1));
    assert!(!scene.iter().any(|c| c.location == point(2, 20, 3, 0)));
    let landing = point(2, 30, 2, 0);
    game.register_named_anchors(
        RegionId(2),
        std::collections::BTreeMap::from([("landing".into(), landing.position)]),
    )
    .unwrap();
    game.connect_named_stair(
        point(1, 43, 2, 0),
        Direction::Up,
        tor_world::NamedAnchor {
            region: RegionId(2),
            name: "landing".into(),
        },
    )
    .unwrap();
    let scene = game.scene(fixtures::OWNER).unwrap();
    assert!(scene.iter().any(|c| c.location == landing && !c.physical));
    assert!(!scene.iter().any(|c| c.location == point(2, 31, 2, 0)));
}

#[test]
fn worn_attribute_source_and_restorable_loss_drive_current_nonvisual_range() {
    let mut game = fixture(1, &[]);
    dark(&mut game);
    let base = locations(&game);
    let mut spec = crate::ItemSpec::ordinary("awareness fixture ring".into());
    spec.class = crate::ItemClass::Ring;
    spec.equipment = Some(crate::EquipmentSpec {
        periodic_relocation: None,
        slot: crate::EquipmentSlot::new("ring").unwrap(),
        armor: None,
        attack: None,
        weapon_profile: None,
        modifiers: vec![crate::rules::Modifier {
            scope: crate::rules::Scope::Always,
            op: crate::rules::ModifierOp::Add {
                stat: crate::rules::Stat::Attribute {
                    attribute: Attribute::Awareness,
                },
                value: 2,
            },
        }],
        potency_modifiers: vec![],
    });
    let ring = game
        .place_item_stack(91, point(1, 20, 2, 0), Some(fixtures::OWNER), 1, spec)
        .unwrap();
    let socket = game
        .anatomy(fixtures::OWNER)
        .unwrap()
        .slots
        .iter()
        .position(|slot| slot.as_str() == "ring")
        .unwrap();
    game.equip_authored(fixtures::OWNER, ring, crate::EquipmentSlotId(socket as u16))
        .unwrap();
    assert!(locations(&game).contains(&point(1, 24, 2, 0)));
    game.apply_effects(
        fixtures::OWNER,
        &[crate::rules::Effect::LoseAttribute {
            attribute: Attribute::Awareness,
            amount: crate::rules::AmountExpr::Fixed { amount: 2 },
        }],
    )
    .unwrap();
    assert_eq!(locations(&game), base);
    game.apply_effects(
        fixtures::OWNER,
        &[crate::rules::Effect::RestoreAttribute {
            attribute: Attribute::Awareness,
            amount: crate::rules::AmountExpr::Fixed { amount: 2 },
        }],
    )
    .unwrap();
    assert!(locations(&game).contains(&point(1, 24, 2, 0)));
    assert_eq!(locations(&restore(&game)), locations(&game));
}

#[test]
fn awareness_scene_cache_is_branch_safe_and_queries_neither_roll_nor_teach() {
    let mut original = fixture(1, &[]);
    dark(&mut original);
    let before = original.clone();
    let original_scene = original.streaming_scene(fixtures::OWNER).unwrap();
    let mut candidate = original.clone();
    fixtures::apply(&mut candidate, fixtures::OWNER, "aware_boost", 100);
    let changed = candidate.streaming_scene(fixtures::OWNER).unwrap();
    assert_ne!(changed, original_scene);
    for _ in 0..3 {
        assert_eq!(
            original.streaming_scene(fixtures::OWNER).unwrap(),
            original_scene
        );
        assert_eq!(candidate.streaming_scene(fixtures::OWNER).unwrap(), changed);
    }
    assert_eq!(
        original, before,
        "derived cache queries do not mutate authoritative state"
    );
    assert_eq!(candidate.rules_rng, original.rules_rng);
}

#[test]
fn nonvisual_vertical_geometry_and_streaming_use_the_same_physical_boundary() {
    let mut game = fixture(3, &[]);
    game.world
        .connect_portal_area(
            Passage {
                from: point(1, 20, 2, 2),
                direction: Direction::Up,
                to: point(2, 20, 2, 0),
            },
            0,
            1,
            1,
        )
        .unwrap();
    assert!(
        !game.world.is_stair(point(1, 20, 2, 2), Direction::Up),
        "this witness requires a physical aperture, not an abstract stair"
    );
    assert_eq!(
        game.world
            .physics_neighbor(point(1, 20, 2, 2), Direction::Up),
        Some((point(2, 20, 2, 0), 0))
    );
    assert_eq!(
        restore(&game),
        game,
        "valid vertical-portal checkpoint baseline"
    );
    dark(&mut game);
    let scene = game.scene(fixtures::OWNER).unwrap();
    assert!(scene
        .iter()
        .any(|c| c.location == point(2, 20, 2, 1) && c.offset == Position { x: 0, y: 0, z: 4 }));
    assert!(!scene.iter().any(|c| c.location == point(2, 20, 2, 2)));
    game.add_reference_point(crate::streaming::ReferencePoint {
        target: crate::streaming::ReferenceTarget::Actor(fixtures::OWNER),
        active_radius: Some(0),
        load_radius: Some(0),
        observes: true,
    })
    .unwrap();
    let requirement = game.region_requirements(
        &BTreeSet::from([RegionId(1)]),
        &BTreeSet::from([RegionId(1)]),
    );
    assert!(
        requirement.active.contains(&RegionId(2)),
        "disclosed physical geometry is active"
    );
    assert!(requirement.loaded.contains(&RegionId(2)));
    game.world.set_wall(point(1, 20, 2, 1), true).unwrap();
    assert!(
        !locations(&game).iter().any(|at| at.region == RegionId(2)),
        "opaque vertical boundary is not a stair projection"
    );
}

#[test]
fn extended_body_awareness_never_collides_with_the_abstract_stair_occurrence() {
    let mut game = fixture(7, &[]);
    let mut world = World::new(vec![], vec![]).unwrap();
    for region in 1..=2 {
        world
            .add_chamber(Region {
                id: RegionId(region),
                name: format!("tall awareness {region}"),
                bounds: Extent::new(45, 5, 40).unwrap(),
            })
            .unwrap();
    }
    game.world = tor_world::Shared::new(world);
    game.set_body(
        fixtures::OWNER,
        crate::BodySpec {
            cells: vec![[0, 0, 0], [0, 0, 1], [0, 0, 2], [0, 0, 3]],
            eye: [0, 0, 0],
            mass: 80,
        },
    )
    .unwrap();
    dark(&mut game);
    let landing = point(2, 30, 2, 0);
    game.register_named_anchors(
        RegionId(2),
        std::collections::BTreeMap::from([("landing".into(), landing.position)]),
    )
    .unwrap();
    game.connect_named_stair(
        point(1, 20, 2, 0),
        Direction::Up,
        tor_world::NamedAnchor {
            region: RegionId(2),
            name: "landing".into(),
        },
    )
    .unwrap();
    let scene = game.scene(fixtures::OWNER).unwrap();
    assert!(scene
        .iter()
        .any(|c| c.physical && c.location == point(1, 20, 2, 19)));
    let projected = scene
        .iter()
        .find(|c| !c.physical && c.location == landing)
        .unwrap();
    assert_eq!(projected.offset, Position { x: 0, y: 0, z: 20 });
    assert!(scene
        .iter()
        .filter(|c| c.physical)
        .all(|c| c.offset != projected.offset));
}
