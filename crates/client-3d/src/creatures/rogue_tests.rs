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
    let look = Look::of(model, 9);
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
