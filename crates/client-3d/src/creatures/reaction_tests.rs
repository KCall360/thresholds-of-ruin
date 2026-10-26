use super::*;
use crate::{props::PropMaterials, ActorBody, Game};
use tor_client_3d_model::{Actor, Happening};
use tor_protocol::{AbilityOutcome, ActorTarget, Position};

fn body(app: &mut App, at: Vec3) -> Entity {
    let label = app.world_mut().spawn_empty().id();
    let ring = app.world_mut().spawn_empty().id();
    app.world_mut()
        .spawn((
            ActorBody {
                appearance_seed: None,
                appearance_key: "fixture".into(),
                model: CreatureModel::Placeholder,
                wearing: Default::default(),
                hostile: false,
                goal: at,
                label,
                ring,
                name: String::new(),
                height: 1.0,
            },
            Anim::new(0, at),
        ))
        .id()
}

fn scene(id: &str, name: &str, show_target: bool, disclose_target: bool) -> (App, Entity, Entity) {
    let mut app = App::new();
    let mut materials = Assets::<StandardMaterial>::default();
    let library = PropLibrary::new(PropMaterials::new(&mut materials));
    app.init_resource::<Game>()
        .init_resource::<Assets<Mesh>>()
        .insert_resource(materials)
        .insert_resource(library)
        .add_systems(Update, crate::scene::react);
    let caster = ActorTarget::from_digest([11; 32]);
    let target = ActorTarget::from_digest([12; 32]);
    let caster_entity = body(&mut app, Vec3::ZERO);
    let target_entity = body(&mut app, Vec3::X);
    let mut view = crate::test_support::view(1);
    view.hp = Some((12, 12));
    view.actors = [(caster, 0), (target, 1)]
        .into_iter()
        .filter(|(id, _)| show_target || *id == caster)
        .map(|(id, x)| Actor {
            appearance_seed: None,
            model: CreatureModel::Placeholder,
            wearing: Default::default(),
            id,
            name: String::new(),
            description: String::new(),
            position: Position { x, y: 0, z: 0 },
            hostile: false,
            injury: None,
            is_self: id == caster,
        })
        .collect();
    {
        let mut game = app.world_mut().resource_mut::<Game>();
        game.view = Some(view);
        game.actors.insert(caster, caster_entity);
        if show_target {
            game.actors.insert(target, target_entity);
        }
        game.pending.push(Happening::Ability {
            caster: Some(caster),
            target: disclose_target.then_some(target),
            id: id.into(),
            name: name.into(),
            outcome: AbilityOutcome::Applied,
            strike: None,
        });
    }
    (app, caster_entity, target_entity)
}

#[test]
fn an_applied_effect_without_a_harm_witness_does_not_invent_a_flinch() {
    let (mut app, caster_entity, target_entity) =
        scene("fixture_support", "Gentle benediction", true, true);
    app.update();
    assert!(
        app.world()
            .get::<Anim>(caster_entity)
            .unwrap()
            .action
            .is_some(),
        "the applied effect must still animate its caster"
    );
    assert!(
        app.world()
            .get::<Anim>(target_entity)
            .unwrap()
            .hurt
            .is_none(),
        "applied is not evidence of damage or an injury change"
    );
}

#[test]
fn an_undisclosed_target_does_not_get_a_fabricated_projectile_endpoint() {
    let (mut app, caster, _) = scene("spark_lance_fire", "Spark Lance (Fire)", false, false);
    app.update();
    assert!(app.world().get::<Anim>(caster).unwrap().action.is_some());
    let mut bolts = app.world_mut().query::<&Bolt>();
    assert_eq!(
        bolts.iter(app.world()).count(),
        0,
        "no target position was disclosed"
    );
}

fn pose(id: &str, name: &str) -> Move {
    let (mut app, caster, _) = scene(id, name, true, true);
    app.update();
    app.world().get::<Anim>(caster).unwrap().action.unwrap().0
}

#[test]
fn renaming_a_disclosed_ability_does_not_change_its_motion_recipe() {
    assert_eq!(
        pose("basic_melee", "Spark Lance (Fire)"),
        pose("basic_melee", "Bind Wounds")
    );
}

#[test]
fn known_techniques_keep_distinct_motions_even_with_the_same_caption() {
    assert_ne!(
        pose("basic_melee", "A technique"),
        pose("bind_wounds", "A technique")
    );
}

#[test]
fn a_disclosed_harm_witness_animates_at_the_effects_arrival() {
    let (mut app, _, target_entity) = scene("spark_lance_fire", "A technique", true, true);
    let target = ActorTarget::from_digest([12; 32]);
    app.world_mut()
        .resource_mut::<Game>()
        .pending
        .push(Happening::Hurt(target));
    app.update();
    assert!(app
        .world()
        .get::<Anim>(target_entity)
        .unwrap()
        .hurt
        .is_some_and(|t| t < -0.32));
    let mut bolts = app.world_mut().query::<&Bolt>();
    assert_eq!(
        bolts.iter(app.world()).count(),
        1,
        "known target keeps its projectile"
    );
}

#[test]
fn an_unfamiliar_technique_keeps_a_neutral_motion_regardless_of_caption() {
    assert_eq!(pose("custom_support", "Spark Lance (Fire)"), Move::Gesture);
    assert_eq!(pose("custom_support", "Bind Wounds"), Move::Gesture);
}
