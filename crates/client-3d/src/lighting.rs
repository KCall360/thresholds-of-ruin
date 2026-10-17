//! Illumination is a disclosed server fact, separate from visual awareness.
use super::*;
#[cfg(test)]
mod tests;

/// Lighting of a scene root. Descendants (including held objects) inherit it.
#[derive(Component, Clone, Copy)]
pub(super) struct Lit(pub bool);

pub(super) fn at(view: &View, position: Position) -> bool {
    view.visible
        .iter()
        .any(|cell| cell.position == position && cell.lit)
}

fn scope(
    mut entity: Entity,
    parents: &Query<&ChildOf>,
    roots: &Query<&Lit>,
    dead: &Query<&creatures::Snuffed>,
) -> (bool, bool) {
    let mut snuffed = false;
    loop {
        snuffed |= dead.contains(entity);
        if let Ok(lit) = roots.get(entity) {
            return (lit.0, snuffed);
        }
        let Ok(parent) = parents.get(entity) else {
            // No disclosed source cell, so never invent a light source.
            return (false, snuffed);
        };
        entity = parent.parent();
    }
}

/// Moving cosmetic effects inherit only the cell they currently occupy in the
/// disclosed chart. They cannot light an unknown or dark trajectory segment.
#[allow(clippy::type_complexity)]
pub(super) fn effects(
    mut commands: Commands,
    game: Res<Game>,
    effects: Query<
        (Entity, &Transform, Option<&Lit>),
        Or<(With<creatures::Bolt>, With<creatures::Fade>)>,
    >,
) {
    for (entity, transform, previous) in &effects {
        let p = transform.translation;
        let position = Position {
            x: p.x.round() as i32 - game.offset.x,
            y: p.z.round() as i32 - game.offset.y,
            z: (p.y + 0.5).floor() as i32 - game.offset.z,
        };
        let lit = game.view.as_ref().is_some_and(|view| at(view, position));
        if previous.is_none_or(|previous| previous.0 != lit) {
            commands.entity(entity).insert(Lit(lit));
        }
    }
}

/// Muted unlit silhouettes cannot receive the artistic directional/fill light.
/// Restore the recipe's material when authoritative illumination returns.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(super) fn shade(
    library: Res<PropLibrary>,
    mut assets: ResMut<Assets<StandardMaterial>>,
    parents: Query<&ChildOf>,
    roots: Query<&Lit>,
    changed_roots: Query<(), Changed<Lit>>,
    mut removed_materials: RemovedComponents<MeshMaterial3d<StandardMaterial>>,
    dead: Query<&creatures::Snuffed>,
    changed_dead: Query<(), Changed<creatures::Snuffed>>,
    mut removed_dead: RemovedComponents<creatures::Snuffed>,
    mut materials: Query<(Entity, &mut MeshMaterial3d<StandardMaterial>), Without<HostileRing>>,
    mut original: Local<HashMap<Entity, Handle<StandardMaterial>>>,
    mut dark: Local<Option<Handle<StandardMaterial>>>,
) {
    let refresh = !changed_roots.is_empty()
        || !changed_dead.is_empty()
        || removed_dead.read().count() > 0
        || removed_materials.read().count() > 0
        || materials
            .iter_mut()
            .any(|(_, material)| material.is_changed());
    if !refresh {
        return;
    }
    let dark = dark.get_or_insert_with(|| {
        assets.add(StandardMaterial {
            base_color: Color::srgb(0.15, 0.17, 0.22),
            unlit: true,
            ..default()
        })
    });
    original.retain(|entity, _| materials.contains(*entity));
    for (entity, mut material) in &mut materials {
        let source = original.entry(entity).or_insert_with(|| material.0.clone());
        if material.0 != *dark {
            *source = material.0.clone();
        }
        let (lit, snuffed) = scope(entity, &parents, &roots, &dead);
        // Death and resurrection may happen while the dark replacement is in
        // place. Preserve the creature's snuff state across that replacement.
        if let Some(handle) = if snuffed {
            library.materials.dim(source)
        } else {
            library.materials.undim(source)
        } {
            *source = handle;
        }
        let wanted = if lit { source.clone() } else { dark.clone() };
        if material.0 != wanted {
            material.0 = wanted;
        }
    }
}

/// Point lights must obey the same rule as their visible glowing meshes.
pub(super) fn lamps(
    parents: Query<&ChildOf>,
    roots: Query<&Lit>,
    dead: Query<&creatures::Snuffed>,
    mut lights: Query<(Entity, &mut PointLight), Without<Torch>>,
    mut levels: Local<HashMap<Entity, (f32, f32)>>,
) {
    levels.retain(|entity, _| lights.contains(*entity));
    for (entity, mut light) in &mut lights {
        let (lit, snuffed) = scope(entity, &parents, &roots, &dead);
        let (source, applied) = levels
            .entry(entity)
            .or_insert((light.intensity, light.intensity));
        if light.intensity != *applied && !snuffed {
            *source = light.intensity;
        }
        let wanted = if lit && !snuffed { *source } else { 0.0 };
        if light.intensity != wanted {
            light.intensity = wanted;
        }
        *applied = wanted;
    }
}
