use std::{
    collections::{BTreeMap, BTreeSet},
    num::NonZeroU64,
};
use tor_simulation::{
    ai::AiProfile,
    creatures::Template,
    grants::{Ability, Grant},
    Action, ActorId, CreatureIdentity, Game,
};
use tor_world::{Location, Position, RegionId};

fn fixture(abilities: &[Ability], distance: i32) -> (Game, ActorId, ActorId) {
    let mut game = Game::two_room_in_stone(42);
    let at = |x| Location {
        region: RegionId(1),
        position: Position { x, y: 1, z: 0 },
    };
    let human = game
        .spawn_actor(at(2 + distance), NonZeroU64::new(100).unwrap())
        .unwrap();
    let mob = game
        .spawn_actor(at(2), NonZeroU64::new(100).unwrap())
        .unwrap();
    let mut human_build = super::creature_state::build();
    let mut health = Template::new("target_health", 0);
    health.grants = vec![Grant::Health(200)];
    human_build.set_templates(vec![health]).unwrap();
    game.configure_creature(
        human,
        CreatureIdentity {
            name: "human".into(),
            faction: "hero".into(),
        },
        human_build,
    )
    .unwrap();
    let mut build = super::creature_state::build();
    let mut template = Template::new("ai_techniques", 0);
    template.grants = abilities.iter().copied().map(Grant::Ability).collect();
    build.set_templates(vec![template]).unwrap();
    game.configure_creature(
        mob,
        CreatureIdentity {
            name: "mob".into(),
            faction: "foe".into(),
        },
        build,
    )
    .unwrap();
    game.configure_ai(
        mob,
        AiProfile {
            flee_percent: 0,
            ..Default::default()
        },
    )
    .unwrap();
    game.configure_run(
        human,
        BTreeSet::from([human]),
        None,
        BTreeMap::from([("foe".into(), BTreeSet::from(["hero".into()]))]),
    )
    .unwrap();
    game.refresh_navigation();
    // A waiting external actor holds the clock at the preparation boundary,
    // allowing tests to inspect/cancel work before fast-forward resolution.
    game.spawn_actor(
        Location {
            region: RegionId(1),
            position: Position { x: 1, y: 0, z: 0 },
        },
        NonZeroU64::new(100).unwrap(),
    )
    .unwrap();
    game.act(human, Action::Wait).unwrap();
    (game, human, mob)
}

#[test]
fn visible_creature_ai_selects_granted_paid_melee_without_mutating_or_spending() {
    let (game, human, mob) = fixture(&[Ability::PowerStrike], 1);
    let before = game.clone();
    assert_eq!(
        game.next_ai_action(),
        Some((
            mob,
            Action::UseAbility {
                ability: Ability::PowerStrike,
                target: human
            }
        ))
    );
    assert_eq!(game, before);
}

#[test]
fn ranged_creature_ai_casts_only_at_currently_visible_targets() {
    let (mut game, human, mob) = fixture(&[Ability::MagicBolt], 2);
    assert_eq!(
        game.next_ai_action(),
        Some((
            mob,
            Action::UseAbility {
                ability: Ability::MagicBolt,
                target: human
            }
        ))
    );
    game.set_wall(
        Location {
            region: RegionId(1),
            position: Position { x: 3, y: 1, z: 0 },
        },
        true,
    )
    .unwrap();
    assert!(!matches!(
        game.next_ai_action(),
        Some((_, Action::UseAbility { .. }))
    ));
}

#[test]
fn cancelled_paid_ai_preparations_reduce_available_funding_and_fall_back_to_basic_melee() {
    let (mut game, human, mob) = fixture(&[Ability::PowerStrike], 1);
    for _ in 0..3 {
        let intention = game.admit_ai_intention(mob).unwrap();
        let execution = game.execute_next_intention().unwrap();
        assert_eq!(
            execution.action,
            Some(Action::UseAbility {
                ability: Ability::PowerStrike,
                target: human
            })
        );
        execution.outcome.unwrap();
        game.cancel_intention(mob, intention).unwrap();
    }
    assert_eq!(
        game.creature(mob)
            .unwrap()
            .costs()
            .available(tor_simulation::resources::Resource::Stamina),
        1
    );
    assert_eq!(
        game.next_ai_action(),
        Some((mob, Action::Attack { target: human }))
    );
}

#[test]
fn autonomous_fear_preparation_restores_its_cost_and_does_not_repeat_until_focus_recovers() {
    use tor_simulation::{checkpoint::SharedState, resources::Resource};
    let (mut game, human, mob) = fixture(
        &[Ability::Fear, Ability::PowerStrike, Ability::MagicBolt],
        1,
    );
    let intention = game.admit_ai_intention(mob).unwrap();
    let execution = game.execute_next_intention().unwrap();
    execution.outcome.as_ref().expect("AI fear execution");
    assert_eq!(
        execution.action,
        Some(Action::UseAbility {
            ability: Ability::Fear,
            target: human
        })
    );
    execution.outcome.unwrap();
    let costs = game.creature(mob).unwrap().costs();
    assert_eq!(costs.resources().balance(Resource::Focus), 3);
    assert_eq!(costs.available(Resource::Focus), 2);
    let mut shared = SharedState::default();
    let snapshot =
        serde_json::from_value(serde_json::to_value(game.checkpoint(&mut shared)).unwrap())
            .unwrap();
    let shared = serde_json::from_value(serde_json::to_value(shared).unwrap()).unwrap();
    let mut restored = Game::restore_checkpoint(snapshot, &shared).unwrap();
    assert_eq!(
        restored.preparation(mob).unwrap().origin_intention(),
        Some(intention)
    );
    for _ in 0..8 {
        if restored.preparation(mob).is_none() && restored.next_actor() == Some(mob) {
            break;
        }
        restored
            .act(restored.next_actor().unwrap(), Action::Wait)
            .unwrap();
    }
    assert!(restored.preparation(mob).is_none());
    let costs = restored.creature(mob).unwrap().costs();
    assert_eq!(costs.resources().balance(Resource::Focus), 2);
    assert_eq!(costs.available(Resource::Focus), 2);
    assert_eq!(
        restored.next_ai_action(),
        Some((
            mob,
            Action::UseAbility {
                ability: Ability::PowerStrike,
                target: human
            }
        ))
    );
}

#[test]
fn interrupted_ai_resumes_its_funded_fear_without_a_second_start_charge() {
    let (mut game, human, mob) = fixture(&[Ability::Fear, Ability::PowerStrike], 1);
    let original = game.admit_ai_intention(mob).unwrap();
    game.execute_next_intention().unwrap().outcome.unwrap();
    game.apply_effects(
        mob,
        &[tor_simulation::EffectSpec::Damage {
            components: BTreeMap::from([(tor_simulation::combat::DamageType::Vital, 1)]),
        }],
    )
    .unwrap();
    assert!(!game.preparation(mob).unwrap().active);
    assert_eq!(
        game.next_ai_action(),
        Some((
            mob,
            Action::UseAbility {
                ability: Ability::Fear,
                target: human
            }
        ))
    );
    let before = game.creature(mob).unwrap().costs().clone();
    let latest = game.admit_ai_intention(mob).unwrap();
    game.execute_next_intention().unwrap().outcome.unwrap();
    let preparation = game.preparation(mob).unwrap();
    assert_eq!(preparation.origin_intention(), Some(original));
    assert_eq!(preparation.intention, Some(latest));
    assert_eq!(game.creature(mob).unwrap().costs(), &before);
}
