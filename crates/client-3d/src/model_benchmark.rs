//! CPU-only, bounded construction workload for the shared performance harness.
use crate::{
    creatures::{self, Equipment, Look, Wardrobe},
    props::{PropLibrary, PropMaterials},
};
use bevy::prelude::*;
use serde_json::json;
use std::{
    io::{self, Write},
    time::{Duration, Instant},
};
use tor_client_3d_model::CreatureModel;

fn spawn(look: &Look, library: &mut PropLibrary, meshes: &mut Assets<Mesh>) -> World {
    let mut world = World::new();
    let parent = world.spawn(Transform::default()).id();
    let mut queue = bevy::ecs::world::CommandQueue::default();
    let rig = creatures::spawn(
        &mut Commands::new(&mut queue, &world),
        library,
        meshes,
        parent,
        look,
    );
    queue.apply(&mut world);
    world.entity_mut(parent).insert(rig);
    world
}

fn payload(meshes: &Assets<Mesh>) -> usize {
    meshes
        .iter()
        .map(|(_, mesh)| {
            mesh.get_vertex_buffer_size() + mesh.get_index_buffer_bytes().map_or(0, <[u8]>::len)
        })
        .sum()
}

struct MotionSample {
    times: Vec<f64>,
    phases: [Vec<f64>; 4],
    hair_phases: [Vec<f64>; 4],
    changed: usize,
}

fn motion(mut world: World, meshes: Assets<Mesh>) -> MotionSample {
    let root = world
        .query_filtered::<Entity, With<creatures::Rig>>()
        .single(&world)
        .unwrap();
    let lift = world
        .get::<creatures::Rig>(root)
        .unwrap()
        .joint(creatures::J::Lift)
        .unwrap();
    world
        .entity_mut(root)
        .insert(creatures::Anim::new(7, Vec3::ZERO));
    world.insert_resource(meshes);
    world.insert_resource(Time::<()>::default());
    world.insert_resource(creatures::MotionProfile::default());
    let mut schedule = crate::review_script::animation_schedule();
    // Initialize systems and settle the starting pose outside the measured loop.
    for _ in 0..2 {
        world
            .resource_mut::<Time>()
            .advance_by(Duration::from_millis(20));
        schedule.run(&mut world);
    }
    world
        .get_mut::<creatures::Anim>(root)
        .unwrap()
        .act(creatures::Move::Jump);
    let mut times = Vec::with_capacity(30);
    let mut changed = 0;
    let mut phases: [Vec<f64>; 4] = std::array::from_fn(|_| Vec::with_capacity(30));
    let mut hair_phases: [Vec<f64>; 4] = std::array::from_fn(|_| Vec::with_capacity(30));
    for _ in 0..30 {
        let before = *world.get::<Transform>(lift).unwrap();
        world
            .resource_mut::<Time>()
            .advance_by(Duration::from_millis(20));
        let start = Instant::now();
        schedule.run(&mut world);
        times.push(start.elapsed().as_secs_f64() * 1000.0);
        let profile = world.resource::<creatures::MotionProfile>();
        assert_eq!(profile.rigs, 1, "the workload must profile its actual rig");
        for (values, elapsed) in phases.iter_mut().zip(profile.phases) {
            values.push(elapsed.as_secs_f64() * 1000.0);
        }
        for (values, elapsed) in hair_phases.iter_mut().zip(profile.hair_phases) {
            values.push(elapsed.as_secs_f64() * 1000.0);
        }
        let after = world.get::<Transform>(lift).unwrap();
        assert!(
            after.translation.is_finite() && after.rotation.is_finite() && after.scale.is_finite()
        );
        changed += usize::from(*after != before);
    }
    MotionSample {
        times,
        phases,
        hair_phases,
        changed,
    }
}

/// Emit versioned CPU construction and fixed-step motion samples, without a GPU.
/// Setup and mesh accounting are outside the spawn timers. Both worlds stay alive
/// through measurement: cached geometry is shared, mutable hair/cloth is not.
pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let cases = [
        (
            "human-sylvan-plate",
            CreatureModel::Human,
            Wardrobe::SylvanDuelist,
            "breastplate",
        ),
        (
            "nymph-princess-cloak",
            CreatureModel::Nymph,
            Wardrobe::RoyalPrincess,
            "cloak",
        ),
        (
            "mage-native-boots",
            CreatureModel::Mage,
            Wardrobe::Native,
            "boots",
        ),
    ];
    let mut output = io::BufWriter::new(io::stdout().lock());
    for (case, model, wardrobe, item) in cases {
        let mut equipment = Equipment::default();
        equipment.select(item)?;
        let look = Look::preview_adjusted(model, 7, |body| body.woman = true)
            .composed(wardrobe, equipment)?;
        for sample in 0..3 {
            let mut materials = Assets::<StandardMaterial>::default();
            let mut library = PropLibrary::new(PropMaterials::new(&mut materials));
            let mut meshes = Assets::<Mesh>::default();
            let start = Instant::now();
            let cold = spawn(&look, &mut library, &mut meshes);
            let cold_spawn_ms = start.elapsed().as_secs_f64() * 1000.0;
            let cold_meshes = meshes.len();
            let cold_mesh_bytes = payload(&meshes);
            let start = Instant::now();
            let warm = spawn(&look, &mut library, &mut meshes);
            let warm_spawn_ms = start.elapsed().as_secs_f64() * 1000.0;
            let warm_added_meshes = meshes.len() - cold_meshes;
            let warm_added_mesh_bytes = payload(&meshes) - cold_mesh_bytes;
            let warm_entities = warm.entities().len();
            let motion = motion(warm, meshes);
            writeln!(
                output,
                "{}",
                json!({"version":4,"case":case,"sample":sample,"seed":7,
                "cold_spawn_ms":cold_spawn_ms,"warm_spawn_ms":warm_spawn_ms,
                "cold_meshes":cold_meshes,"warm_added_meshes":warm_added_meshes,
                "cold_mesh_bytes":cold_mesh_bytes,"warm_added_mesh_bytes":warm_added_mesh_bytes,
                "cold_entities":cold.entities().len(),"warm_entities":warm_entities,
                "pose_update_ms":motion.times,"motion_frames":30,"hz":50,"changed_pose_frames":motion.changed,
                "pose_joints_ms":motion.phases[0],"robe_fit_ms":motion.phases[1],
                "drape_update_ms":motion.phases[2],"hair_fit_ms":motion.phases[3],
                "hair_controls_ms":motion.hair_phases[0],"hair_deform_ms":motion.hair_phases[1],
                "hair_constraints_ms":motion.hair_phases[2],"hair_mesh_ms":motion.hair_phases[3]})
            )?;
        }
    }
    output.flush()?;
    Ok(())
}
