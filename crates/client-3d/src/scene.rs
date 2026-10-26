//! Reconcile disclosed views into render entities.
use super::*;
use std::time::Instant;
#[cfg(test)]
mod tests;

#[derive(bevy::ecs::system::SystemParam)]
pub(super) struct SceneAssets<'w> {
    palette: Res<'w, Palette>,
    meshes: ResMut<'w, Assets<Mesh>>,
    library: ResMut<'w, PropLibrary>,
}

type TransientEffects = Or<(With<creatures::Bolt>, With<creatures::Fade>)>;

pub(super) fn receive(
    mut commands: Commands,
    net: Res<Net>,
    mut game: ResMut<Game>,
    mut bodies: Query<&mut ActorBody>,
    effects: Query<Entity, TransientEffects>,
    assets: SceneAssets,
) {
    let SceneAssets {
        palette,
        mut meshes,
        mut library,
    } = assets;
    let events = net.0.lock().unwrap().frame_events();
    for event in events {
        match event {
            NetEvent::Ready => game.busy = false,
            NetEvent::Status(text) => {
                if std::env::var_os("TOR_3D_DEBUG").is_some() {
                    eprintln!("status: {text}");
                }
                game.status = text;
            }
            NetEvent::Fatal(text) => {
                game.status = text;
                game.fatal = true;
            }
            NetEvent::View(view) => {
                let profile_started = props::profile_builds().then(Instant::now);
                align(&mut commands, &mut game, &view);
                if game.snap_camera {
                    for effect in &effects {
                        commands.entity(effect).try_despawn();
                    }
                }
                if view.narration != game.last_narration {
                    // Turn-timing lines repeat every step; the status shows readiness.
                    let noise = ["You must wait.", "You can act again."];
                    for line in view
                        .narration
                        .iter()
                        .filter(|l| !noise.contains(&l.as_str()))
                    {
                        if std::env::var_os("TOR_3D_DEBUG").is_some() {
                            eprintln!("log: {line}");
                        }
                        // Bevy's default font has no middle dot.
                        game.log.push_back(line.replace('\u{b7}', "|"));
                    }
                    while game.log.len() > 200 {
                        game.log.pop_front();
                    }
                    game.last_narration = view.narration.clone();
                }
                game.feet_world = ivec(view.feet) + game.offset;
                let before = game.view.as_ref().and_then(|v| v.hp).map(|(hp, _)| hp);
                if let (Some(before), Some((now, _))) = (before, view.hp) {
                    if now < before {
                        game.hurt = 0.35;
                    }
                }
                if std::env::var_os("TOR_3D_DEBUG").is_some() {
                    eprintln!(
                        "feet {:?} offset {:?} actors {:?}",
                        view.feet,
                        game.offset,
                        view.actors
                            .iter()
                            .map(|a| (a.position, a.is_self, a.name.as_str()))
                            .collect::<Vec<_>>()
                    );
                }
                let cells_started = profile_started.map(|_| Instant::now());
                build_cells(
                    &mut commands,
                    &mut game,
                    &palette,
                    &mut library,
                    &mut meshes,
                    &view,
                );
                let cells_ms = cells_started.map(|time| time.elapsed().as_secs_f64() * 1000.0);
                let fresh = game.last_revision != Some(view.revision);
                game.last_revision = Some(view.revision);
                if fresh {
                    game.pending.extend(view.events.iter().cloned());
                    if let Some(before) = &game.view {
                        let harm = tor_client_3d_model::observed_harm(before, &view);
                        game.pending.extend(harm.into_iter().map(Happening::Hurt));
                    }
                }
                let died: HashSet<ActorTarget> = view
                    .events
                    .iter()
                    .filter_map(|e| match e {
                        Happening::Died(id) if fresh => Some(*id),
                        _ => None,
                    })
                    .collect();
                let actors_started = profile_started.map(|_| Instant::now());
                build_actors(
                    &mut commands,
                    &mut game,
                    &palette,
                    &mut library,
                    &mut meshes,
                    &view,
                    &mut bodies,
                    &died,
                );
                let actors_ms = actors_started.map(|time| time.elapsed().as_secs_f64() * 1000.0);
                let items_started = profile_started.map(|_| Instant::now());
                build_items(&mut commands, &mut game, &mut library, &mut meshes, &view);
                update_held(&mut commands, &mut game, &mut library, &mut meshes, &view);
                if let (Some(started), Some(cells), Some(actors), Some(items)) =
                    (profile_started, cells_ms, actors_ms, items_started)
                {
                    eprintln!(
                        "3d_scene_build view={} actors={} cells_ms={cells:.3} actors_ms={actors:.3} items_ms={:.3} total_ms={:.3}",
                        game.view_serial + 1,
                        view.actors.len(),
                        items.elapsed().as_secs_f64() * 1000.0,
                        started.elapsed().as_secs_f64() * 1000.0,
                    );
                }
                game.view = Some(*view);
                game.view_serial += 1;
            }
        }
    }
}

/// Clear every scene-local cache when the disclosed chart no longer anchors it.
fn align(commands: &mut Commands, game: &mut Game, view: &View) {
    let new_scope = game
        .view
        .as_ref()
        .is_none_or(|old| old.context != view.context || old.branch != view.branch);
    if new_scope {
        commands.insert_resource(interact::Pursuit::default());
        commands.insert_resource(interact::Panel::default());
        commands.insert_resource(interact::Hover::default());
        game.hurt = 0.0;
        game.status.clear();
        game.log.clear();
        game.last_narration.clear();
        game.view = None;
    }
    let change = game
        .frame
        .observe(&view.visible, &view.context, &view.branch);
    game.offset = ivec(change.offset);
    if !change.discontinuous {
        return;
    }
    for (_, entity) in game.actors.drain() {
        commands.entity(entity).try_despawn();
    }
    for (_, (entity, _)) in game.cells.drain() {
        commands.entity(entity).try_despawn();
    }
    for (_, entity) in game.chunks.drain() {
        commands.entity(entity).try_despawn();
    }
    for entity in game.items.drain(..) {
        commands.entity(entity).try_despawn();
    }
    for (_, body) in game.corpses.drain() {
        commands.entity(body.entity).try_despawn();
    }
    for body in game.dying.drain(..) {
        commands.entity(body.entity).try_despawn();
    }
    for entity in game.labels.drain() {
        commands.entity(entity).try_despawn();
    }
    game.cave = CaveWorld::default();
    game.door_leaves.clear();
    game.hands.clear();
    game.held = None;
    game.pending.clear();
    game.last_revision = None;
    game.snap_camera = true;
}

fn look(cell: &tor_client_3d_model::Terrain<'_>) -> CellLook {
    if cell.door.is_some() {
        CellLook::Door
    } else if let Some(model) = cell.object_model {
        CellLook::Object(model)
    } else if cell.stairs_up {
        CellLook::StairsUp
    } else {
        CellLook::StairsDown
    }
}

/// Grid (x, south, up) to Bevy cell coordinates (x, up, south).
fn bevy_cell(at: IVec3) -> IVec3 {
    IVec3::new(at.x, at.z, at.y)
}

fn build_cells(
    commands: &mut Commands,
    game: &mut Game,
    palette: &Palette,
    library: &mut PropLibrary,
    meshes: &mut Assets<Mesh>,
    view: &View,
) {
    let mut solids = HashMap::new();
    let mut open = HashSet::new();
    let mut lit_open = HashSet::new();
    let mut blocked: HashSet<IVec3> = view
        .items
        .iter()
        .map(|i| bevy_cell(ivec(i.position) + game.offset))
        .collect();
    let feet_z = view.feet.z;
    let mut wanted: HashMap<IVec3, (CellLook, tor_client_3d_model::Terrain<'_>)> = HashMap::new();
    let cells = view.terrain();
    let debug = std::env::var_os("TOR_3D_DEBUG").is_some();
    for cell in cells {
        let seen = cell.seen;
        let dz = cell.position.z - feet_z;
        if debug && (cell.stairs_up || cell.stairs_down) {
            eprintln!(
                "stairs: up={} down={} wall={} seen={seen} dz={dz} at={:?}",
                cell.stairs_up, cell.stairs_down, cell.wall, cell.position
            );
        }
        // Ceilings over the character's head would hide it from above.
        if dz >= 2 {
            continue;
        }
        let at = ivec(cell.position) + game.offset;
        if cell.wall && cell.door.is_none() {
            // Visible cells come first, so a fresh sighting wins.
            solids.entry(bevy_cell(at)).or_insert(Solid {
                seen,
                lit: cell.lit,
                key: hash(cell.key),
            });
            continue;
        }
        open.insert(bevy_cell(at));
        if seen && cell.lit {
            lit_open.insert(bevy_cell(at));
        }
        if !(cell.door.is_some() || cell.stairs_up || cell.stairs_down) {
            continue;
        }
        blocked.insert(bevy_cell(at));
        let _ = seen;
        wanted.entry(at).or_insert((look(&cell), cell));
    }
    let started = std::time::Instant::now();
    let dirty = game.cave.update(solids, open, lit_open, blocked);
    let count = dirty.len();
    rebuild_chunks(commands, game, palette, meshes, dirty);
    if std::env::var_os("TOR_3D_DEBUG").is_some() {
        eprintln!(
            "cave: {count} chunks in {:.1} ms",
            started.elapsed().as_secs_f64() * 1000.0
        );
    }
    let door_leaves = &mut game.door_leaves;
    game.cells.retain(|at, (entity, current)| {
        let keep = wanted.get(at).is_some_and(|(look, _)| look == current);
        if !keep {
            commands.entity(*entity).despawn();
            door_leaves.remove(at);
        }
        keep
    });
    let doors: HashSet<IVec3> = wanted
        .iter()
        .filter(|(_, (look, _))| *look == CellLook::Door)
        .map(|(at, _)| *at)
        .collect();
    for (at, (look, cell)) in &wanted {
        let at = *at;
        let lit = lighting::Lit(lighting::at(view, cell.position));
        if let Some((entity, _)) = game.cells.get(&at) {
            commands.entity(*entity).insert((
                lit,
                crate::picking::Pick(tor_client_3d_model::Focus::Cell(cell.position)),
            ));
            continue;
        }
        let base = stand(at);
        let entity = match look {
            CellLook::Object(kind) => {
                let model = library.item(*kind, meshes);
                let scale = if *kind == ItemModel::Stairs {
                    1.0
                } else {
                    ITEM_SCALE
                };
                commands
                    .spawn((
                        Transform::from_translation(base).with_scale(Vec3::splat(scale)),
                        Visibility::default(),
                    ))
                    .with_children(|parent| model.attach(parent))
                    .id()
            }
            CellLook::Door => {
                // A door two cells high is drawn once, from its lower cell.
                if doors.contains(&(at - IVec3::Z)) {
                    continue;
                }
                let b = bevy_cell(at);
                let walls_x = [IVec3::X, IVec3::NEG_X]
                    .iter()
                    .any(|d| game.cave.solids.contains_key(&(b + *d)));
                let turn = if walls_x {
                    0.0
                } else {
                    std::f32::consts::FRAC_PI_2
                };
                let open = cell.door == Some(true);
                let (frame, leaf_model) = (library.door_frame(meshes), library.door_leaf(meshes));
                let leaf = commands
                    .spawn((
                        DoorLeaf { open },
                        Transform::from_xyz(-0.41, 0.0, 0.0).with_rotation(leaf_rotation(open)),
                        Visibility::default(),
                    ))
                    .with_children(|parent| leaf_model.attach(parent))
                    .id();
                game.door_leaves.insert(at, leaf);
                commands
                    .spawn((
                        Transform::from_translation(base)
                            .with_rotation(Quat::from_rotation_y(turn)),
                        Visibility::default(),
                    ))
                    .with_children(|parent| {
                        frame.attach(parent);
                        parent.spawn((
                            PointLight {
                                color: props::Accent::Amber.light(),
                                intensity: 6_000.0,
                                range: 4.0,
                                shadows_enabled: false,
                                ..default()
                            },
                            Transform::from_xyz(0.0, 1.7, 0.4),
                        ));
                    })
                    .add_child(leaf)
                    .id()
            }
            CellLook::StairsUp => {
                let model = library.stairs_up(meshes);
                commands
                    .spawn((Transform::from_translation(base), Visibility::default()))
                    .with_children(|parent| model.attach(parent))
                    .id()
            }
            CellLook::StairsDown => {
                let model = library.stairs_down(meshes);
                commands
                    .spawn((Transform::from_translation(base), Visibility::default()))
                    .with_children(|parent| {
                        model.attach(parent);
                        parent.spawn((
                            PointLight {
                                color: Color::srgb(0.3, 0.5, 1.0),
                                intensity: 5_000.0,
                                range: 3.5,
                                shadows_enabled: false,
                                ..default()
                            },
                            Transform::from_xyz(0.0, -0.6, 0.0),
                        ));
                    })
                    .id()
            }
        };
        commands.entity(entity).insert((
            lit,
            crate::picking::Pick(tor_client_3d_model::Focus::Cell(cell.position)),
        ));
        game.cells.insert(at, (entity, *look));
    }
    // Doors already standing swing to their new state.
    for (at, (_, cell)) in &wanted {
        if let (Some(leaf), Some(door)) = (game.door_leaves.get(at), &cell.door) {
            commands.entity(*leaf).insert(DoorLeaf { open: *door });
        }
    }
    let _ = palette;
}

fn leaf_rotation(open: bool) -> Quat {
    Quat::from_rotation_y(if open { -1.5 } else { 0.0 })
}

/// Rebuild the cave meshes of changed chunks: rock, crystals and their light.
fn rebuild_chunks(
    commands: &mut Commands,
    game: &mut Game,
    palette: &Palette,
    meshes: &mut Assets<Mesh>,
    dirty: HashSet<IVec3>,
) {
    for chunk in dirty {
        if let Some(entity) = game.chunks.remove(&chunk) {
            commands.entity(entity).despawn();
        }
        let built = game.cave.build(chunk);
        if built.rock.is_none() && built.lights.is_empty() {
            continue;
        }
        let entity = commands
            .spawn((
                lighting::Lit(true),
                Transform::default(),
                Visibility::default(),
            ))
            .with_children(|parent| {
                if let Some(rock) = built.rock {
                    parent.spawn((
                        Mesh3d(meshes.add(rock)),
                        MeshMaterial3d(palette.cave_mat.clone()),
                    ));
                }
                for (mesh, material) in built.crystals.into_iter().zip(&palette.crystal_mat) {
                    if let Some(mesh) = mesh {
                        parent.spawn((Mesh3d(meshes.add(mesh)), MeshMaterial3d(material.clone())));
                    }
                }
                for (at, glow) in built.lights {
                    parent.spawn((
                        PointLight {
                            color: glow.light(),
                            intensity: 9_000.0,
                            range: 4.5,
                            radius: 0.15,
                            shadows_enabled: false,
                            ..default()
                        },
                        Transform::from_translation(at),
                    ));
                }
            })
            .id();
        game.chunks.insert(chunk, entity);
    }
}

/// Feet of a creature standing in a cell: on top of the solid cell below.
pub(super) fn stand(at: IVec3) -> Vec3 {
    world(at) - Vec3::Y * 0.5
}

#[allow(clippy::too_many_arguments)]
fn build_actors(
    commands: &mut Commands,
    game: &mut Game,
    palette: &Palette,
    library: &mut PropLibrary,
    meshes: &mut Assets<Mesh>,
    view: &View,
    bodies: &mut Query<&mut ActorBody>,
    died: &HashSet<ActorTarget>,
) {
    let present: HashSet<ActorTarget> = view.actors.iter().map(|a| a.id).collect();
    let mut fallen = Vec::new();
    game.actors.retain(|id, entity| {
        let keep = present.contains(id);
        if !keep {
            game.hands.remove(entity);
            if let Ok(body) = bodies.get(*entity) {
                commands.entity(body.label).despawn();
                game.labels.remove(&body.label);
                commands.entity(body.ring).insert(Visibility::Hidden);
                if died.contains(id) {
                    commands.entity(*entity).remove::<crate::picking::Pick>();
                    let cell = IVec3::new(
                        body.goal.x.round() as i32,
                        body.goal.z.round() as i32,
                        (body.goal.y + 0.5).round() as i32,
                    );
                    fallen.push(DeadBody {
                        id: *id,
                        entity: *entity,
                        cell,
                        appearance_seed: body.appearance_seed,
                        age: 0.0,
                        holds: false,
                    });
                    return false;
                }
            }
            commands.entity(*entity).despawn();
        }
        keep
    });
    game.dying.extend(fallen);
    // Anchor within this individual's disclosed body cells. Do not infer
    // support, gravity, or a floor below what the observer was told.
    let occupied: HashSet<_> = view
        .actors
        .iter()
        .map(|actor| (actor.id, actor.position))
        .collect();
    let anchor = |actor: &tor_client_3d_model::Actor| {
        let mut at = actor.position;
        while let Some(z) = at.z.checked_sub(1) {
            let below = Position { z, ..at };
            if !occupied.contains(&(actor.id, below)) {
                break;
            }
            at = below;
        }
        at
    };
    let mut represented = HashSet::new();
    for actor in &view.actors {
        // This renderer keeps one representative rig per target. Use the
        // same primary for both newly spawned and already cached entities.
        if !represented.insert(actor.id) {
            continue;
        }
        let goal = stand(ivec(anchor(actor)) + game.offset);
        // A tall body is fully illuminated only when all its disclosed cells
        // are lit. This avoids lighting dark portions from another body cell.
        let lit = lighting::Lit(
            view.actors
                .iter()
                .filter(|other| other.id == actor.id)
                .all(|other| lighting::at(view, other.position)),
        );
        if let Some(entity) = game.actors.get(&actor.id).copied() {
            commands.entity(entity).insert((
                lit,
                crate::picking::Pick(tor_client_3d_model::Focus::Actor(actor.id)),
            ));
            if let Ok(mut body) = bodies.get_mut(entity) {
                if body.model == actor.model && body.appearance_seed == actor.appearance_seed {
                    body.goal = goal;
                    if body.name != actor.name || body.hostile != actor.hostile {
                        body.name.clone_from(&actor.name);
                        body.hostile = actor.hostile;
                        commands.entity(body.label).insert((
                            Text::new(if actor.is_self { "" } else { &actor.name }),
                            TextColor(label_color(actor.hostile)),
                        ));
                    }
                    commands
                        .entity(body.ring)
                        .insert(ring_visibility(actor.hostile));
                    continue;
                }
                commands.entity(body.label).despawn();
                game.labels.remove(&body.label);
            }
            commands.entity(entity).try_despawn();
            game.hands.remove(&entity);
        }
        let individual = creatures::individual(actor.appearance_seed);
        let look = Look::of(actor.model, individual);
        let name = if actor.is_self {
            String::new()
        } else {
            actor.name.clone()
        };
        let label = commands
            .spawn((
                Label,
                Text::new(name.clone()),
                TextFont {
                    font_size: 14.0,
                    ..default()
                },
                TextColor(label_color(actor.hostile)),
                Node {
                    position_type: PositionType::Absolute,
                    ..default()
                },
                Visibility::Hidden,
            ))
            .id();
        game.labels.insert(label);
        let ring = commands
            .spawn((
                HostileRing,
                Mesh3d(palette.ring.clone()),
                MeshMaterial3d(palette.hostile_ring.clone()),
                Transform::from_xyz(0.0, 0.03, 0.0)
                    .with_scale(Vec3::splat((look.scale * 1.5).max(0.8))),
                ring_visibility(actor.hostile),
            ))
            .id();
        let entity = commands
            .spawn((
                lit,
                ActorBody {
                    appearance_seed: actor.appearance_seed,
                    appearance_key: look.key.clone(),
                    model: actor.model,
                    hostile: actor.hostile,
                    goal,
                    label,
                    ring,
                    name: actor.name.clone(),
                    height: look.height(),
                },
                Anim::new(individual, goal),
                Transform::from_translation(goal),
                Visibility::default(),
            ))
            .add_child(ring)
            .id();
        let rig = creatures::spawn(commands, library, meshes, entity, &look);
        if let Some(hand) = rig.joint(J::HandR) {
            game.hands.insert(entity, hand);
        }
        commands.entity(entity).insert((
            rig,
            crate::picking::Pick(tor_client_3d_model::Focus::Actor(actor.id)),
        ));
        game.actors.insert(actor.id, entity);
    }
}

/// Act out what the server said happened: the caster's move, its effect, and
/// the target's flinch or sidestep when the blow lands.
pub(super) fn react(
    mut commands: Commands,
    mut game: ResMut<Game>,
    mut library: ResMut<PropLibrary>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut bodies: Query<(&ActorBody, &mut Anim)>,
) {
    let pending = std::mem::take(&mut game.pending);
    let mut arrivals = HashMap::new();
    let find = |game: &Game, id: Option<ActorTarget>| {
        id.and_then(|id| {
            game.actors
                .get(&id)
                .copied()
                .or_else(|| game.dying.iter().find(|d| d.id == id).map(|d| d.entity))
        })
    };
    for happening in pending {
        match happening {
            Happening::Hurt(id) => {
                if let Some(entity) = find(&game, Some(id)) {
                    if let Ok((_, mut anim)) = bodies.get_mut(entity) {
                        anim.hit(arrivals.get(&id).copied().unwrap_or(0.0));
                    }
                }
            }
            Happening::Died(id) => {
                if let Some(entity) = find(&game, Some(id)) {
                    if let Ok((body, mut anim)) = bodies.get_mut(entity) {
                        anim.dead = Some(-0.35);
                        anim.target_yaw = Some(lying_yaw(&game, body.goal));
                    }
                }
            }
            Happening::Ability {
                caster,
                target,
                id,
                name: _,
                outcome,
                strike,
            } => {
                let (Some(caster_entity), target_entity) =
                    (find(&game, caster), find(&game, target))
                else {
                    continue;
                };
                let self_targeted = caster.is_some() && caster == target;
                let action = Move::for_ability(&id, strike);
                let Ok((caster_body, _)) = bodies.get(caster_entity) else {
                    continue;
                };
                let from = (caster_body.goal, caster_body.height);
                let to = target_entity
                    .filter(|_| !self_targeted)
                    .and_then(|e| bodies.get(e).ok())
                    .map(|(b, _)| (b.goal, b.height));
                if let Ok((_, mut anim)) = bodies.get_mut(caster_entity) {
                    anim.act(action);
                    if let Some((at, _)) = to {
                        let d = at - from.0;
                        if d.length_squared() > 0.01 {
                            anim.target_yaw = Some(d.x.atan2(d.z));
                        }
                    }
                }
                let reaction = creatures::stage(
                    &mut commands,
                    &mut library,
                    &mut meshes,
                    action,
                    from,
                    to,
                    outcome == tor_protocol::AbilityOutcome::Applied,
                );
                if let (Some(delay), Some(target_entity)) =
                    (reaction, target_entity.filter(|_| !self_targeted))
                {
                    if let Ok((_, mut anim)) = bodies.get_mut(target_entity) {
                        match outcome {
                            tor_protocol::AbilityOutcome::Miss => anim.miss(delay),
                            tor_protocol::AbilityOutcome::Applied => {
                                if let Some(id) = target {
                                    arrivals.insert(id, delay);
                                }
                            }
                            tor_protocol::AbilityOutcome::Unaffected => {}
                        }
                    }
                }
            }
        }
    }
}

/// Which way a body lying in a cell should point so it stays out of the
/// rock: along whichever axis has open cells on both sides. Its length runs
/// along the rig's x axis once it has fallen.
fn lying_yaw(game: &Game, at: Vec3) -> f32 {
    let cell = IVec3::new(
        at.x.round() as i32,
        at.z.round() as i32,
        (at.y + 0.5).round() as i32,
    );
    let rock = |d: IVec3| {
        [d, -d]
            .iter()
            .filter(|d| game.cave.solids.contains_key(&bevy_cell(cell + **d)))
            .count()
    };
    if rock(IVec3::X) <= rock(IVec3::Y) {
        0.0
    } else {
        std::f32::consts::FRAC_PI_2
    }
}

/// Bodies of the fallen stay while their corpse lies where they fell.
pub(super) fn tend_the_dead(mut commands: Commands, time: Res<Time>, mut game: ResMut<Game>) {
    let dt = time.delta_secs();
    game.dying.retain_mut(|body| {
        body.age += dt;
        let keep = body.holds || body.age < 2.0;
        if !keep {
            commands.entity(body.entity).despawn();
        }
        keep
    });
}

fn label_color(hostile: bool) -> Color {
    if hostile {
        Color::srgb(1.0, 0.55, 0.5)
    } else {
        Color::srgb(0.85, 0.85, 0.8)
    }
}

fn ring_visibility(hostile: bool) -> Visibility {
    if hostile {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    }
}

/// Items are drawn larger than life so they read at play distance.
const ITEM_SCALE: f32 = 0.72;

/// Lay each item on the floor as its model, with a glint so it reads in the
/// dark. Several in one cell gather around its middle.
fn build_items(
    commands: &mut Commands,
    game: &mut Game,
    library: &mut PropLibrary,
    meshes: &mut Assets<Mesh>,
    view: &View,
) {
    for entity in game.items.drain(..) {
        commands.entity(entity).despawn();
    }
    for body in &mut game.dying {
        body.holds = false;
        // An undisclosed fading body is historical presentation, never a
        // current light source. A disclosed corpse below may restore it.
        commands
            .entity(body.entity)
            .insert(lighting::Lit(false))
            .remove::<crate::picking::Pick>();
    }
    // Corpses stay where they lie: keep the ones still seen in place.
    let mut seen_corpses = HashSet::new();
    let mut per_cell: HashMap<IVec3, u32> = HashMap::new();
    let glint = library.glint(meshes);
    for item in &view.items {
        let at = ivec(item.position) + game.offset;
        let lit = lighting::Lit(lighting::at(view, item.position));
        let count = per_cell.entry(at).or_insert(0);
        let index = *count;
        *count += 1;
        let key = hash(&item.name) ^ u64::from(index);
        let angle = (key % 6283) as f32 / 1000.0;
        let spread = if index == 0 {
            Vec3::ZERO
        } else {
            let a = index as f32 * 2.4;
            Vec3::new(a.cos(), 0.0, a.sin()) * 0.27
        };
        if item.model == ItemModel::Corpse {
            if let Some(body) = game.dying.iter_mut().find(|d| {
                !d.holds
                    && d.cell == at
                    && item.corpse_seed.is_some()
                    && d.appearance_seed == item.corpse_seed
            }) {
                body.holds = true;
                commands.entity(body.entity).insert((
                    lit,
                    crate::picking::Pick(tor_client_3d_model::Focus::Item(item.id)),
                ));
                continue;
            }
            seen_corpses.insert(item.id);
            if let Some(body) = game.corpses.get(&item.id).filter(|body| {
                body.cell == at
                    && body.model == item.corpse_model
                    && body.appearance_seed == item.corpse_seed
            }) {
                commands.entity(body.entity).insert(lit);
                continue;
            }
            if let Some(old) = game.corpses.remove(&item.id) {
                commands.entity(old.entity).despawn();
            }
            let entity = creatures::spawn_corpse(
                commands,
                library,
                meshes,
                item.corpse_model,
                stand(at),
                lying_yaw(game, stand(at)),
                creatures::individual(item.corpse_seed),
            );
            commands.entity(entity).insert((
                lit,
                crate::picking::Pick(tor_client_3d_model::Focus::Item(item.id)),
            ));
            commands.entity(entity).with_children(|parent| {
                parent
                    .spawn((
                        Glint { phase: angle },
                        Transform::from_xyz(0.0, 0.45, 0.0),
                        Visibility::default(),
                    ))
                    .with_children(|g| glint.attach(g));
            });
            game.corpses.insert(
                item.id,
                CachedCorpse {
                    entity,
                    cell: at,
                    model: item.corpse_model,
                    appearance_seed: item.corpse_seed,
                },
            );
            continue;
        }
        let model = library.item(item.model, meshes);
        let entity = commands
            .spawn((
                lit,
                crate::picking::Pick(tor_client_3d_model::Focus::Item(item.id)),
                Transform::from_translation(stand(at) + spread)
                    .with_rotation(Quat::from_rotation_y(angle))
                    .with_scale(Vec3::splat(ITEM_SCALE)),
                Visibility::default(),
            ))
            .with_children(|parent| {
                model.attach(parent);
                parent
                    .spawn((
                        Glint { phase: angle },
                        Transform::from_xyz(0.0, 0.32, 0.0),
                        Visibility::default(),
                    ))
                    .with_children(|g| glint.attach(g));
            })
            .id();
        game.items.push(entity);
    }
    prune_corpses(commands, game, &seen_corpses);
}

/// Bright, readable figures and usable objects take priority over darkness.
/// A warm key sculpts forms; generous cool fill opens their shadow faces while
/// the backdrop palette, set design and local light contrast carry the mood.
pub(crate) fn studio_lights(commands: &mut Commands) {
    for (color, lux, from, shadows) in [
        (
            Color::srgb(1.0, 0.94, 0.86),
            2600.0,
            Vec3::new(-6.0, 10.0, 6.0),
            true,
        ),
        (
            Color::srgb(0.72, 0.84, 1.0),
            1050.0,
            Vec3::new(9.0, 4.0, 2.0),
            false,
        ),
        (
            Color::srgb(0.7, 0.82, 1.0),
            700.0,
            Vec3::new(1.0, 5.0, -9.0),
            false,
        ),
    ] {
        let mut light = commands.spawn((
            DirectionalLight {
                color,
                illuminance: lux,
                shadows_enabled: shadows,
                ..default()
            },
            Transform::from_translation(from).looking_at(Vec3::ZERO, Vec3::Y),
        ));
        if shadows {
            // Spend shadow resolution on the playable room and cast, rather
            // than the engine's 150-unit outdoor default range.
            light.insert(
                bevy::pbr::CascadeShadowConfigBuilder {
                    num_cascades: 3,
                    first_cascade_far_bound: 12.0,
                    maximum_distance: 48.0,
                    ..default()
                }
                .build(),
            );
        }
    }
}

/// Clear corpses no longer seen, after a view's items are laid out.
fn prune_corpses(
    commands: &mut Commands,
    game: &mut Game,
    seen: &HashSet<tor_protocol::ItemTarget>,
) {
    game.corpses.retain(|id, body| {
        let keep = seen.contains(id);
        if !keep {
            commands.entity(body.entity).despawn();
        }
        keep
    });
}

/// Show the equipped weapon at the player's side.
fn update_held(
    commands: &mut Commands,
    game: &mut Game,
    library: &mut PropLibrary,
    meshes: &mut Assets<Mesh>,
    view: &View,
) {
    let weapon = view
        .inventory
        .iter()
        .find(|c| c.equipped.is_some() && c.class == ItemClass::Weapon);
    let body = view
        .actors
        .iter()
        .find(|a| a.is_self)
        .and_then(|a| game.actors.get(&a.id))
        .and_then(|body| game.hands.get(body))
        .copied();
    let wanted = weapon.zip(body).map(|(w, b)| (w.model, b));
    if wanted == game.held.as_ref().map(|(model, body, _)| (*model, *body)) {
        return;
    }
    if let Some((_, _, model)) = game.held.take() {
        commands.entity(model).try_despawn();
    }
    if let Some((kind, body)) = wanted {
        let model = library.item(kind, meshes);
        let held = commands
            .spawn((crate::props::held_transform(kind), Visibility::default()))
            .with_children(|parent| model.attach(parent))
            .id();
        commands.entity(body).add_child(held);
        game.held = Some((kind, body, held));
    }
}

pub(super) fn animate_props(
    time: Res<Time>,
    mut glints: Query<(&Glint, &mut Transform), Without<DoorLeaf>>,
    mut leaves: Query<(&DoorLeaf, &mut Transform), Without<Glint>>,
) {
    let t = time.elapsed_secs();
    for (glint, mut transform) in &mut glints {
        let twinkle = ((t * 1.7 + glint.phase).sin() * 0.5 + 0.5).powi(6);
        transform.scale = Vec3::splat(0.25 + 0.9 * twinkle);
        transform.rotation =
            Quat::from_rotation_y(t * 0.9 + glint.phase) * Quat::from_rotation_z(0.6);
        transform.translation.y = 0.3 + (t * 1.3 + glint.phase).sin() * 0.03;
    }
    let smooth = 1.0 - (-time.delta_secs() * 6.0).exp();
    for (leaf, mut transform) in &mut leaves {
        transform.rotation = transform.rotation.slerp(leaf_rotation(leaf.open), smooth);
    }
}
