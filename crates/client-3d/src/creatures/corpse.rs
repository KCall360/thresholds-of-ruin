use super::*;

/// Live discoveries use a stand-in while their articulated corpse is prepared.
pub(crate) fn request_corpse(
    commands: &mut Commands,
    library: &mut PropLibrary,
    meshes: &mut Assets<Mesh>,
    model: CreatureModel,
    at: Vec3,
    yaw: f32,
    individual: u64,
) -> Entity {
    let look = Look::of(model, individual);
    let mut anim = Anim::corpse(individual);
    anim.yaw = yaw;
    let entity = commands
        .spawn((anim, Transform::from_translation(at), Visibility::default()))
        .id();
    let rig = loading::request(commands, library, meshes, entity, &look);
    commands.entity(entity).insert(rig);
    entity
}

/// A corpse on the floor, using only the disclosed creature recipe.
pub fn spawn_corpse(
    commands: &mut Commands,
    library: &mut PropLibrary,
    meshes: &mut Assets<Mesh>,
    model: CreatureModel,
    at: Vec3,
    yaw: f32,
    individual: u64,
) -> Entity {
    let look = Look::of(model, individual);
    let mut anim = Anim::corpse(individual);
    anim.yaw = yaw;
    let entity = commands
        .spawn((anim, Transform::from_translation(at), Visibility::default()))
        .id();
    let rig = spawn(commands, library, meshes, entity, &look);
    commands.entity(entity).insert(rig);
    entity
}

/// Lights and glowing parts dimmed on the dead, to restore if they rise.
#[derive(Component, Default)]
pub struct Snuffed(Vec<(Entity, f32)>);

/// Put out the glow of the dead: glowing parts fade to embers and any light
/// they carried goes out. Bring it back if the creature stirs again.
pub fn snuff(
    mut commands: Commands,
    library: Option<Res<PropLibrary>>,
    rigs: Query<(Entity, &Anim, Option<&Snuffed>), With<Rig>>,
    children: Query<&Children>,
    mut materials: Query<&mut MeshMaterial3d<StandardMaterial>>,
    mut lights: Query<&mut PointLight>,
) {
    let Some(library) = library else {
        return;
    };
    for (entity, anim, snuffed) in &rigs {
        let dead = anim.dead.is_some_and(|d| d > 0.3);
        match (dead, snuffed) {
            (true, None) => {
                let mut put_out = Vec::new();
                for e in children.iter_descendants(entity) {
                    if let Ok(mut material) = materials.get_mut(e) {
                        if let Some(dim) = library.materials.dim(&material.0) {
                            material.0 = dim;
                        }
                    }
                    if let Ok(mut light) = lights.get_mut(e) {
                        put_out.push((e, light.intensity));
                        light.intensity = 0.0;
                    }
                }
                commands.entity(entity).insert(Snuffed(put_out));
            }
            (false, Some(snuffed)) => {
                for e in children.iter_descendants(entity) {
                    if let Ok(mut material) = materials.get_mut(e) {
                        if let Some(bright) = library.materials.undim(&material.0) {
                            material.0 = bright;
                        }
                    }
                }
                for (e, intensity) in &snuffed.0 {
                    if let Ok(mut light) = lights.get_mut(*e) {
                        light.intensity = *intensity;
                    }
                }
                commands.entity(entity).remove::<Snuffed>();
            }
            _ => {}
        }
    }
}
