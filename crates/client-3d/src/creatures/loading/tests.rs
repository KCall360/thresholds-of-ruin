use super::*;
use std::time::{Duration, Instant};

fn world() -> World {
    let mut world = World::new();
    let mut materials = Assets::<StandardMaterial>::default();
    world.insert_resource(PropLibrary::new(crate::props::PropMaterials::new(
        &mut materials,
    )));
    world.insert_resource(materials);
    world.init_resource::<Assets<Mesh>>();
    world.init_resource::<crate::Game>();
    world
}

fn request_in(world: &mut World, entity: Entity, look: &Look) {
    let mut state =
        SystemState::<(Commands, ResMut<PropLibrary>, ResMut<Assets<Mesh>>)>::new(world);
    let (mut commands, mut library, mut meshes) = state.get_mut(world);
    let rig = request(&mut commands, &mut library, &mut meshes, entity, look);
    commands.entity(entity).insert(rig);
    state.apply(world);
}

fn finish(world: &mut World) {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        poll(world);
        if world.query::<&PendingModel>().iter(world).next().is_none()
            && world.resource::<ModelLoading>().task.is_none()
        {
            break;
        }
        assert!(Instant::now() < deadline, "worker did not complete");
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(world.query::<&LoadFailed>().iter(world).count(), 0);
}

fn generator(world: &World) -> Generator {
    Generator::new(world.resource::<PropLibrary>().materials.clone())
}

#[test]
fn worker_collection_drains_asset_bookkeeping_and_preserves_cached_sources() {
    use bevy::asset::AssetEvent;
    let world = world();
    let mut generator = generator(&world);
    let cached = generator
        .library
        .get("retained".into(), &mut generator.meshes, || {
            let mut parts = Parts::default();
            parts.matte.tri([Vec3::ZERO, Vec3::X, Vec3::Y], Vec3::ONE);
            parts
        });
    let obsolete = generator.meshes.add(Mesh::from(Cuboid::default()));
    let obsolete_id = obsolete.id();
    drop(obsolete);
    generator.collect();
    assert!(generator.meshes.contains(cached.0[0].0.id()));
    assert!(!generator.meshes.contains(obsolete_id));
    generator
        .maintenance
        .insert_resource(std::mem::take(&mut generator.meshes));
    generator.maintenance.run_schedule(PostUpdate);
    assert!(
        generator
            .maintenance
            .resource::<Events<AssetEvent<Mesh>>>()
            .is_empty(),
        "worker asset changes must already have been drained"
    );
    let positive = generator
        .maintenance
        .resource_mut::<Assets<Mesh>>()
        .add(Mesh::from(Cuboid::default()));
    generator.maintenance.run_schedule(PostUpdate);
    assert!(
        !generator
            .maintenance
            .resource::<Events<AssetEvent<Mesh>>>()
            .is_empty(),
        "positive control must expose undrained asset changes"
    );
    drop(positive);
}

#[test]
fn a_cold_request_immediately_shows_the_egg_without_building_the_creature() {
    let mut world = world();
    let parent = world.spawn_empty().id();
    request_in(&mut world, parent, &Look::of(CreatureModel::Human, 17));
    let rig = world.get::<Rig>(parent).unwrap();
    assert_eq!(
        rig.plan,
        Plan::Marker,
        "the client must return an egg before generating the human"
    );
    assert!(rig.joint(J::Head).is_none());
    assert!(world.get::<PendingModel>(parent).is_some());
    assert!(
        world.resource::<Assets<Mesh>>().len() < 3,
        "only the cheap egg is built on the client"
    );
    let egg = rig.joint(J::Lift).unwrap();
    finish(&mut world);
    assert_eq!(world.get::<Rig>(parent).unwrap().plan, Plan::Biped);
    assert!(!world.entities().contains(egg));
    assert!(world.get::<Rig>(parent).unwrap().joint(J::Head).is_some());
}

#[test]
fn an_unfinished_job_does_not_block_updates_or_start_another_crowd_job() {
    let mut world = world();
    let pool = bevy::tasks::TaskPool::new();
    world.insert_resource(ModelLoading {
        generator: None,
        task: Some(
            pool.spawn(async { bevy::tasks::futures_lite::future::pending::<Finished>().await }),
        ),
    });
    for _ in 0..101 {
        let e = world.spawn_empty().id();
        request_in(&mut world, e, &Look::of(CreatureModel::Human, 17));
    }
    for step in 0..20 {
        poll(&mut world);
        world.resource_mut::<crate::Game>().status = format!("input update {step}");
    }
    assert_eq!(world.resource::<crate::Game>().status, "input update 19");
    assert_eq!(world.query::<&PendingModel>().iter(&world).count(), 101);
    assert_eq!(
        world.resource::<Assets<Mesh>>().len(),
        1,
        "eggs share a single mesh"
    );
    assert!(
        world.resource::<ModelLoading>().generator.is_none(),
        "the busy worker was not replaced"
    );
}

#[test]
fn disappearance_and_replacement_discard_the_old_completion() {
    for replace in [false, true] {
        let mut world = world();
        let parent = world.spawn_empty().id();
        request_in(&mut world, parent, &Look::of(CreatureModel::Human, 17));
        let egg = world.get::<PendingModel>(parent).unwrap().egg;
        // Construct an already completed, obsolete job independently of timing.
        let mut generator = generator(&world);
        let prepared = generator.prepare(&Look::of(CreatureModel::Human, 17));
        let pool = bevy::tasks::TaskPool::new();
        world.insert_resource(ModelLoading {
            generator: None,
            task: Some(pool.spawn(async move {
                Finished {
                    generator,
                    entity: parent,
                    egg,
                    result: Ok(prepared),
                }
            })),
        });
        world.entity_mut(parent).despawn();
        let replacement = replace.then(|| {
            let e = world.spawn_empty().id();
            request_in(&mut world, e, &Look::of(CreatureModel::Emu, 3));
            e
        });
        finish(&mut world);
        assert!(
            !world.entities().contains(parent),
            "late completion resurrected the old actor"
        );
        if let Some(entity) = replacement {
            assert_eq!(world.get::<Rig>(entity).unwrap().plan, Plan::Bird);
        } else {
            assert_eq!(
                world.entities().len(),
                0,
                "abandoned geometry was installed"
            );
        }
    }
}

#[test]
fn readiness_keeps_current_position_animation_and_unrelated_children() {
    let mut world = world();
    let parent = world
        .spawn((
            Transform::from_xyz(4.0, 2.0, 7.0),
            Anim::new(17, Vec3::ZERO),
        ))
        .id();
    let ring = world.spawn_empty().id();
    world.entity_mut(parent).add_child(ring);
    world
        .resource_mut::<crate::Game>()
        .actors
        .insert(tor_protocol::ActorTarget::from_digest([7; 32]), parent);
    request_in(&mut world, parent, &Look::of(CreatureModel::Human, 17));
    world.get_mut::<Anim>(parent).unwrap().act(Move::Jump);
    world.get_mut::<Transform>(parent).unwrap().translation = Vec3::new(9.0, 3.0, 2.0);
    finish(&mut world);
    assert_eq!(
        world.get::<Transform>(parent).unwrap().translation,
        Vec3::new(9.0, 3.0, 2.0)
    );
    assert!(matches!(
        world.get::<Anim>(parent).unwrap().action,
        Some((Move::Jump, _))
    ));
    assert_eq!(world.get::<ChildOf>(ring).unwrap().parent(), parent);
    let hand = world.get::<Rig>(parent).unwrap().joint(J::HandR).unwrap();
    assert_eq!(
        world.resource::<crate::Game>().hands.get(&parent),
        Some(&hand)
    );
}

#[test]
fn failed_generation_keeps_the_egg_and_does_not_poison_the_next_request() {
    let mut world = world();
    let parent = world.spawn_empty().id();
    request_in(&mut world, parent, &Look::of(CreatureModel::Human, 17));
    let egg = world.get::<PendingModel>(parent).unwrap().egg;
    let generator = generator(&world);
    let pool = bevy::tasks::TaskPool::new();
    world.insert_resource(ModelLoading {
        generator: None,
        task: Some(pool.spawn(async move {
            Finished {
                generator,
                entity: parent,
                egg,
                result: Err("injected preparation failure".into()),
            }
        })),
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    while world.get::<LoadFailed>(parent).is_none() {
        poll(&mut world);
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(world.get::<Rig>(parent).unwrap().plan, Plan::Marker);
    assert!(world.entities().contains(egg));
    world.entity_mut(parent).despawn();
    let next = world.spawn_empty().id();
    request_in(&mut world, next, &Look::of(CreatureModel::Emu, 3));
    finish(&mut world);
    assert_eq!(world.get::<Rig>(next).unwrap().plan, Plan::Bird);
}

fn component_inventory(
    world: &World,
    entities: &[Entity],
) -> std::collections::BTreeMap<String, usize> {
    let mut result = std::collections::BTreeMap::new();
    for &entity in entities {
        for id in world.entity(entity).archetype().components() {
            let name = world.components().get_info(id).unwrap().name().to_string();
            *result.entry(name).or_default() += 1;
        }
    }
    result
}

#[test]
fn transfer_preserves_every_generated_component_and_deformation_reference() {
    let mut world = world();
    // Occupy the target IDs so accidentally retaining scratch-world entity IDs
    // cannot pass by coincidence.
    for _ in 0..600 {
        world.spawn_empty();
    }
    let mut generator = generator(&world);
    let mut witnessed = [false; 4];
    for look in [
        Look::with_choice(
            CreatureModel::Human,
            7,
            Some(Variant {
                woman: true,
                bust_lean: 1,
                ..default()
            }),
        )
        .dressed(Wardrobe::Leotard),
        Look::preview_adjusted(CreatureModel::Human, 17, |v| {
            v.woman = true;
            v.hair = 4;
        })
        .dressed(Wardrobe::Courtier),
        Look::of(CreatureModel::Mage, 1),
        Look::of(CreatureModel::Snake, 3),
        Look::of(CreatureModel::Wisp, 3),
    ] {
        let prepared = generator.prepare(&look);
        let source_ids: Vec<_> = prepared.world.iter_entities().map(|e| e.id()).collect();
        let expected = component_inventory(&prepared.world, &source_ids);
        let source_rig = prepared.world.get::<Rig>(prepared.root).unwrap();
        witnessed[0] |= source_rig.hair.is_some();
        witnessed[1] |= source_rig.robe.is_some();
        witnessed[2] |= !source_rig.cloaks.is_empty();
        witnessed[3] |= expected.keys().any(|name| name.ends_with("PointLight"));
        let expected_meshes: Vec<_> = source_ids
            .iter()
            .filter_map(|id| prepared.world.get::<Mesh3d>(*id))
            .map(|mesh| {
                let data = &prepared
                    .meshes
                    .iter()
                    .find(|(id, _)| *id == mesh.0.id())
                    .expect("exported source or private payload")
                    .1;
                (data.count_vertices(), data.indices().unwrap().len())
            })
            .collect();
        let parent = world
            .spawn((Transform::default(), Visibility::default()))
            .id();
        let rig = prepared.install(&mut world, parent);
        world.entity_mut(parent).insert(rig);
        let mut ids = vec![parent];
        let mut next = 0;
        while next < ids.len() {
            if let Some(children) = world.get::<Children>(ids[next]) {
                ids.extend(children.iter());
            }
            next += 1;
        }
        assert_eq!(
            component_inventory(&world, &ids),
            expected,
            "a generated component was lost: {}",
            look.key
        );
        let mut actual_meshes: Vec<_> = ids
            .iter()
            .filter_map(|id| world.get::<Mesh3d>(*id))
            .map(|mesh| {
                let data = world
                    .resource::<Assets<Mesh>>()
                    .get(&mesh.0)
                    .expect("published render mesh");
                (data.count_vertices(), data.indices().unwrap().len())
            })
            .collect();
        let mut expected_meshes = expected_meshes;
        actual_meshes.sort();
        expected_meshes.sort();
        assert_eq!(actual_meshes, expected_meshes);
        for &id in &ids {
            if let Some(skin) = world.get::<SkinnedMesh>(id) {
                assert!(
                    skin.joints.iter().all(|joint| ids.contains(joint)),
                    "skin references a scratch entity"
                );
                assert!(world
                    .resource::<Assets<SkinnedMeshInverseBindposes>>()
                    .get(&skin.inverse_bindposes)
                    .is_some());
            }
        }
        let mut rig = world.entity_mut(parent).take::<Rig>().unwrap();
        rig.map_render_references(
            |entity| {
                assert!(ids.contains(&entity));
                entity
            },
            |handle| {
                assert!(world.resource::<Assets<Mesh>>().get(handle).is_some());
                handle.clone()
            },
        );
        world.entity_mut(parent).insert(rig);
    }
    assert_eq!(
        witnessed, [true; 4],
        "hair, robe, cloak and light positive controls must all be generated"
    );
}

#[test]
fn repeated_preparation_reuses_sources_but_keeps_deforming_meshes_private() {
    let mut world = world();
    let mut generator = generator(&world);
    let look = Look::of(CreatureModel::Mage, 1);
    let mut source_meshes = Vec::new();
    let mut robes = Vec::new();
    for _ in 0..2 {
        let prepared = generator.prepare(&look);
        let root = world.spawn_empty().id();
        let rig = prepared.install(&mut world, root);
        let head = rig.joint(J::Head).unwrap();
        let mesh = world
            .get::<Children>(head)
            .unwrap()
            .iter()
            .find_map(|e| world.get::<Mesh3d>(e))
            .unwrap();
        source_meshes.push(mesh.0.id());
        // The robe helper exposes its handle through the same remapper used by
        // transfer; the last mesh here is not assumed to be the robe.
        let mut robe_ids = Vec::new();
        let mut rig = rig;
        rig.robe.as_mut().expect("Mage garment").map_mesh(&mut |h| {
            robe_ids.push(h.id());
            h.clone()
        });
        robes.push(robe_ids[0]);
        world.entity_mut(root).insert(rig);
    }
    assert_eq!(
        source_meshes[0], source_meshes[1],
        "immutable source meshes are shared"
    );
    assert_ne!(robes[0], robes[1], "each wearer owns its deforming garment");
}

#[test]
fn a_model_finishing_after_death_keeps_death_state_and_registers_no_live_hand() {
    let mut world = world();
    let parent = world.spawn(Anim::corpse(17)).id();
    let dead = world.get::<Anim>(parent).unwrap().dead;
    request_in(&mut world, parent, &Look::of(CreatureModel::Human, 17));
    world.entity_mut(parent).insert(Snuffed::default());
    finish(&mut world);
    assert_eq!(world.get::<Anim>(parent).unwrap().dead, dead);
    assert!(
        world.get::<Snuffed>(parent).is_none(),
        "new glowing descendants must be snuffed again"
    );
    assert!(
        !world.resource::<crate::Game>().hands.contains_key(&parent),
        "a corpse must not acquire a live weapon grip"
    );
}

#[test]
fn a_ready_model_replaces_its_egg_on_the_next_poll_while_others_are_queued() {
    let mut world = world();
    let first = world.spawn_empty().id();
    let second = world.spawn_empty().id();
    let look = Look::of(CreatureModel::Human, 17);
    request_in(&mut world, first, &look);
    request_in(&mut world, second, &look);
    let egg = world.get::<PendingModel>(first).unwrap().egg;
    let mut generator = generator(&world);
    let prepared = generator.prepare(&look);
    let pool = bevy::tasks::TaskPool::new();
    let task = pool.spawn(async move {
        Finished {
            generator,
            entity: first,
            egg,
            result: Ok(prepared),
        }
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    while !task.is_finished() {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
    world.insert_resource(ModelLoading {
        generator: None,
        task: Some(task),
    });
    poll(&mut world);
    assert!(world.get::<PendingModel>(first).is_none());
    assert_eq!(world.get::<Rig>(first).unwrap().plan, Plan::Biped);
    assert!(!world.entities().contains(egg));
    assert!(
        world.get::<PendingModel>(second).is_some(),
        "ready creatures must not wait for the whole queue"
    );
    assert!(
        world.resource::<ModelLoading>().task.is_some(),
        "the next creature can already prepare"
    );
}
