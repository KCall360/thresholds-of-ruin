//! Direction choice never asks geometry to predict a future ray collision.
use super::*;
use crate::abilities::{AbilityInvocation, InvocationKind, StepDirection};
use crate::rules::{
    Against, CheckActor, CheckDef, Modifier, ModifierOp, RangePreference, Scope, SkillSelector,
    StrikeSource,
};
use tor_world::{Position, RegionId, SightCell};

fn fixture() -> Game {
    let mut definition = crate::test_creatures::mana_ray();
    definition.invocation = InvocationKind::DirectionalRay;
    definition.cost = None;
    definition.ai.priority = 100;
    definition.ai.range_preference = RangePreference::Any;
    let StrikeSource::Authored { strikes } = &mut definition.strikes else {
        unreachable!()
    };
    let strike = &mut strikes[0];
    strike.check = Some(CheckDef {
        actor: CheckActor::Target,
        skill: SkillSelector::Named {
            skill: crate::attributes::Skill::Agility,
        },
        against: Against::Opposed {
            skill: SkillSelector::Named {
                skill: crate::attributes::Skill::Spellcasting,
            },
        },
    });
    strike.on_failure = std::mem::take(&mut strike.on_success);
    let mut document = crate::test_creatures::ruleset().document().clone();
    let mut foe = crate::test_creatures::species();
    foe.modifiers.push(Modifier {
        scope: Scope::Always,
        op: ModifierOp::GrantAbility {
            ability: definition.id.clone(),
        },
    });
    document.abilities.push(definition);
    let mut game = super::tests::configured_game(
        crate::test_creatures::species(),
        foe,
        crate::rules::Ruleset::validate(document).unwrap(),
    );
    game.teleport(
        ActorId(1),
        Location {
            region: RegionId(1),
            position: Position { x: 4, y: 1, z: 0 },
        },
    )
    .unwrap();
    game
}

#[test]
fn hostile_ai_selects_disclosed_alignment_without_target_or_hidden_collision_preview() {
    let game = fixture();
    let mut twin = game.clone();
    twin.add_region(tor_world::Region {
        id: RegionId(3),
        name: "undisclosed".into(),
        bounds: tor_world::Extent::new(8, 8, 4).unwrap(),
    })
    .unwrap();
    let hidden = Location {
        region: RegionId(3),
        position: Position { x: 2, y: 2, z: 0 },
    };
    twin.set_wall(hidden, true).unwrap();
    twin.spawn_actor(
        Location {
            position: Position { x: 3, y: 3, z: 0 },
            ..hidden
        },
        std::num::NonZeroU64::new(100).unwrap(),
    )
    .unwrap();
    assert_eq!(
        game.observe(ActorId(2)).unwrap(),
        twin.observe(ActorId(2)).unwrap()
    );
    let before = game.clone();
    let twin_before = twin.clone();
    let selected = game.choose_ai(ActorId(2)).unwrap().action;
    assert!(matches!(
        selected,
        Action::UseAbility {
            invocation: AbilityInvocation::DirectionalRay {
                direction: StepDirection::East
            },
            ..
        }
    ));
    assert_eq!(selected, twin.choose_ai(ActorId(2)).unwrap().action);
    assert_eq!(game, before);
    assert_eq!(twin, twin_before);
}

#[test]
fn disclosed_ray_alignment_rejects_undisclosed_contacts_stairs_and_inexact_vectors() {
    let game = fixture();
    let mut view = game.observe(ActorId(2)).unwrap();
    let target = ActorId(1);
    let target_location = view
        .visible_actors
        .iter()
        .find(|other| other.id == target)
        .unwrap()
        .location;
    let cell = SightCell {
        physical: true,
        location: target_location,
        offset: Position { x: 4, y: 0, z: -1 },
        rotation: 0,
        wall: false,
    };
    // Original decision cells are eye-relative. Add the known eye offset to aim
    // from the source's reference cell, without consulting hidden terrain.
    assert_eq!(
        disclosed_ray_direction(target, &[cell], &view, [0, 0, 1], 6),
        Some(StepDirection::East)
    );
    assert_eq!(
        disclosed_ray_direction(target, &[cell], &view, [0, 0, 1], 3),
        None
    );
    for direction in Direction::HORIZONTAL
        .into_iter()
        .chain([Direction::Up, Direction::Down])
    {
        let (x, y, z) = direction.delta();
        let offset = Position {
            x: 3 * x,
            y: 3 * y,
            z: 3 * z,
        };
        assert_eq!(
            disclosed_ray_direction(target, &[SightCell { offset, ..cell }], &view, [0, 0, 0], 6),
            Some(direction.try_into().unwrap())
        );
    }
    for offset in [
        Position { x: 0, y: 0, z: 0 },
        Position { x: 2, y: 1, z: 0 },
        Position { x: 1, y: 0, z: 1 },
        Position { x: 1, y: 1, z: 1 },
    ] {
        assert_eq!(
            disclosed_ray_direction(target, &[SightCell { offset, ..cell }], &view, [0, 0, 0], 6),
            None
        );
    }
    assert_eq!(
        disclosed_ray_direction(
            target,
            &[SightCell {
                physical: false,
                ..cell
            }],
            &view,
            [0, 0, 1],
            6
        ),
        None
    );
    assert_eq!(
        disclosed_ray_direction(
            target,
            &[SightCell { wall: true, ..cell }],
            &view,
            [0, 0, 1],
            6
        ),
        None
    );
    view.visible_actors
        .iter_mut()
        .find(|other| other.id == target)
        .unwrap()
        .disclosure = crate::perception::ActorDisclosure::AnonymousContact;
    assert_eq!(
        disclosed_ray_direction(target, &[cell], &view, [0, 0, 1], 6),
        None
    );
    view.visible_actors.clear();
    assert_eq!(
        disclosed_ray_direction(target, &[cell], &view, [0, 0, 1], 6),
        None
    );
}

#[test]
fn queued_ray_restores_without_target_and_starts_with_directional_work() {
    let mut game = fixture();
    // The next human decision must precede the ray's completion; a full Wait
    // lets ordinary action scheduling release it before execution returns.
    game.actors.get_mut(&ActorId(1)).unwrap().ready_at = 1;
    let action = game.choose_ai(ActorId(2)).unwrap().action;
    let id = game
        .admit_intention(
            ActorId(2),
            action.clone(),
            crate::intention::IntentionOrigin::Human,
        )
        .unwrap();
    let mut shared = crate::checkpoint::SharedState::default();
    let checkpoint = game.checkpoint(&mut shared);
    let mut restored =
        Game::restore_checkpoint(checkpoint, &shared, game.content().clone()).unwrap();
    assert_eq!(restored, game);
    let execution = restored.execute_next_intention().unwrap();
    assert_eq!(execution.intention.id, id);
    assert_eq!(execution.action, Some(action));
    assert!(matches!(
        execution.outcome.unwrap().kind,
        crate::OutcomeKind::ItemStarted {
            work: crate::Work::UseAbility {
                invocation: AbilityInvocation::DirectionalRay { .. },
                ..
            }
        }
    ));
    assert_eq!(restored.tick, 1);
    assert_eq!(restored.next_actor(), Some(ActorId(1)));
    assert_eq!(restored.rules_rng, game.rules_rng);
    assert!(!restored
        .combat_events()
        .iter()
        .any(|event| matches!(event, crate::combat::CombatEvent::Resolved { .. })));
    let preparation = restored.preparation(ActorId(2)).unwrap();
    assert_eq!(preparation.intention, Some(id));
    assert!(preparation.active);
    assert!(preparation.started + preparation.remaining > restored.tick);
    assert_eq!(preparation.plan.work().target(), None);
}
