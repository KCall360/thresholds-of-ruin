use std::num::NonZeroU64;
use tor_simulation::attributes::Skill;
use tor_simulation::creatures::{GrantSource, Template};
use tor_simulation::grants::{Ability, Grant};
use tor_simulation::progression::{Class, HdSource};
use tor_simulation::resources::Resource;
use tor_simulation::talents::Talent;
use tor_simulation::{Action, ActorId, CreatureIdentity, Game, IntentionOrigin};
use tor_world::{Location, Position, RegionId};

fn game() -> Game {
    let mut game = Game::two_room_in_stone(42);
    for x in 1..=2 {
        let actor = game
            .spawn_actor(
                Location {
                    region: RegionId(1),
                    position: Position { x, y: 1, z: 0 },
                },
                NonZeroU64::new(100).unwrap(),
            )
            .unwrap();
        let mut build = super::creature_state::build();
        build.train(1, Skill::Spellcasting).unwrap();
        build.train(1, Skill::Intimidation).unwrap();
        build.select_talent(0, Talent::MagicBolt).unwrap();
        build.select_talent(1, Talent::Fear).unwrap();
        game.configure_creature(
            actor,
            CreatureIdentity {
                name: format!("subject {x}"),
                faction: format!("team{x}"),
            },
            build,
        )
        .unwrap();
    }
    game.refresh_navigation();
    game
}

#[test]
fn trusted_inspection_preserves_paid_preparation_and_exposes_owned_sources() {
    let mut game = game();
    game.admit_intention(
        ActorId(1),
        Action::UseAbility {
            ability: Ability::MagicBolt,
            target: ActorId(2),
        },
        IntentionOrigin::Human,
    )
    .unwrap();
    game.execute_next_intention().unwrap().outcome.unwrap();
    let before = game.clone();
    let inspection = game.inspect_creature(ActorId(1)).unwrap();
    assert_eq!(inspection.identity.name, "subject 1");
    assert_eq!(inspection.identity.faction, "team1");
    assert_eq!(
        inspection.stats.hit_dice,
        [
            HdSource::Class(Class::Warrior),
            HdSource::Class(Class::Mage)
        ]
    );
    assert_eq!(
        inspection.creature.build().choices()[0].talent,
        Some(Talent::MagicBolt)
    );
    assert_eq!(
        inspection.creature.build().choices()[1].training,
        [Skill::Spellcasting, Skill::Intimidation]
    );
    assert!(
        inspection.creature.derived().grants[&GrantSource::Class(Class::Mage)]
            .contains(&Grant::Magical)
    );
    assert_eq!(inspection.creature.costs().reservations().len(), 1);
    assert_eq!(
        inspection
            .stats
            .resources
            .iter()
            .find(|pool| pool.resource == Resource::Mana)
            .unwrap()
            .reserved,
        1
    );
    assert_eq!(game, before);
    assert!(game.inspect_creature(ActorId(999)).is_none());
    assert_eq!(game, before);
}

#[test]
fn inspection_reflects_dormancy_transformation_and_persistent_death() {
    let mut game = game();
    let mut build = game.creature(ActorId(1)).unwrap().build().clone();
    let retained = build.ledger().entries().to_vec();
    build.set_templates(vec![Template::zombified(10)]).unwrap();
    game.rebuild_creature(ActorId(1), build.clone()).unwrap();
    let inspection = game.inspect_creature(ActorId(1)).unwrap();
    assert_eq!(inspection.creature.build().ledger().entries(), retained);
    assert_eq!(inspection.creature.build().templates()[0].id, "zombified");
    assert!(inspection.stats.dormant_talents.contains(&Talent::Fear));
    assert!(!inspection.stats.active_talents.contains(&Talent::Fear));
    assert!(inspection.stats.active_talents.contains(&Talent::MagicBolt));
    build.set_templates(vec![]).unwrap();
    game.rebuild_creature(ActorId(1), build.clone()).unwrap();
    let restored = game.inspect_creature(ActorId(1)).unwrap();
    assert!(restored.stats.active_talents.contains(&Talent::Fear));
    assert!(!restored.stats.dormant_talents.contains(&Talent::Fear));
    while build.remove_latest().is_some() {}
    game.rebuild_creature(ActorId(1), build).unwrap();
    let before = game.clone();
    let inspection = game.inspect_creature(ActorId(1)).unwrap();
    assert!(inspection.creature.health().dead());
    assert_eq!(inspection.creature.health().maximum(), 0);
    assert_eq!(inspection.creature.health().current(), 0);
    assert!(inspection.stats.hit_dice.is_empty());
    assert_eq!(inspection.identity.name, "subject 1");
    assert_eq!(game, before);
}
