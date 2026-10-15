use std::num::NonZeroU64;
use tor_simulation::attributes::Skill;
use tor_simulation::content_ids::{AbilityId, ClassId, FlagId, TalentId, TemplateId};
use tor_simulation::creatures::{SourceId, Template};
use tor_simulation::resources::Resource;
use tor_simulation::rules::{Modifier, ModifierOp, Scope};
use tor_simulation::{Action, ActorId, CreatureIdentity, Game, IntentionOrigin};
use tor_world::{Location, Position, RegionId};

fn game() -> Game {
    let mut template = Template::new(TemplateId::new("mindless_form").unwrap(), 10);
    template.modifiers = vec![Modifier {
        scope: Scope::Always,
        op: ModifierOp::Flag {
            flag: FlagId::new("mindless").unwrap(),
        },
    }];
    let mut base = super::creature_state::with_template(&super::creature_state::build(), template);
    base.set_templates(vec![]).unwrap();
    let mut thinking = super::creature_fixture::talent("fixture_thinking_form", vec![]);
    thinking.requirement = tor_simulation::rules::Requirement::All {
        requirements: vec![
            tor_simulation::rules::Requirement::ClassLevel {
                class: ClassId::new("mage").unwrap(),
                minimum: 1,
            },
            tor_simulation::rules::Requirement::SkillRank {
                skill: tor_simulation::rules::SkillSelector::Named {
                    skill: Skill::Spellcasting,
                },
                minimum: 1,
            },
            tor_simulation::rules::Requirement::Flag {
                flag: FlagId::new("mindless").unwrap(),
                present: false,
            },
        ],
    };
    thinking.form_choices = vec![tor_simulation::content_ids::FormId::new("falter").unwrap()];
    base = super::creature_fixture::with_talents(base, [thinking]);
    let mut game = Game::two_room_in_stone(42, base.content().clone());
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
        let mut build = base.clone();
        build = super::creature_fixture::with_starting_skills(
            build,
            [Skill::Spellcasting, Skill::Intimidation],
        );
        build
            .purchase(
                0,
                tor_simulation::content_ids::PointPoolId::new("advancement").unwrap(),
                tor_simulation::creatures::PurchaseSelection::TalentForm {
                    talent: TalentId::new("evocation_initiate").unwrap(),
                    form: tor_simulation::content_ids::FormId::new("spark_lance_fire").unwrap(),
                },
            )
            .unwrap();
        build
            .purchase(
                1,
                tor_simulation::content_ids::PointPoolId::new("advancement").unwrap(),
                tor_simulation::creatures::PurchaseSelection::TalentForm {
                    talent: TalentId::new("fixture_thinking_form").unwrap(),
                    form: tor_simulation::content_ids::FormId::new("falter").unwrap(),
                },
            )
            .unwrap();
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
            id: AbilityId::new("spark_lance_fire").unwrap(),
            invocation: tor_simulation::abilities::AbilityInvocation::ActorTarget {
                target: ActorId(2),
            },
            supplies: Default::default(),
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
        inspection.stats.hit_dice.classes,
        std::collections::BTreeMap::from([
            (ClassId::new("warrior").unwrap(), 1),
            (ClassId::new("mage").unwrap(), 1)
        ])
    );
    assert_eq!(
        inspection.creature.build().choices()[0]
            .talents()
            .cloned()
            .collect::<Vec<_>>(),
        vec![TalentId::new("evocation_initiate").unwrap()]
    );
    assert_eq!(
        *inspection.creature.build().starting_skills(),
        [Skill::Spellcasting, Skill::Intimidation]
    );
    assert!(
        inspection.creature.derived().sources[&SourceId::Class(ClassId::new("mage").unwrap())]
            .iter().any(|modifier| matches!(&modifier.op, ModifierOp::Flag { flag } if flag.as_str() == "magical"))
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
    build
        .set_templates(vec![TemplateId::new("mindless_form").unwrap()])
        .unwrap();
    game.rebuild_creature(ActorId(1), build.clone()).unwrap();
    let inspection = game.inspect_creature(ActorId(1)).unwrap();
    assert_eq!(inspection.creature.build().ledger().entries(), retained);
    assert_eq!(
        inspection
            .creature
            .build()
            .templates()
            .next()
            .unwrap()
            .id
            .as_str(),
        "mindless_form"
    );
    assert!(inspection
        .stats
        .dormant_talents
        .contains(&TalentId::new("fixture_thinking_form").unwrap()));
    assert!(!inspection
        .stats
        .active_talents
        .contains(&TalentId::new("fixture_thinking_form").unwrap()));
    assert!(inspection
        .stats
        .active_talents
        .contains(&TalentId::new("evocation_initiate").unwrap()));
    build.set_templates(vec![]).unwrap();
    game.rebuild_creature(ActorId(1), build.clone()).unwrap();
    let restored = game.inspect_creature(ActorId(1)).unwrap();
    assert!(restored
        .stats
        .active_talents
        .contains(&TalentId::new("fixture_thinking_form").unwrap()));
    assert!(!restored
        .stats
        .dormant_talents
        .contains(&TalentId::new("fixture_thinking_form").unwrap()));
    while build.remove_latest().is_some() {}
    game.rebuild_creature(ActorId(1), build).unwrap();
    let before = game.clone();
    let inspection = game.inspect_creature(ActorId(1)).unwrap();
    assert!(inspection.creature.health().dead());
    assert_eq!(inspection.creature.health().maximum(), 0);
    assert_eq!(inspection.creature.health().current(), 0);
    assert_eq!(inspection.stats.hit_dice.total, 0);
    assert_eq!(inspection.stats.hit_dice.racial, 0);
    assert!(inspection.stats.hit_dice.classes.is_empty());
    assert_eq!(inspection.identity.name, "subject 1");
    assert_eq!(game, before);
}
