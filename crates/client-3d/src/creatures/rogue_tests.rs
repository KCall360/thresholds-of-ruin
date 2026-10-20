//! The Rogue scenario's whole bestiary: every monster it names is drawn with a
//! recipe of its own, with the rig landmarks its body plan needs.
use super::*;
use std::collections::{BTreeMap, BTreeSet};
use tor_client_common::Palette;
use tor_protocol::{PaletteBody, PaletteUpdate};

/// The monsters the Rogue scenario names in its dungeon asset list; the
/// player's own body is named elsewhere in the manifest.
fn rogue_creature_assets() -> BTreeSet<String> {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../scenarios/rogue-exploration/scenario.toml");
    let text = std::fs::read_to_string(&manifest).expect("rogue scenario manifest");
    let dungeon = text
        .split("dungeon = [")
        .nth(1)
        .and_then(|rest| rest.split(']').next())
        .expect("dungeon asset list");
    dungeon
        .split('"')
        .filter(|token| token.starts_with("creature."))
        .map(str::to_owned)
        .collect()
}

/// Each Rogue asset with the model the client resolves for it.
fn rogue_models() -> Vec<(String, CreatureModel)> {
    let assets = rogue_creature_assets();
    let mut palette = Palette::default();
    palette.apply(&PaletteUpdate {
        revision: 1,
        body: PaletteBody::Full {
            assets: assets.clone(),
        },
    });
    assets
        .into_iter()
        .map(|asset| {
            let model = tor_client_3d_model::creature_model(&palette, Some(&asset));
            (asset, model)
        })
        .collect()
}

#[test]
fn every_rogue_monster_is_drawn_with_its_own_recipe() {
    let models = rogue_models();
    assert_eq!(models.len(), 26);
    let mut recipes: BTreeMap<String, String> = BTreeMap::new();
    for (asset, model) in models {
        let look = Look::of(model, 7);
        assert_ne!(look.plan, Plan::Marker, "{asset} is drawn as a marker");
        let recipe = format!("{:?}", look.style);
        assert!(
            !matches!(look.style, Style::Common),
            "{asset} is drawn as a generic person"
        );
        if let Some(other) = recipes.insert(recipe.clone(), asset.clone()) {
            panic!("{asset} and {other} share the {recipe} recipe");
        }
    }
}

/// Spawn one creature into a fresh world, returning the world and its rig.
fn spawn_one(look: &Look) -> (World, Rig, PropLibrary, Assets<Mesh>) {
    let mut materials = Assets::<StandardMaterial>::default();
    let mut library = PropLibrary::new(crate::props::PropMaterials::new(&mut materials));
    let mut meshes = Assets::<Mesh>::default();
    let mut world = World::new();
    let parent = world.spawn(Transform::default()).id();
    let mut queue = bevy::ecs::world::CommandQueue::default();
    let rig = spawn(
        &mut Commands::new(&mut queue, &world),
        &mut library,
        &mut meshes,
        parent,
        look,
    );
    queue.apply(&mut world);
    (world, rig, library, meshes)
}

#[test]
fn rogue_rigs_carry_their_landmarks_and_finite_meshes() {
    for (asset, model) in rogue_models() {
        let look = Look::of(model, 11);
        let (_, rig, _, meshes) = spawn_one(&look);
        let has = |j: J| rig.joint(j).is_some();
        match look.style {
            Style::Bat | Style::Kestrel => {
                for j in [
                    J::Body,
                    J::Head,
                    J::WingL,
                    J::WingR,
                    J::WingTipL,
                    J::WingTipR,
                ] {
                    assert!(has(j), "{asset} lacks {j:?}");
                }
                assert!(
                    !has(J::FootL) && !has(J::FootR),
                    "{asset} must have no planted feet to stand on"
                );
            }
            Style::Griffin | Style::Dragon => {
                for j in [
                    J::WingL,
                    J::WingR,
                    J::WingTipL,
                    J::WingTipR,
                    J::Tail,
                    J::HoofFL,
                ] {
                    assert!(has(j), "{asset} lacks {j:?}");
                }
            }
            Style::Flytrap => {
                for j in [J::Body, J::Neck, J::Torso, J::Head, J::Jaw] {
                    assert!(has(j), "{asset} lacks {j:?}");
                }
            }
            _ => {}
        }
        if matches!(
            look.style,
            Style::Dragon | Style::Jabberwock | Style::Xeroc | Style::Aquator
        ) {
            assert!(has(J::Jaw), "{asset} needs a jaw to open");
        }
        let segments = match look.style {
            Style::Medusa => 11,
            Style::Aquator => 8,
            Style::Rattlesnake | Style::Snake => geometry::SNAKE,
            Style::Ice => 6,
            _ => 0,
        };
        if segments > 0 {
            assert_eq!(rig.segments.len(), segments, "{asset} segments");
        }
        let mut vertices = 0;
        for (_, mesh) in meshes.iter() {
            let Some(bevy::render::mesh::VertexAttributeValues::Float32x3(positions)) =
                mesh.attribute(Mesh::ATTRIBUTE_POSITION)
            else {
                continue;
            };
            vertices += positions.len();
            assert!(
                positions.iter().flatten().all(|v| v.is_finite()),
                "{asset} has a non-finite vertex"
            );
        }
        assert!(vertices > 500, "{asset} has almost no geometry: {vertices}");
    }
}

#[test]
fn rogue_beasts_stand_with_their_soles_on_the_floor() {
    for model in [
        CreatureModel::Griffin,
        CreatureModel::Dragon,
        CreatureModel::Jabberwock,
        CreatureModel::Aquator,
        CreatureModel::Xeroc,
    ] {
        let (_, rig, _, _) = spawn_one(&Look::of(model, 3));
        for (thigh, shin, foot) in [
            (J::FL, J::FLs, J::HoofFL),
            (J::FR, J::FRs, J::HoofFR),
            (J::BL, J::BLs, J::HoofBL),
            (J::BR, J::BRs, J::HoofBR),
        ] {
            let ankle = rig.rest[J::Body as usize]
                + rig.rest[thigh as usize]
                + rig.rest[shin as usize]
                + rig.rest[foot as usize];
            assert!(
                (rig.ankle_floor - 0.008 - ankle.y).abs() < 1e-4,
                "{model:?} {foot:?} ankle {} against floor {}",
                ankle.y,
                rig.ankle_floor
            );
        }
        assert!(
            rig.legs > 0.2 && rig.legs < 1.5,
            "{model:?} legs {}",
            rig.legs
        );
        assert!(rig.girth > 0.1, "{model:?} girth {}", rig.girth);
    }
}

#[test]
fn the_phantom_is_drawn_through_the_translucent_material() {
    for (model, ghostly) in [
        (CreatureModel::Phantom, true),
        (CreatureModel::Wraith, false),
    ] {
        let (mut world, _, library, _) = spawn_one(&Look::of(model, 5));
        let ghost = library.materials.ghost();
        let mut query = world.query::<&MeshMaterial3d<StandardMaterial>>();
        let translucent = query.iter(&world).filter(|m| m.0 == ghost).count();
        let total = query.iter(&world).count();
        if ghostly {
            assert!(
                translucent * 2 > total,
                "{translucent} of {total} parts translucent"
            );
        } else {
            assert_eq!(translucent, 0, "the wraith stays solid");
        }
    }
}

/// Run the shared animation system over a rig moved along `path` each frame.
fn animate_along(
    model: CreatureModel,
    frames: usize,
    path: impl Fn(usize) -> Vec3,
) -> (App, Entity) {
    animate_look(Look::of(model, 9), frames, path)
}

/// As `animate_along`, for an exact individual.
fn animate_look(look: Look, frames: usize, path: impl Fn(usize) -> Vec3) -> (App, Entity) {
    use bevy::time::TimeUpdateStrategy;
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(TimeUpdateStrategy::ManualDuration(
            std::time::Duration::from_millis(16),
        ))
        .init_resource::<Assets<Mesh>>()
        .add_systems(Update, animate);
    let mut materials = Assets::<StandardMaterial>::default();
    let mut library = PropLibrary::new(crate::props::PropMaterials::new(&mut materials));
    let root = app
        .world_mut()
        .spawn((Anim::new(9, path(0)), Transform::from_translation(path(0))))
        .id();
    let mut queue = bevy::ecs::world::CommandQueue::default();
    let rig = {
        let world = app.world_mut();
        let mut meshes = Assets::<Mesh>::default();
        let rig = spawn(
            &mut Commands::new(&mut queue, world),
            &mut library,
            &mut meshes,
            root,
            &look,
        );
        queue.apply(world);
        rig
    };
    app.world_mut().entity_mut(root).insert(rig);
    for frame in 0..frames {
        app.world_mut()
            .get_mut::<Transform>(root)
            .unwrap()
            .translation = path(frame);
        app.update();
    }
    (app, root)
}

#[test]
fn fliers_stay_on_the_wing_without_ground_contacts() {
    for model in [CreatureModel::Bat, CreatureModel::Kestrel] {
        let (app, root) = animate_along(model, 90, |f| Vec3::Z * f as f32 * 0.03);
        let rig = app.world().get::<Rig>(root).unwrap();
        let anim = app.world().get::<Anim>(root).unwrap();
        assert!(anim.contacts.rig.is_none(), "{model:?} must not plant feet");
        let lift = app
            .world()
            .get::<Transform>(rig.joint(J::Lift).unwrap())
            .unwrap();
        let body = lift.translation.y + rig.rest[J::Body as usize].y * rig.scale;
        assert!(body > 0.5, "{model:?} sank to {body}");
        let wing = app
            .world()
            .get::<Transform>(rig.joint(J::WingL).unwrap())
            .unwrap();
        assert!(
            wing.rotation.angle_between(Quat::IDENTITY) > 0.01,
            "{model:?} wings must beat"
        );
    }
}

#[test]
fn walking_rogue_beasts_plant_their_feet() {
    for model in [CreatureModel::Dragon, CreatureModel::Aquator] {
        let (app, root) = animate_along(model, 60, |f| Vec3::Z * f as f32 * 0.02);
        let anim = app.world().get::<Anim>(root).unwrap();
        assert!(
            anim.contacts.rig.is_some(),
            "{model:?} must walk on its own feet"
        );
    }
}

#[test]
fn rogue_signature_abilities_have_their_own_motions() {
    for (id, motion) in [
        ("rogue.breath_weapon", Move::Breath(Accent::Ember)),
        ("rogue.confusing_gaze", Move::Gaze),
        ("rogue.corrode", Move::Touch(Accent::Amber)),
        ("rogue.energy_drain", Move::Drain),
        ("rogue.ambush_from_guise", Move::Shove),
        ("rogue.venomous_bite", Move::Bite(Accent::Teal)),
    ] {
        assert_eq!(Move::for_ability(id, None), motion, "{id}");
        assert!(motion.duration() > 0.0 && (0.0..1.0).contains(&motion.impact()));
    }
}

#[test]
fn cloaks_flare_and_skirts_sway_as_their_wearers_travel() {
    let drape_angles = |moving: bool| {
        let speed = if moving { 0.03 } else { 0.0 };
        let (app, root) = animate_along(CreatureModel::Human, 60, |f| Vec3::Z * f as f32 * speed);
        let rig = app.world().get::<Rig>(root).unwrap();
        let mut cloak = None;
        let mut skirt = None;
        for &(e, kind) in &rig.drapes {
            let angle = app
                .world()
                .get::<Transform>(e)
                .unwrap()
                .rotation
                .angle_between(Quat::IDENTITY);
            match kind {
                Drape::Cloak => cloak = Some(angle),
                Drape::Skirt(_) => skirt = Some(angle),
                Drape::Hair => {}
            }
        }
        (
            cloak.expect("the warrior wears a cloak"),
            skirt.expect("and a tunic skirt"),
        )
    };
    let (cloak, skirt) = drape_angles(true);
    assert!(cloak > 0.15, "a travelling cloak flares back: {cloak}");
    assert!(skirt > 0.01, "a travelling skirt sways: {skirt}");
    let (cloak, skirt) = drape_angles(false);
    assert!(
        cloak < 0.05 && skirt < 0.03,
        "cloth at rest barely stirs: {cloak} {skirt}"
    );
}

#[test]
fn leather_jerkins_hang_free_below_the_belt_and_stay_shorter_than_the_tunic() {
    for model in [
        CreatureModel::Human,
        CreatureModel::Scout,
        CreatureModel::Hobgoblin,
        CreatureModel::Orc,
    ] {
        let look = Look::of(model, 4);
        let (_, rig, _, _) = spawn_one(&look);
        let skirts: Vec<f32> = rig
            .drapes
            .iter()
            .filter_map(|(_, kind)| match kind {
                Drape::Skirt(length) => Some(*length),
                Drape::Cloak | Drape::Hair => None,
            })
            .collect();
        let peplum = geometry::jerkin_peplum(look.style).expect("a jerkin with a free hem");
        assert!(
            skirts.contains(&peplum),
            "{model:?} hangs its peplum as a drape"
        );
        assert!(
            skirts.iter().any(|&tunic| tunic > peplum),
            "{model:?}: the tunic skirt shows below the jerkin's hem"
        );
    }
    assert!(geometry::jerkin_peplum(Look::of(CreatureModel::Mage, 4).style).is_none());
}

/// Walk a creature at a steady pace with the real animation system and
/// return, for each foot, the median horizontal slide between consecutive
/// frames while it is planted (within 3 mm of its lowest height), and the
/// share of those frames that slide more than a millimetre.
fn planted_foot_slide(model: CreatureModel, path: &dyn Fn(usize) -> Vec3) -> Vec<(J, f32, f32)> {
    use bevy::time::TimeUpdateStrategy;
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, bevy::transform::TransformPlugin))
        .insert_resource(TimeUpdateStrategy::ManualDuration(
            std::time::Duration::from_millis(16),
        ))
        .init_resource::<Assets<Mesh>>()
        .add_systems(Update, animate);
    let mut materials = Assets::<StandardMaterial>::default();
    let mut library = PropLibrary::new(crate::props::PropMaterials::new(&mut materials));
    let look = Look::of(model, 21);
    let root = app
        .world_mut()
        .spawn((
            Anim::new(21, Vec3::ZERO),
            Transform::default(),
            Visibility::default(),
        ))
        .id();
    let mut queue = bevy::ecs::world::CommandQueue::default();
    let rig = {
        let world = app.world_mut();
        let mut meshes = Assets::<Mesh>::default();
        let rig = spawn(
            &mut Commands::new(&mut queue, world),
            &mut library,
            &mut meshes,
            root,
            &look,
        );
        queue.apply(world);
        rig
    };
    let feet: Vec<J> = [
        J::FootL,
        J::FootR,
        J::HoofFL,
        J::HoofFR,
        J::HoofBL,
        J::HoofBR,
    ]
    .into_iter()
    .filter(|j| rig.joint(*j).is_some())
    .collect();
    let entities: Vec<Entity> = feet.iter().map(|j| rig.joint(*j).unwrap()).collect();
    app.world_mut().entity_mut(root).insert(rig);
    let mut tracks: Vec<Vec<Vec3>> = vec![Vec::new(); feet.len()];
    for frame in 0..200 {
        app.world_mut()
            .get_mut::<Transform>(root)
            .unwrap()
            .translation = path(frame);
        app.update();
        if frame >= 60 {
            for (track, e) in tracks.iter_mut().zip(&entities) {
                track.push(
                    app.world()
                        .get::<GlobalTransform>(*e)
                        .unwrap()
                        .translation(),
                );
            }
        }
    }
    feet.into_iter()
        .zip(tracks)
        .map(|(foot, track)| {
            // The planted height: low, but robust to one unusually deep frame.
            let mut heights: Vec<f32> = track.iter().map(|p| p.y).collect();
            heights.sort_by(f32::total_cmp);
            let low = heights[heights.len() / 10];
            let mut slides: Vec<f32> = track
                .windows(2)
                .filter(|w| w[0].y < low + 0.003 && w[1].y < low + 0.003)
                .map(|w| (w[1] - w[0]).with_y(0.0).length())
                .collect();
            slides.sort_by(f32::total_cmp);
            if slides.len() < 10 {
                return (foot, f32::NAN, f32::NAN);
            }
            let median = slides[slides.len() / 2];
            let sliding =
                slides.iter().filter(|s| **s > 0.001).count() as f32 / slides.len() as f32;
            (foot, median, sliding)
        })
        .collect()
}

#[test]
fn real_speed_walk_review_keeps_planted_feet_from_sliding() {
    let mut report = Vec::new();
    for model in [
        CreatureModel::Human,
        CreatureModel::Scout,
        CreatureModel::Hobgoblin,
        CreatureModel::Zombie,
        CreatureModel::Guardian,
        CreatureModel::Yeti,
        CreatureModel::Orc,
        CreatureModel::Troll,
        CreatureModel::Leprechaun,
        CreatureModel::Emu,
        CreatureModel::Quagga,
        CreatureModel::Centaur,
        CreatureModel::Griffin,
        CreatureModel::Dragon,
        CreatureModel::Jabberwock,
        CreatureModel::Aquator,
        CreatureModel::Xeroc,
    ] {
        for (foot, median, sliding) in planted_foot_slide(model, &|f| Vec3::Z * f as f32 * 0.03) {
            report.push(format!(
                "{model:?} {foot:?} median {median:.4} sliding {sliding:.2}"
            ));
            assert!(
                median < 0.001,
                "{model:?} {foot:?} slides while planted: {median}"
            );
            // Touchdown and lift-off frames may move; the stance between may not.
            assert!(
                sliding < 0.2,
                "{model:?} {foot:?} drags through its stance: {sliding}"
            );
        }
    }
    eprintln!("{}", report.join("\n"));
}

/// The same review on harder paths: a steady turn, and a walk that stops,
/// settles and sets off again.
#[test]
fn real_speed_turns_and_stops_keep_planted_feet_from_sliding() {
    let turn = |f: usize| {
        let a = f as f32 * 0.02;
        Vec3::new(1.5 - 1.5 * a.cos(), 0.0, 1.5 * a.sin())
    };
    let stop = |f: usize| {
        let moving = f.min(90) + f.saturating_sub(140);
        Vec3::Z * moving as f32 * 0.03
    };
    let mut report = Vec::new();
    for model in [
        CreatureModel::Human,
        CreatureModel::Zombie,
        CreatureModel::Troll,
        CreatureModel::Emu,
        CreatureModel::Quagga,
        CreatureModel::Centaur,
        CreatureModel::Griffin,
        CreatureModel::Dragon,
        CreatureModel::Aquator,
    ] {
        for (name, path) in [("turn", &turn as &dyn Fn(usize) -> Vec3), ("stop", &stop)] {
            for (foot, median, sliding) in planted_foot_slide(model, path) {
                report.push(format!(
                    "{name} {model:?} {foot:?} median {median:.4} sliding {sliding:.2}"
                ));
                // Tight turns may yield a little rather than cross the legs,
                // and a stop settles each foot into place; neither may skate.
                assert!(
                    median.is_nan() || median < 0.0025,
                    "{name}: {model:?} {foot:?} skates while planted: {median}"
                );
            }
        }
    }
    eprintln!(
        "{}",
        report.join(
            "
"
        )
    );
}

/// The largest frame-to-frame jump of any foot or the head while a creature
/// walks, is interrupted by an action, falls dead and is revived.
fn largest_pop(model: CreatureModel) -> (f32, f32, f32) {
    use bevy::time::TimeUpdateStrategy;
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, bevy::transform::TransformPlugin))
        .insert_resource(TimeUpdateStrategy::ManualDuration(
            std::time::Duration::from_millis(16),
        ))
        .init_resource::<Assets<Mesh>>()
        .add_systems(Update, animate);
    let mut materials = Assets::<StandardMaterial>::default();
    let mut library = PropLibrary::new(crate::props::PropMaterials::new(&mut materials));
    let look = Look::of(model, 33);
    let root = app
        .world_mut()
        .spawn((
            Anim::new(33, Vec3::ZERO),
            Transform::default(),
            Visibility::default(),
        ))
        .id();
    let mut queue = bevy::ecs::world::CommandQueue::default();
    let rig = {
        let world = app.world_mut();
        let mut meshes = Assets::<Mesh>::default();
        let rig = spawn(
            &mut Commands::new(&mut queue, world),
            &mut library,
            &mut meshes,
            root,
            &look,
        );
        queue.apply(world);
        rig
    };
    let watched: Vec<Entity> = [
        J::FootL,
        J::FootR,
        J::HoofFL,
        J::HoofFR,
        J::HoofBL,
        J::HoofBR,
        J::Head,
    ]
    .into_iter()
    .filter_map(|j| rig.joint(j))
    .collect();
    app.world_mut().entity_mut(root).insert(rig);
    let (mut interrupt, mut revive, mut react) = (0.0f32, 0.0f32, 0.0f32);
    let mut last: Vec<Vec3> = Vec::new();
    for frame in 0..320 {
        let travel = frame.min(100) as f32 * 0.03;
        app.world_mut()
            .get_mut::<Transform>(root)
            .unwrap()
            .translation = Vec3::Z * travel;
        {
            let mut anim = app.world_mut().get_mut::<Anim>(root).unwrap();
            match frame {
                30 => anim.hit(0.0),
                80 => anim.act(Move::Strike),
                140 => anim.miss(0.0),
                160 => anim.dead = Some(0.0),
                260 => anim.dead = None,
                _ => {}
            }
        }
        app.update();
        let now: Vec<Vec3> = watched
            .iter()
            .map(|e| {
                app.world()
                    .get::<GlobalTransform>(*e)
                    .unwrap()
                    .translation()
            })
            .collect();
        if !last.is_empty() {
            let jump = now
                .iter()
                .zip(&last)
                .map(|(a, b)| a.distance(*b))
                .fold(0.0, f32::max);
            if (30..70).contains(&frame) || (140..160).contains(&frame) {
                react = react.max(jump);
            } else if (80..140).contains(&frame) {
                interrupt = interrupt.max(jump);
            } else if (260..320).contains(&frame) {
                revive = revive.max(jump);
            }
        }
        last = now;
    }
    (interrupt, revive, react)
}

#[test]
fn real_speed_interruptions_and_revival_do_not_pop() {
    let mut report = Vec::new();
    for model in [
        CreatureModel::Human,
        CreatureModel::Zombie,
        CreatureModel::Troll,
        CreatureModel::Emu,
        CreatureModel::Quagga,
        CreatureModel::Centaur,
        CreatureModel::Griffin,
        CreatureModel::Dragon,
        CreatureModel::Aquator,
        CreatureModel::Bat,
        CreatureModel::Flytrap,
    ] {
        let (interrupt, revive, react) = largest_pop(model);
        // A flinch or sidestep while walking keeps the feet planted.
        assert!(react < 0.25, "{model:?} pops when hit or missed: {react}");
        report.push(format!(
            "{model:?} interrupt {interrupt:.3} revive {revive:.3}"
        ));
        // An action cut into the walk moves no faster than its own lunge.
        assert!(
            interrupt < 0.4,
            "{model:?} pops when interrupted: {interrupt}"
        );
        // Revival rises out of the death pose rather than snapping upright.
        assert!(revive < 0.2, "{model:?} pops when revived: {revive}");
    }
    eprintln!("{}", report.join("\n"));
}

/// The largest frame-to-frame jump of the hands and head as an action ends
/// and the idle fidgets take over again, for a creature standing still.
fn action_exit_pop(model: CreatureModel) -> f32 {
    use bevy::time::TimeUpdateStrategy;
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, bevy::transform::TransformPlugin))
        .insert_resource(TimeUpdateStrategy::ManualDuration(
            std::time::Duration::from_millis(16),
        ))
        .init_resource::<Assets<Mesh>>()
        .add_systems(Update, animate);
    let mut materials = Assets::<StandardMaterial>::default();
    let mut library = PropLibrary::new(crate::props::PropMaterials::new(&mut materials));
    let look = Look::of(model, 5);
    let root = app
        .world_mut()
        .spawn((
            Anim::new(5, Vec3::ZERO),
            Transform::default(),
            Visibility::default(),
        ))
        .id();
    let mut queue = bevy::ecs::world::CommandQueue::default();
    let rig = {
        let world = app.world_mut();
        let mut meshes = Assets::<Mesh>::default();
        let rig = spawn(
            &mut Commands::new(&mut queue, world),
            &mut library,
            &mut meshes,
            root,
            &look,
        );
        queue.apply(world);
        rig
    };
    let watched: Vec<Entity> = [J::HandL, J::HandR, J::Head, J::Neck]
        .into_iter()
        .filter_map(|j| rig.joint(j))
        .collect();
    app.world_mut().entity_mut(root).insert(rig);
    let mut worst = 0.0f32;
    let mut last: Vec<Vec3> = Vec::new();
    // Each start is timed so the action ends while a fidget is under way.
    for frame in 0..900 {
        if frame % 150 == 40 {
            app.world_mut()
                .get_mut::<Anim>(root)
                .unwrap()
                .act(Move::Focus);
        }
        app.update();
        let now: Vec<Vec3> = watched
            .iter()
            .map(|e| {
                app.world()
                    .get::<GlobalTransform>(*e)
                    .unwrap()
                    .translation()
            })
            .collect();
        if !last.is_empty() && frame % 150 > 100 {
            let jump = now
                .iter()
                .zip(&last)
                .map(|(a, b)| a.distance(*b))
                .fold(0.0, f32::max);
            worst = worst.max(jump);
        }
        last = now;
    }
    worst
}

#[test]
fn ending_an_action_hands_back_to_the_idle_without_a_snap() {
    for model in [
        CreatureModel::Human,
        CreatureModel::Mage,
        CreatureModel::Vampire,
        CreatureModel::Quagga,
    ] {
        let pop = action_exit_pop(model);
        assert!(pop < 0.12, "{model:?} snaps back to its idle: {pop}");
    }
}

#[test]
fn at_rest_women_stand_with_their_weight_on_one_leg() {
    let tilt = |woman: bool| {
        let look = Look::with_choice(
            CreatureModel::Human,
            9,
            Some(Variant {
                woman,
                ..Default::default()
            }),
        );
        let (mut app, root) = animate_look(look, 1, |_| Vec3::ZERO);
        let hips = app
            .world()
            .get::<Rig>(root)
            .unwrap()
            .joint(J::Hips)
            .unwrap();
        let mut most = 0.0f32;
        for _ in 0..2000 {
            app.update();
            let rotation = app.world().get::<Transform>(hips).unwrap().rotation;
            most = most.max(rotation.to_euler(EulerRot::ZXY).0.abs());
        }
        most
    };
    let (hers, his) = (tilt(true), tilt(false));
    assert!(hers > 0.08, "her hip rises over the standing leg: {hers}");
    assert!(his < hers * 0.7, "he stands squarer: {his} against {hers}");
}

#[test]
fn walking_women_sway_their_hips_and_men_do_not() {
    let sway = |woman: bool| {
        let look = Look::with_choice(
            CreatureModel::Human,
            9,
            Some(Variant {
                woman,
                ..Default::default()
            }),
        );
        let (mut app, root) = animate_look(look, 1, |_| Vec3::ZERO);
        let hips = app
            .world()
            .get::<Rig>(root)
            .unwrap()
            .joint(J::Hips)
            .unwrap();
        let (mut low, mut high) = (f32::INFINITY, f32::NEG_INFINITY);
        for frame in 1..240 {
            app.world_mut()
                .get_mut::<Transform>(root)
                .unwrap()
                .translation = Vec3::Z * frame as f32 * 0.03;
            app.update();
            if frame > 60 {
                let roll = app
                    .world()
                    .get::<Transform>(hips)
                    .unwrap()
                    .rotation
                    .to_euler(EulerRot::ZXY)
                    .0;
                low = low.min(roll);
                high = high.max(roll);
            }
        }
        high - low
    };
    let (hers, his) = (sway(true), sway(false));
    assert!(hers > 0.1, "her hips sway as she walks: {hers}");
    assert!(his < hers * 0.6, "his stay squarer: {his} against {hers}");
}

#[test]
fn women_of_the_heeled_kinds_wear_heels_and_others_do_not() {
    for (style, heeled) in [
        (Style::Vampire, true),
        (Style::Medusa, true),
        (Style::Leprechaun, true),
        (Style::Hero, false),
        (Style::Nymph, false),
        (Style::Troll, false),
    ] {
        assert_eq!(geometry::heeled_boots(style, true), heeled, "{style:?}");
        assert!(
            !geometry::heeled_boots(style, false),
            "{style:?} men stay flat-footed"
        );
    }
}

#[test]
fn skirts_hug_the_waist_and_clear_the_thighs_for_every_figure() {
    // The spawned skirt, swept around the body in 20-degree sectors, is never
    // inside the spawned thighs anywhere down their length. The front sector
    // is left out, where some skirts are split to show the legs.
    let sectors = 18;
    let in_sector = |p: &Vec3, k: usize| {
        let lo = -PI + TAU * k as f32 / sectors as f32;
        (lo..lo + TAU / sectors as f32).contains(&p.x.atan2(p.z))
    };
    let reach = |points: &[Vec3], k: usize, y: f32| {
        points
            .iter()
            .filter(|p| (y - 0.05..y).contains(&p.y) && in_sector(p, k))
            .map(|p| Vec2::new(p.x, p.z).length())
            .fold(None, |m: Option<f32>, r| Some(m.map_or(r, |m| m.max(r))))
    };
    for model in [
        CreatureModel::Medusa,
        CreatureModel::Human,
        CreatureModel::Leprechaun,
        CreatureModel::Nymph,
    ] {
        for woman in [false, true] {
            for build in 0..3 {
                let look = Look::with_choice(
                    model,
                    1,
                    Some(Variant {
                        woman,
                        build,
                        ..Default::default()
                    }),
                );
                let (world, rig, _, meshes) = spawn_one(&look);
                let placed = |e: Entity| {
                    let at = world.get::<Transform>(e).unwrap().compute_affine();
                    points_under(&world, &meshes, e)
                        .into_iter()
                        .map(move |p| at.transform_point3(p))
                };
                let thighs: Vec<Vec3> = [J::LegL, J::LegR]
                    .into_iter()
                    .flat_map(|j| placed(rig.joint(j).expect("legs")))
                    .collect();
                // The tunic's skirt: longer than any jerkin's peplum over it.
                let skirt = rig
                    .drapes
                    .iter()
                    .filter_map(|&(e, kind)| match kind {
                        Drape::Skirt(length) => Some((e, length)),
                        Drape::Cloak | Drape::Hair => None,
                    })
                    .max_by(|a, b| a.1.total_cmp(&b.1))
                    .map(|(e, _)| placed(e).collect::<Vec<_>>())
                    .expect("a skirt");
                // The pelvis's own meshes, without the legs and drapes it carries.
                let hips = rig.joint(J::Hips).expect("hips");
                let pelvis: Vec<Vec3> = world
                    .get::<Children>(hips)
                    .expect("pelvis")
                    .iter()
                    .filter(|&child| world.get::<Mesh3d>(child).is_some())
                    .flat_map(|child| {
                        let at = world.get::<Transform>(child).unwrap().compute_affine();
                        points_under(&world, &meshes, child)
                            .into_iter()
                            .map(move |p| at.transform_point3(p))
                    })
                    .collect();
                // The waistband sits on the hips, not standing off them.
                for k in 0..sectors {
                    let band = |points: &[Vec3]| {
                        points
                            .iter()
                            .filter(|p| (0.025..0.045).contains(&p.y) && in_sector(p, k))
                            .map(|p| Vec2::new(p.x, p.z).length())
                            .reduce(f32::max)
                    };
                    if let (Some(cloth), Some(body)) = (band(&skirt), band(&pelvis)) {
                        assert!(
                            cloth < body + 0.015,
                            "{model:?} woman {woman} build {build}: waistband {cloth:.3} stands off the hips {body:.3}, sector {k}"
                        );
                    }
                }
                // Cloth flares only as far as it needs to: never into a bell.
                // Measured behind, where no split parts the front panels.
                let flare = skirt
                    .iter()
                    .filter(|p| p.z < 0.0)
                    .map(|p| p.x.abs())
                    .fold(0.0f32, f32::max);
                // The body's hip line: the pelvis and the thigh roots below it.
                let hips = pelvis
                    .iter()
                    .chain(thighs.iter().filter(|p| p.y > -0.1))
                    .map(|p| p.x.abs())
                    .fold(0.0f32, f32::max);
                assert!(
                    flare < hips * 1.55,
                    "{model:?} woman {woman} build {build}: skirt flares to {flare:.3} over hips {hips:.3}"
                );
                let hem = skirt.iter().fold(0.0f32, |m, p| m.min(p.y));
                let mut y = 0.0;
                // A cutaway coat's fronts part around the legs: only its sides
                // and back must clear them.
                let open = |k: usize| {
                    let mid = -PI + TAU * (k as f32 + 0.5) / sectors as f32;
                    k == sectors / 2 || (geometry::cutaway_coat(look.style) && mid.cos() > 0.1)
                };
                while y > hem + 0.05 {
                    for k in (0..sectors).filter(|&k| !open(k)) {
                        if let (Some(cloth), Some(leg)) =
                            (reach(&skirt, k, y), reach(&thighs, k, y))
                        {
                            assert!(
                                cloth > leg,
                                "{model:?} woman {woman} build {build}: thigh {leg:.3} through skirt {cloth:.3} at height {y:.2}, sector {k}"
                            );
                        }
                    }
                    y -= 0.05;
                }
            }
        }
    }
}

/// Every mesh vertex under `root`, in `root`'s own space.
fn points_under(world: &World, meshes: &Assets<Mesh>, root: Entity) -> Vec<Vec3> {
    let mut out = Vec::new();
    let mut stack = vec![(root, Affine3A::IDENTITY)];
    while let Some((e, to_root)) = stack.pop() {
        if let Some(mesh) = world.get::<Mesh3d>(e).and_then(|m| meshes.get(&m.0)) {
            if let Some(bevy::render::mesh::VertexAttributeValues::Float32x3(positions)) =
                mesh.attribute(Mesh::ATTRIBUTE_POSITION)
            {
                out.extend(
                    positions
                        .iter()
                        .map(|v| to_root.transform_point3(Vec3::from(*v))),
                );
            }
        }
        if let Some(children) = world.get::<Children>(e) {
            for child in children.iter() {
                let local = world
                    .get::<Transform>(child)
                    .map_or(Affine3A::IDENTITY, |t| t.compute_affine());
                stack.push((child, to_root * local));
            }
        }
    }
    out
}

#[test]
fn every_dressable_humanoid_wears_every_outfit() {
    // Clothes and armour are cut for the wearer, not its kind: every
    // humanoid that wears separable clothes can be dressed from the whole
    // wardrobe, and each outfit brings exactly its own garments.
    // Kinds that differ in what they natively wear: plate, a jerkin, a
    // coat, bare limbs, rags and a ghost's translucent body.
    for model in [
        CreatureModel::Human,
        CreatureModel::Sentinel,
        CreatureModel::Vampire,
        CreatureModel::Troll,
        CreatureModel::Leprechaun,
        CreatureModel::Nymph,
        CreatureModel::Medusa,
        CreatureModel::Phantom,
    ] {
        for woman in [false, true] {
            let look = Look::with_choice(
                model,
                2,
                Some(Variant {
                    woman,
                    ..Default::default()
                }),
            );
            assert!(look.dresses(), "{model:?} wears separable clothes");
            assert_eq!(look.clone().dressed(Wardrobe::Native).key, look.key);
            let mut keys = BTreeSet::new();
            for wardrobe in Wardrobe::ALL {
                let dressed = look.clone().dressed(wardrobe);
                assert!(
                    keys.insert(dressed.key.clone()),
                    "{wardrobe:?} is cached apart"
                );
                let outfit = outfit::Outfit::worn(&dressed);
                let (world, rig, library, meshes) = spawn_one(&dressed);
                // What the body grows stays with it whatever it wears.
                if model == CreatureModel::Medusa {
                    assert!(!rig.segments.is_empty(), "{wardrobe:?} keeps her serpents");
                }
                for (_, mesh) in meshes.iter() {
                    if let Some(bevy::render::mesh::VertexAttributeValues::Float32x3(ps)) =
                        mesh.attribute(Mesh::ATTRIBUTE_POSITION)
                    {
                        assert!(ps.iter().flatten().all(|v| v.is_finite()));
                    }
                }
                let cloaks = rig
                    .drapes
                    .iter()
                    .filter(|(_, kind)| *kind == Drape::Cloak)
                    .count();
                let skirts = rig
                    .drapes
                    .iter()
                    .filter(|(_, kind)| matches!(kind, Drape::Skirt(_)))
                    .count();
                if wardrobe != Wardrobe::Native {
                    assert_eq!(
                        cloaks,
                        usize::from(outfit.cloak.is_some()),
                        "{model:?} {wardrobe:?} cloak"
                    );
                    assert!(skirts >= usize::from(outfit.skirt.is_some()));
                }
                // Plate is steel on the arms and legs; cloth outfits have none
                // there. A ghost's whole body below the head is translucent.
                if dressed.style.ghostly() {
                    continue;
                }
                let metal = library.materials.metal().clone();
                let steel_on = |joint: J| {
                    let e = rig.joint(joint).expect("limb");
                    world
                        .get::<Children>(e)
                        .into_iter()
                        .flat_map(|children| children.iter())
                        .any(|child| {
                            world
                                .get::<MeshMaterial3d<StandardMaterial>>(child)
                                .is_some_and(|m| m.0 == metal)
                        })
                };
                let plated = outfit.arms == outfit::Arms::Plate;
                assert_eq!(steel_on(J::ArmL), plated, "{model:?} {wardrobe:?} arms");
                assert_eq!(
                    steel_on(J::LegL),
                    outfit.legs == outfit::Legs::Plate,
                    "{model:?} {wardrobe:?} legs"
                );
            }
        }
    }
}

#[test]
fn long_hair_hangs_where_it_did_and_swings_smoothly_as_she_moves() {
    let look = Look::with_choice(
        CreatureModel::Nymph,
        9,
        Some(Variant {
            woman: true,
            ..Default::default()
        }),
    );
    assert!(
        geometry::loose_fall(&look),
        "the nymph wears her hair long and loose"
    );
    // At rest the fall sits exactly where it would on the head: its own
    // part, pivoted at the crown, carries the head's proportions.
    let (world, rig, _, meshes) = spawn_one(&look);
    let (hair, _) = *rig
        .drapes
        .iter()
        .find(|(_, kind)| *kind == Drape::Hair)
        .expect("long hair hangs on its own");
    let at = world.get::<Transform>(hair).unwrap().compute_affine();
    let placed: Vec<Vec3> = points_under(&world, &meshes, hair)
        .into_iter()
        .map(|p| at.transform_point3(p))
        .collect();
    let mut on_head = Parts::default();
    on_head.set_transform(
        Affine3A::from_translation(Vec3::Y * 0.08)
            * Affine3A::from_scale(Vec3::splat(1.1))
            * Affine3A::from_translation(Vec3::Y * -0.08),
    );
    geometry::long_hair_form_for(&mut on_head, &look);
    let (low, high) = on_head.matte.bounds();
    let scale = anatomy::storybook_part(Plan::Biped, J::Head);
    let (low, high) = (low * scale, high * scale);
    let bounds = placed.iter().fold((Vec3::MAX, Vec3::MIN), |(lo, hi), p| {
        (lo.min(*p), hi.max(*p))
    });
    assert!(
        (bounds.0 - low).abs().max_element() < 1e-4 && (bounds.1 - high).abs().max_element() < 1e-4,
        "{bounds:?} against {low} {high}"
    );
    // Walking, then turning a right angle: it trails and lifts with each
    // step, more through the turn, and never jumps between frames.
    let path = |f: usize| {
        if f < 70 {
            Vec3::Z * f as f32 * 0.03
        } else {
            Vec3::Z * 70.0 * 0.03 + Vec3::X * (f - 70) as f32 * 0.03
        }
    };
    let (mut app, root) = animate_look(look, 1, path);
    let hair = app
        .world()
        .get::<Rig>(root)
        .unwrap()
        .drapes
        .iter()
        .find(|(_, kind)| *kind == Drape::Hair)
        .unwrap()
        .0;
    let mut previous = Quat::IDENTITY;
    let (mut walking, mut turning, mut jump) = (0.0f32, 0.0f32, 0.0f32);
    for frame in 1..140 {
        app.world_mut()
            .get_mut::<Transform>(root)
            .unwrap()
            .translation = path(frame);
        app.update();
        let rotation = app.world().get::<Transform>(hair).unwrap().rotation;
        if frame > 1 {
            jump = jump.max(rotation.angle_between(previous));
        }
        previous = rotation;
        let swing = rotation.angle_between(Quat::IDENTITY);
        if (40..70).contains(&frame) {
            walking = walking.max(swing);
        } else if (70..100).contains(&frame) {
            turning = turning.max(swing);
        }
    }
    assert!(walking > 0.06, "her hair trails as she walks: {walking}");
    assert!(
        turning > walking,
        "and swings out through a turn: {turning} {walking}"
    );
    assert!(jump < 0.03, "and never jumps between frames: {jump}");
    // Cloth on the same walk and turn eases into its swing just as smoothly.
    let (mut app, root) = animate_along(CreatureModel::Human, 1, path);
    let drapes = app.world().get::<Rig>(root).unwrap().drapes.clone();
    let mut previous: Vec<Quat> = vec![Quat::IDENTITY; drapes.len()];
    for frame in 1..140 {
        app.world_mut()
            .get_mut::<Transform>(root)
            .unwrap()
            .translation = path(frame);
        app.update();
        for (k, &(e, kind)) in drapes.iter().enumerate() {
            let rotation = app.world().get::<Transform>(e).unwrap().rotation;
            if frame > 1 {
                let jump = rotation.angle_between(previous[k]);
                assert!(jump < 0.04, "{kind:?} jumps {jump} at frame {frame}");
            }
            previous[k] = rotation;
        }
    }
}
