//! Prepare live creatures without holding up the client update.
use super::*;
use bevy::{
    asset::AssetId,
    ecs::system::SystemState,
    render::{
        mesh::skinning::{SkinnedMesh, SkinnedMeshInverseBindposes},
        primitives::Aabb,
    },
    tasks::{AsyncComputeTaskPool, Task},
};
use std::collections::{HashMap, HashSet};

#[derive(Component)]
pub(crate) struct PendingModel {
    look: Look,
    egg: Entity,
}

#[derive(Component)]
pub(crate) struct LoadFailed;

#[derive(Resource, Default)]
pub(crate) struct ModelLoading {
    generator: Option<Generator>,
    task: Option<Task<Finished>>,
}

#[cfg(test)]
impl ModelLoading {
    pub(crate) fn blocked_for_tests() -> Self {
        let pool = AsyncComputeTaskPool::get_or_init(bevy::tasks::TaskPool::default);
        Self {
            generator: None,
            task: Some(pool.spawn(bevy::tasks::futures_lite::future::pending())),
        }
    }
}

struct Generator {
    library: PropLibrary,
    meshes: Assets<Mesh>,
    maintenance: World,
}

struct Finished {
    generator: Generator,
    entity: Entity,
    egg: Entity,
    result: Result<Prepared, String>,
}

struct Prepared {
    world: World,
    root: Entity,
    meshes: Vec<(AssetId<Mesh>, Mesh)>,
    models: Vec<(String, Model)>,
}

pub(crate) fn request(
    commands: &mut Commands,
    library: &mut PropLibrary,
    meshes: &mut Assets<Mesh>,
    parent: Entity,
    look: &Look,
) -> Rig {
    let mut egg = Look::of(CreatureModel::Placeholder, 0);
    egg.scale = look.scale;
    let rig = spawn(commands, library, meshes, parent, &egg);
    commands.entity(parent).insert(PendingModel {
        look: look.clone(),
        egg: rig.joint(J::Lift).expect("stand-in root"),
    });
    rig
}

impl Generator {
    fn new(materials: crate::props::PropMaterials) -> Self {
        use bevy::asset::{AssetApp, AssetPlugin};
        let mut app = App::new();
        app.add_plugins(AssetPlugin {
            watch_for_changes_override: Some(false),
            ..default()
        })
        .init_asset::<Mesh>();
        let meshes = app
            .world_mut()
            .remove_resource::<Assets<Mesh>>()
            .expect("worker meshes");
        Self {
            library: PropLibrary::new(materials),
            meshes,
            maintenance: std::mem::take(app.world_mut()),
        }
    }

    fn collect(&mut self) {
        self.library.collect_worker_meshes(&mut self.meshes);
        // Assets also queue change/drop bookkeeping. Run Bevy's own maintenance
        // and drain its events; manually removing mesh payloads is insufficient.
        self.maintenance
            .insert_resource(std::mem::take(&mut self.meshes));
        self.maintenance.run_schedule(PreUpdate);
        self.maintenance.run_schedule(PostUpdate);
        self.maintenance
            .resource_mut::<Events<bevy::asset::AssetEvent<Mesh>>>()
            .clear();
        self.meshes = self
            .maintenance
            .remove_resource::<Assets<Mesh>>()
            .expect("collected worker meshes");
    }

    fn prepare(&mut self, look: &Look) -> Prepared {
        let mut world = World::new();
        let root = world
            .spawn((Transform::default(), Visibility::default()))
            .id();
        let mut state = SystemState::<Commands>::new(&mut world);
        let rig = spawn(
            &mut state.get_mut(&mut world),
            &mut self.library,
            &mut self.meshes,
            root,
            look,
        );
        state.apply(&mut world);
        let mut used: HashSet<_> = world
            .query::<&Mesh3d>()
            .iter(&world)
            .map(|mesh| mesh.0.id())
            .collect();
        let mut rig = rig;
        rig.map_render_references(
            |e| e,
            |h| {
                used.insert(h.id());
                h.clone()
            },
        );
        world.entity_mut(root).insert(rig);
        let models = self.library.prepared_models(&used);
        for (_, model) in &models {
            used.extend(model.0.iter().map(|(h, _)| h.id()));
        }
        let meshes = used
            .into_iter()
            .map(|id| (id, self.meshes.get(id).expect("prepared mesh").clone()))
            .collect();
        let prepared = Prepared {
            world,
            root,
            meshes,
            models,
        };
        self.collect();
        prepared
    }
}

impl Prepared {
    fn install(mut self, world: &mut World, parent: Entity) -> Rig {
        let meshes = world.resource_scope(|world, mut library: Mut<PropLibrary>| {
            library.import_prepared(
                &mut world.resource_mut::<Assets<Mesh>>(),
                self.models,
                self.meshes,
            )
        });
        let entities: HashMap<_, _> = self
            .world
            .iter_entities()
            .map(|source| {
                (
                    source.id(),
                    if source.id() == self.root {
                        parent
                    } else {
                        world.spawn_empty().id()
                    },
                )
            })
            .collect();
        let map = |id: Entity| entities[&id];
        let mut inverse_poses = HashMap::new();
        let sources: Vec<_> = entities.keys().copied().collect();
        for source in sources {
            if source == self.root {
                continue;
            }
            let target = map(source);
            let parent = self.world.get::<ChildOf>(source).map(|p| map(p.parent()));
            let mut node = self.world.entity_mut(source);
            // This is the render payload produced by geometry::spawn. Required
            // transforms/visibility components are recreated by Bevy on insert.
            macro_rules! transfer {
                ($component:ty) => {
                    if let Some(value) = node.take::<$component>() {
                        world.entity_mut(target).insert(value);
                    }
                };
            }
            transfer!(Transform);
            transfer!(Visibility);
            transfer!(MeshMaterial3d<StandardMaterial>);
            transfer!(crate::props::SoftBone);
            transfer!(Aabb);
            transfer!(crate::picking::LooseBounds);
            transfer!(BodySurface);
            transfer!(PointLight);
            if let Some(mesh) = node.take::<Mesh3d>() {
                world
                    .entity_mut(target)
                    .insert(Mesh3d(meshes[&mesh.0.id()].clone()));
            }
            if let Some(mut skin) = node.take::<SkinnedMesh>() {
                let id = skin.inverse_bindposes.id();
                let inverse = inverse_poses.entry(id).or_insert_with(|| {
                    let poses = self
                        .world
                        .resource::<Assets<SkinnedMeshInverseBindposes>>()
                        .get(id)
                        .expect("prepared bind poses")
                        .to_vec();
                    world
                        .get_resource_or_init::<Assets<SkinnedMeshInverseBindposes>>()
                        .add(poses)
                });
                skin.inverse_bindposes = inverse.clone();
                for joint in &mut skin.joints {
                    *joint = map(*joint);
                }
                world.entity_mut(target).insert(skin);
            }
            if let Some(parent) = parent {
                world.entity_mut(parent).add_child(target);
            }
        }
        let mut rig = self
            .world
            .entity_mut(self.root)
            .take::<Rig>()
            .expect("prepared rig");
        rig.map_render_references(map, |h| meshes[&h.id()].clone());
        rig
    }
}

/// Poll once; never wait for geometry. One active job bounds CPU contention and
/// completed-result memory, including when an entire crowd enters the view.
pub(crate) fn poll(world: &mut World) {
    let mut loading = world.remove_resource::<ModelLoading>().unwrap_or_default();
    if let Some(task) = &mut loading.task {
        if let Some(finished) =
            bevy::tasks::block_on(bevy::tasks::futures_lite::future::poll_once(task))
        {
            loading.task = None;
            loading.generator = Some(finished.generator);
            let current = world
                .get::<PendingModel>(finished.entity)
                .is_some_and(|p| p.egg == finished.egg);
            if current {
                match finished.result {
                    Ok(prepared) => {
                        let rig = prepared.install(world, finished.entity);
                        let hand = rig.joint(J::HandR);
                        world.entity_mut(finished.egg).despawn();
                        world
                            .entity_mut(finished.entity)
                            .remove::<(PendingModel, LoadFailed, Snuffed)>()
                            .insert(rig);
                        if let Some(hand) = hand.filter(|_| {
                            world
                                .resource::<crate::Game>()
                                .actors
                                .values()
                                .any(|&e| e == finished.entity)
                        }) {
                            world
                                .resource_mut::<crate::Game>()
                                .hands
                                .insert(finished.entity, hand);
                            world
                                .run_system_cached(crate::scene::refresh_held)
                                .expect("ready-hand reconciliation");
                        }
                    }
                    Err(message) => {
                        error!("Creature model preparation failed: {message}");
                        world
                            .entity_mut(finished.entity)
                            .remove::<PendingModel>()
                            .insert(LoadFailed);
                    }
                }
            }
        }
    }
    if loading.task.is_none() {
        let next = world
            .query::<(Entity, &PendingModel)>()
            .iter(world)
            .min_by_key(|(e, _)| e.to_bits())
            .map(|(e, p)| (e, p.egg, p.look.clone()));
        if let Some((entity, egg, look)) = next {
            let generator = loading.generator.take();
            let materials = world.resource::<PropLibrary>().materials.clone();
            let pool = AsyncComputeTaskPool::get_or_init(bevy::tasks::TaskPool::default);
            loading.task = Some(pool.spawn(async move {
                let mut generator = generator.unwrap_or_else(|| Generator::new(materials));
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    generator.prepare(&look)
                }))
                .map_err(|panic| {
                    panic
                        .downcast_ref::<String>()
                        .cloned()
                        .or_else(|| panic.downcast_ref::<&str>().map(|s| s.to_string()))
                        .unwrap_or_else(|| "unknown worker panic".into())
                });
                if result.is_err() {
                    generator.collect();
                }
                Finished {
                    generator,
                    entity,
                    egg,
                    result,
                }
            }));
        }
    }
    world.insert_resource(loading);
}

#[cfg(test)]
mod tests;
