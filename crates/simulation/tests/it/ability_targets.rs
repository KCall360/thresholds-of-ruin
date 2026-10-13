use super::abilities::id;
use std::{
    collections::{BTreeMap, BTreeSet},
    num::NonZeroU64,
};
use tor_simulation::{ActorId, BodySpec, CreatureIdentity, Game, MemoryRecords, RegionTransition};
use tor_world::{
    Direction, Extent, Location, NamedAnchor, Passage, Position, Region, RegionId, World,
};

fn at(region: u64, x: i32, y: i32, z: i32) -> Location {
    Location {
        region: RegionId(region),
        position: Position { x, y, z },
    }
}

fn fixture() -> (Game, ActorId, ActorId) {
    fixture_with_content(super::abilities::content())
}
fn fixture_with_content(
    content: std::sync::Arc<tor_simulation::creatures::Content>,
) -> (Game, ActorId, ActorId) {
    fixture_with_factions(content, ["neutral", "neutral"])
}
fn fixture_with_factions(
    content: std::sync::Arc<tor_simulation::creatures::Content>,
    factions: [&str; 2],
) -> (Game, ActorId, ActorId) {
    let world = World::new(
        (1..=2)
            .map(|id| Region {
                id: RegionId(id),
                name: format!("room{id}"),
                bounds: Extent::new(20, 12, 4).unwrap(),
            })
            .collect(),
        vec![],
    )
    .unwrap();
    let mut game = Game::new(world, 42, content.clone());
    let mut ids = vec![];
    for (location, build) in [
        (
            at(1, 1, 1, 0),
            super::abilities::build(content.clone(), true),
        ),
        (
            at(1, 7, 1, 0),
            super::abilities::build(content.clone(), false),
        ),
    ] {
        let id = game
            .spawn_actor(location, NonZeroU64::new(100).unwrap())
            .unwrap();
        game.configure_creature(
            id,
            CreatureIdentity {
                name: format!("subject{}", id.0),
                faction: factions[ids.len()].into(),
            },
            build,
        )
        .unwrap();
        ids.push(id);
    }
    (game, ids[0], ids[1])
}

fn assert_ranged(game: &Game, source: ActorId, target: ActorId, available: bool) {
    let before = game.clone();
    for ability in [id("magic_bolt"), id("fear")] {
        assert_eq!(
            game.ability_plan(source, &ability, target).is_ok(),
            available,
            "{ability:?} targeting {target:?}"
        );
    }
    assert_eq!(*game, before);
}

fn round_trip(game: &Game) -> Game {
    let mut shared = tor_simulation::checkpoint::SharedState::default();
    let snapshot = game.checkpoint(&mut shared);
    let bytes = serde_json::to_vec(&(snapshot, shared)).unwrap();
    let (snapshot, shared) = serde_json::from_slice(&bytes).unwrap();
    Game::restore_checkpoint(snapshot, &shared, game.content().clone()).unwrap()
}

#[test]
fn targeting_reconstructs_after_checkpoints_and_portal_occlusion_changes() {
    let (mut game, source, target) = fixture();
    game.teleport(source, at(1, 18, 1, 0)).unwrap();
    game.teleport(target, at(2, 0, 1, 0)).unwrap();
    game.connect(
        Passage {
            from: at(1, 19, 1, 0),
            direction: Direction::East,
            to: at(2, 0, 1, 0),
        },
        1,
    )
    .unwrap();
    assert_ranged(&game, source, target, true);
    game = round_trip(&game);
    assert_ranged(&game, source, target, true);
    game.set_wall(at(1, 19, 1, 0), true).unwrap();
    assert_ranged(&game, source, target, false);
    game = round_trip(&game);
    assert_ranged(&game, source, target, false);
    game.set_wall(at(1, 19, 1, 0), false).unwrap();
    assert_ranged(&game, source, target, true);
}

#[test]
fn ranged_targeting_uses_six_manhattan_cells_and_current_occlusion() {
    let (mut game, source, target) = fixture();
    for (location, available) in [
        (at(1, 7, 1, 0), true),
        (at(1, 8, 1, 0), false),
        (at(1, 4, 4, 0), true),
        (at(1, 4, 5, 0), false),
        (at(1, 1, 1, 3), true),
    ] {
        game.teleport(target, location).unwrap();
        assert_ranged(&game, source, target, available);
    }
    game.teleport(target, at(1, 7, 1, 0)).unwrap();
    for y in 0..12 {
        for z in 0..4 {
            game.set_wall(at(1, 4, y, z), true).unwrap();
        }
    }
    assert_ranged(&game, source, target, false);
    game.set_wall(at(1, 4, 1, 0), false).unwrap();
    assert_ranged(&game, source, target, true);
}

#[test]
fn ranged_targeting_measures_from_the_eye_to_visible_occupied_cells() {
    let (mut game, source, target) = fixture();
    game.set_body(
        source,
        BodySpec {
            cells: vec![[0, 0, 0], [0, 0, 1]],
            eye: [0, 0, 1],
            mass: 80,
        },
    )
    .unwrap();
    game.teleport(target, at(1, 7, 1, 1)).unwrap();
    assert_ranged(&game, source, target, true);
    game.teleport(target, at(1, 7, 1, 0)).unwrap();
    assert_ranged(&game, source, target, false);
    game.set_body(
        target,
        BodySpec {
            cells: vec![[0, 0, 0], [0, 0, 1]],
            eye: [0, 0, 1],
            mass: 80,
        },
    )
    .unwrap();
    assert_ranged(&game, source, target, true);
}

#[test]
fn targeting_crosses_rotated_physical_portals_but_not_abstract_stair_disclosures() {
    let (mut game, source, target) = fixture();
    game.teleport(source, at(1, 18, 1, 0)).unwrap();
    game.teleport(target, at(2, 0, 1, 0)).unwrap();
    assert_ranged(&game, source, target, false);
    game.connect(
        Passage {
            from: at(1, 19, 1, 0),
            direction: Direction::East,
            to: at(2, 0, 1, 0),
        },
        1,
    )
    .unwrap();
    assert_ranged(&game, source, target, true);
    assert!(!game
        .ability_plan(source, &id("power_strike"), target)
        .is_ok());

    let (mut game, source, target) = fixture();
    let landing = at(2, 1, 1, 0);
    game.teleport(target, landing).unwrap();
    game.register_named_anchors(
        RegionId(2),
        BTreeMap::from([("landing".into(), landing.position)]),
    )
    .unwrap();
    game.connect_named_stair(
        at(1, 1, 1, 0),
        Direction::Up,
        NamedAnchor {
            region: RegionId(2),
            name: "landing".into(),
        },
    )
    .unwrap();
    assert!(game
        .scene(source)
        .unwrap()
        .iter()
        .any(|cell| cell.location == landing));
    assert_ranged(&game, source, target, false);
}

#[test]
fn targeting_rejects_self_unknown_dead_frozen_and_ungranted_actors() {
    let (mut game, source, target) = fixture();
    assert_ranged(&game, source, target, true);
    assert_ranged(&game, source, source, false);
    assert_ranged(&game, source, ActorId(999), false);
    assert_ranged(&game, ActorId(999), target, false);
    let mut records = MemoryRecords::default();
    game.transition_regions(
        &RegionTransition {
            active: BTreeSet::new(),
            loaded: BTreeSet::from([RegionId(1), RegionId(2)]),
        },
        &mut records,
    )
    .unwrap();
    assert_ranged(&game, source, target, false);
    game.transition_regions(
        &RegionTransition {
            active: BTreeSet::from([RegionId(1), RegionId(2)]),
            loaded: BTreeSet::from([RegionId(1), RegionId(2)]),
        },
        &mut records,
    )
    .unwrap();
    assert_ranged(&game, source, target, true);
    let mut build = game.creature(source).unwrap().build().clone();
    build.set_templates(vec![]).unwrap();
    game.rebuild_creature(source, build).unwrap();
    assert_ranged(&game, source, target, false);
    game.teleport(target, at(1, 2, 1, 0)).unwrap();
    assert!(game
        .ability_plan(source, &id("basic_melee"), target)
        .is_ok());
    assert!(!game
        .ability_plan(source, &id("power_strike"), target)
        .is_ok());
    game.apply_effects(
        target,
        &[super::abilities::damage(
            1_000_000,
            tor_simulation::combat::DamageType::Vital,
        )],
    )
    .unwrap();
    assert!(!game
        .ability_plan(source, &id("basic_melee"), target)
        .is_ok());
}

#[test]
fn data_range_modifier_changes_both_disclosure_and_authoritative_targeting() {
    use tor_simulation::{
        content_ids::TemplateId,
        creatures::{Content, Template},
        rules::{AbilityStat, Modifier, ModifierOp, Reach, Scope, Stat},
    };
    let original = super::abilities::content();
    let mut catalog = original.catalog().clone();
    let mut extension = Template::new(TemplateId::new("long_reach").unwrap(), 0);
    extension.modifiers = vec![Modifier {
        scope: Scope::Always,
        op: ModifierOp::Add {
            stat: Stat::Ability {
                ability: id("magic_bolt"),
                stat: AbilityStat::Range,
            },
            value: 3,
        },
    }];
    catalog.templates.insert(extension.id.clone(), extension);
    let content = Content::new(original.rules().clone(), catalog).unwrap();
    let (mut game, source, target) = fixture_with_content(content);
    game.teleport(target, at(1, 9, 1, 0)).unwrap();
    assert!(game
        .ability_plan(source, &id("magic_bolt"), target)
        .is_err());
    let mut build = game.creature(source).unwrap().build().clone();
    build
        .set_templates(vec![
            TemplateId::new("techniques").unwrap(),
            TemplateId::new("long_reach").unwrap(),
        ])
        .unwrap();
    game.rebuild_creature(source, build).unwrap();
    assert_eq!(
        game.ability_plan(source, &id("magic_bolt"), target)
            .unwrap()
            .as_ability()
            .unwrap()
            .reach,
        Reach::Sight { range: 9 }
    );
    let view = game
        .observe(source)
        .unwrap()
        .combat
        .unwrap()
        .own_stats
        .unwrap();
    assert_eq!(
        view.ability_reaches[&id("magic_bolt")],
        Reach::Sight { range: 9 }
    );
    assert!(game.ability_plan(source, &id("fear"), target).is_err());
}

fn ally_fixture(range: serde_json::Value) -> (Game, ActorId, ActorId) {
    let original = super::abilities::content();
    let mut document = original.rules().document().clone();
    let ability = document
        .abilities
        .iter_mut()
        .find(|a| a.id == id("magic_bolt"))
        .unwrap();
    ability.reach = serde_json::from_value(serde_json::json!({
        "kind": "self_or_ally", "range": range,
    }))
    .expect("self-or-ally is a typed authored selector");
    let rules = tor_simulation::rules::Ruleset::validate(document).unwrap();
    let content =
        tor_simulation::creatures::Content::new(rules, original.catalog().clone()).unwrap();
    fixture_with_content(content)
}

#[test]
fn phase5_self_or_ally_touch_accepts_self_and_same_faction_only() {
    let (mut game, source, target) = ally_fixture(serde_json::json!({"kind":"touch"}));
    assert!(game.ability_plan(source, &id("magic_bolt"), source).is_ok());
    assert!(game
        .ability_plan(source, &id("magic_bolt"), target)
        .is_err());
    game.teleport(target, at(1, 2, 1, 0)).unwrap();
    assert!(game.ability_plan(source, &id("magic_bolt"), target).is_ok());
    let (mut game, source, target) =
        fixture_with_factions(game.content().clone(), ["neutral", "other"]);
    game.teleport(target, at(1, 2, 1, 0)).unwrap();
    assert!(!game.hostile(source, target));
    assert!(!game.hostile(target, source));
    assert!(game
        .ability_plan(source, &id("magic_bolt"), target)
        .is_err());
    assert!(game.ability_plan(source, &id("magic_bolt"), source).is_ok());
}

#[test]
fn phase5_self_or_ally_sight_uses_ordinary_occlusion_and_exact_frozen_shape() {
    let (mut game, source, target) = ally_fixture(serde_json::json!({"kind":"sight","range":6}));
    assert!(game.ability_plan(source, &id("magic_bolt"), source).is_ok());
    let plan = game
        .ability_plan(source, &id("magic_bolt"), target)
        .unwrap();
    assert_eq!(
        serde_json::to_value(&plan.as_ability().unwrap().reach).unwrap(),
        serde_json::json!({"kind":"self_or_ally","range":{"kind":"sight","range":6}})
    );
    for y in 0..12 {
        for z in 0..4 {
            game.set_wall(at(1, 4, y, z), true).unwrap();
        }
    }
    assert!(game
        .ability_plan(source, &id("magic_bolt"), target)
        .is_err());
    assert!(game.ability_plan(source, &id("magic_bolt"), source).is_ok());
    game = round_trip(&game);
    assert!(game
        .ability_plan(source, &id("magic_bolt"), target)
        .is_err());
}

#[test]
fn phase5_supportive_selector_rejects_either_hostility_direction_and_same_faction_hostility() {
    for (factions, edge) in [
        (["neutral", "other"], ("neutral", "other")),
        (["neutral", "other"], ("other", "neutral")),
        (["neutral", "neutral"], ("neutral", "neutral")),
    ] {
        let (base, _, _) = ally_fixture(serde_json::json!({"kind":"touch"}));
        let (mut game, actor, target) = fixture_with_factions(base.content().clone(), factions);
        game.teleport(target, at(1, 2, 1, 0)).unwrap();
        game.configure_run(
            actor,
            BTreeSet::from([actor]),
            None,
            BTreeMap::from([(edge.0.into(), BTreeSet::from([edge.1.into()]))]),
        )
        .unwrap();
        assert!(!game.allied(actor, target));
        assert!(game.ability_plan(actor, &id("magic_bolt"), target).is_err());
        assert!(
            game.ability_plan(actor, &id("magic_bolt"), actor).is_ok(),
            "self bypasses ally relation"
        );
        let disclosed = game.observe(actor).unwrap().combat.unwrap();
        assert!(disclosed.actors.iter().all(|(_, _, allied, _)| !allied));
    }
}

#[test]
fn phase5_supportive_touch_reuses_portal_contact_and_blocks_occluded_diagonals() {
    let (mut game, actor, target) = ally_fixture(serde_json::json!({"kind":"touch"}));
    game.teleport(actor, at(1, 19, 1, 0)).unwrap();
    game.teleport(target, at(2, 0, 1, 0)).unwrap();
    game.connect(
        Passage {
            from: at(1, 19, 1, 0),
            direction: Direction::East,
            to: at(2, 0, 1, 0),
        },
        1,
    )
    .unwrap();
    assert!(game.ability_plan(actor, &id("magic_bolt"), target).is_ok());
    game = round_trip(&game);
    assert!(game.ability_plan(actor, &id("magic_bolt"), target).is_ok());
    game.teleport(actor, at(1, 1, 1, 0)).unwrap();
    game.teleport(target, at(1, 2, 2, 0)).unwrap();
    assert!(game.ability_plan(actor, &id("magic_bolt"), target).is_ok());
    game.set_wall(at(1, 2, 1, 0), true).unwrap();
    game.set_wall(at(1, 1, 2, 0), true).unwrap();
    assert!(game.ability_plan(actor, &id("magic_bolt"), target).is_err());
}

#[test]
fn phase5_supportive_frozen_plan_preserves_selector_kind_and_bounded_spatial_mode() {
    use tor_simulation::{
        abilities::Plan,
        rules::{AllyRange, Reach},
    };
    for range in [
        serde_json::json!({"kind":"touch"}),
        serde_json::json!({"kind":"sight","range":6}),
    ] {
        let (mut game, actor, target) = ally_fixture(range.clone());
        game.teleport(target, at(1, 2, 1, 0)).unwrap();
        let original = game.ability_plan(actor, &id("magic_bolt"), target).unwrap();
        assert!(original.valid(game.content().rules()));
        let incompatible = if range["kind"] == "touch" {
            AllyRange::Sight { range: 6 }
        } else {
            AllyRange::Touch
        };
        for reach in [
            Reach::Melee,
            Reach::SelfOnly,
            Reach::Sight { range: 6 },
            Reach::SelfOrAlly {
                range: incompatible,
            },
            Reach::SelfOrAlly {
                range: AllyRange::Sight { range: 0 },
            },
            Reach::SelfOrAlly {
                range: AllyRange::Sight { range: 17 },
            },
        ] {
            let mut forged = original.clone();
            let Plan::Ability(plan) = &mut forged else {
                panic!("ability");
            };
            plan.reach = reach;
            assert!(!forged.valid(game.content().rules()));
        }
        game.teleport(target, at(1, 15, 1, 0)).unwrap();
        assert!(
            original.valid(game.content().rules()),
            "captured structure is independent of current reach"
        );
        assert!(game.ability_plan(actor, &id("magic_bolt"), target).is_err());
    }
}

#[test]
fn phase5_supportive_sight_range_modifiers_are_frozen_and_disclosed() {
    use tor_simulation::{
        content_ids::TemplateId,
        creatures::{Content, Template},
        rules::{AbilityStat, AllyRange, Modifier, ModifierOp, Reach, Scope, Stat},
    };
    let (game, _, _) = ally_fixture(serde_json::json!({"kind":"sight","range":6}));
    let original = game.content();
    let mut catalog = original.catalog().clone();
    let mut extension = Template::new(TemplateId::new("ally_reach").unwrap(), 0);
    extension.modifiers.push(Modifier {
        scope: Scope::Always,
        op: ModifierOp::Add {
            stat: Stat::Ability {
                ability: id("magic_bolt"),
                stat: AbilityStat::Range,
            },
            value: 3,
        },
    });
    catalog.templates.insert(extension.id.clone(), extension);
    let content = Content::new(original.rules().clone(), catalog).unwrap();
    let (mut game, actor, target) = fixture_with_content(content);
    game.teleport(target, at(1, 9, 1, 0)).unwrap();
    assert!(game.ability_plan(actor, &id("magic_bolt"), target).is_err());
    let mut build = game.creature(actor).unwrap().build().clone();
    build
        .set_templates(vec![
            TemplateId::new("techniques").unwrap(),
            TemplateId::new("ally_reach").unwrap(),
        ])
        .unwrap();
    game.rebuild_creature(actor, build).unwrap();
    let expected = Reach::SelfOrAlly {
        range: AllyRange::Sight { range: 9 },
    };
    assert_eq!(
        game.ability_plan(actor, &id("magic_bolt"), target)
            .unwrap()
            .as_ability()
            .unwrap()
            .reach,
        expected
    );
    assert_eq!(
        game.observe(actor)
            .unwrap()
            .combat
            .unwrap()
            .own_stats
            .unwrap()
            .ability_reaches[&id("magic_bolt")],
        expected
    );
}

#[test]
fn phase5_supportive_target_is_rechecked_at_queued_start_and_resolution() {
    use tor_simulation::{Action, IntentionOrigin};
    for started in [false, true] {
        let (mut game, actor, target) = ally_fixture(serde_json::json!({"kind":"touch"}));
        game.teleport(target, at(1, 2, 1, 0)).unwrap();
        let original_health = game.health(target).unwrap();
        game.admit_intention(
            actor,
            Action::UseAbility {
                id: id("magic_bolt"),
                target,
                supplies: Default::default(),
            },
            IntentionOrigin::Human,
        )
        .unwrap();
        if started {
            game.execute_next_intention().unwrap().outcome.unwrap();
        }
        game.configure_run(
            actor,
            BTreeSet::from([actor]),
            None,
            BTreeMap::from([("neutral".into(), BTreeSet::from(["neutral".into()]))]),
        )
        .unwrap();
        if started {
            game.act(target, Action::Wait).unwrap();
            assert!(game.preparation(actor).is_none());
        } else {
            assert!(game.execute_next_intention().unwrap().outcome.is_err());
        }
        assert_eq!(game.health(target).unwrap(), original_health);
        assert!(game
            .creature(actor)
            .unwrap()
            .costs()
            .reservations()
            .is_empty());
    }
}

#[test]
fn phase5_supportive_targets_reject_dead_unknown_and_frozen_actors_including_self() {
    let (mut game, actor, target) = ally_fixture(serde_json::json!({"kind":"touch"}));
    game.teleport(target, at(1, 2, 1, 0)).unwrap();
    assert!(game
        .ability_plan(actor, &id("magic_bolt"), ActorId(999))
        .is_err());
    let mut records = MemoryRecords::default();
    game.transition_regions(
        &RegionTransition {
            active: BTreeSet::new(),
            loaded: BTreeSet::from([RegionId(1), RegionId(2)]),
        },
        &mut records,
    )
    .unwrap();
    for candidate in [actor, target] {
        assert!(game
            .ability_plan(actor, &id("magic_bolt"), candidate)
            .is_err());
    }
    game.transition_regions(
        &RegionTransition {
            active: BTreeSet::from([RegionId(1), RegionId(2)]),
            loaded: BTreeSet::from([RegionId(1), RegionId(2)]),
        },
        &mut records,
    )
    .unwrap();
    game.apply_effects(
        target,
        &[super::abilities::damage(
            1_000_000,
            tor_simulation::combat::DamageType::Vital,
        )],
    )
    .unwrap();
    assert!(game.ability_plan(actor, &id("magic_bolt"), target).is_err());
    assert!(game.ability_plan(actor, &id("magic_bolt"), actor).is_ok());
    game.apply_effects(
        actor,
        &[super::abilities::damage(
            1_000_000,
            tor_simulation::combat::DamageType::Vital,
        )],
    )
    .unwrap();
    assert!(game.ability_plan(actor, &id("magic_bolt"), actor).is_err());
}
