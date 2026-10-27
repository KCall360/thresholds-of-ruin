//! Body artwork follows visible physical state without replacing personal identity.
use super::*;
use crate::attributes::{ManaBinding, Skill};
use crate::content_ids::BodyFormId;
use crate::creatures::{
    BodyPackage, Content, CreatureBuild, HdChoices, PlannedAdvancement, ScenarioCatalog,
};
use crate::progression::{HdLedger, HdSource};
use std::collections::BTreeMap;
use std::sync::Arc;

fn fixture() -> (Game, ActorId, ActorId, Vec<Arc<BodyPackage>>) {
    let species: Vec<_> = ["birth", "changed", "unpictured"]
        .into_iter()
        .map(crate::test_creatures::unmodified_species)
        .collect();
    let content = Content::new(
        crate::test_creatures::ruleset(),
        ScenarioCatalog {
            species: species
                .iter()
                .cloned()
                .map(|row| (row.id.clone(), row))
                .collect(),
            templates: BTreeMap::new(),
        },
    )
    .unwrap();
    let content = Content::with_detection_appearances(
        &content,
        [(
            species[1].id.clone(),
            crate::appearance_catalog::CreatureAppearance {
                name: Arc::from("changed creature"),
                asset: Some(Arc::from("creature.troll")),
            },
        )]
        .into_iter()
        .collect(),
    )
    .unwrap();
    let packages: Vec<_> = species
        .iter()
        .map(|species| {
            Arc::new(BodyPackage {
                id: BodyFormId::new(format!("private.body.{}", species.id)).unwrap(),
                species: species.id.clone(),
                body: crate::BodySpec::default(),
                advancement: vec![PlannedAdvancement {
                    source: HdSource::Racial,
                    choices: HdChoices::default(),
                }],
            })
        })
        .collect();
    let content = Content::with_body_packages(
        &content,
        packages
            .iter()
            .cloned()
            .map(|package| (package.id.clone(), package))
            .collect(),
    )
    .unwrap();
    let build = CreatureBuild::new(
        content.clone(),
        species[0].id.clone(),
        HdLedger::seeded(vec![HdSource::Racial], 42).unwrap(),
        ManaBinding::Intellect,
        [Skill::Deception, Skill::Persuasion],
    )
    .unwrap()
    .with_body_package(packages[0].clone(), 42)
    .unwrap();
    let mut game = Game::two_room_in_stone(42, content);
    game.world.set_region_light(RegionId(1), true).unwrap();
    let mut ids = Vec::new();
    for x in [1, 3] {
        let at = Location {
            region: RegionId(1),
            position: Position { x, y: 1, z: 0 },
        };
        let id = game
            .spawn_actor(at, std::num::NonZeroU64::new(100).unwrap())
            .unwrap();
        game.set_body(id, packages[0].body.clone()).unwrap();
        game.configure_creature(
            id,
            crate::CreatureIdentity {
                name: "same person".into(),
                faction: "fixture".into(),
            },
            build.clone(),
        )
        .unwrap();
        game.set_actor_asset(id, Some("creature.custom.birth".into()))
            .unwrap();
        ids.push(id);
    }
    (game, ids[0], ids[1], packages)
}

#[test]
fn body_artwork_projects_self_visible_other_and_custom_birth_return_without_identity_leak() {
    let (mut game, observer, subject, packages) = fixture();
    let base_view = game.observe(subject).unwrap();
    assert_eq!(base_view.asset.as_deref(), Some("creature.custom.birth"));
    let identity = game.actors[&subject]
        .combat
        .as_ref()
        .unwrap()
        .identity
        .clone();
    game.polymorph_creature(subject, packages[1].clone())
        .unwrap();
    assert_eq!(
        game.observe(subject).unwrap().asset.as_deref(),
        Some("creature.troll")
    );
    let view = game.observe(observer).unwrap();
    let other = view
        .visible_actors
        .iter()
        .find(|row| row.id == subject)
        .unwrap();
    assert_eq!(other.asset.as_deref(), Some("creature.troll"));
    assert_eq!(other.name, "same person");
    assert_eq!(
        game.actors[&subject].combat.as_ref().unwrap().identity,
        identity
    );
    assert_eq!(
        game.actors[&subject].asset.as_deref(),
        Some("creature.custom.birth")
    );
    game.world.set_region_light(RegionId(1), false).unwrap();
    assert!(
        game.observe(observer)
            .unwrap()
            .visible_actors
            .iter()
            .filter(|row| row.id == subject)
            .all(|row| row.asset.is_none()),
        "unseen body artwork remains undisclosed"
    );
    game.world.set_region_light(RegionId(1), true).unwrap();
    game.polymorph_creature(subject, packages[2].clone())
        .unwrap();
    assert_eq!(
        game.observe(subject).unwrap().asset,
        None,
        "missing current-form art must not reuse birth art"
    );
    assert_eq!(
        game.observe(observer)
            .unwrap()
            .visible_actors
            .iter()
            .find(|row| row.id == subject)
            .unwrap()
            .asset,
        None
    );
    game.apply_effects(subject, &[crate::rules::Effect::CancelSpecials])
        .unwrap();
    assert_eq!(
        game.observe(subject).unwrap().asset.as_deref(),
        Some("creature.custom.birth")
    );
}

#[test]
fn corpse_captures_current_body_artwork_not_personal_birth_art() {
    let (mut game, observer, subject, packages) = fixture();
    game.polymorph_creature(subject, packages[1].clone())
        .unwrap();
    crate::test_creatures::injure(&mut game, subject, 1000).unwrap();
    let corpse = game
        .items
        .values()
        .find(|item| item.corpse.as_ref().is_some_and(|row| row.actor == subject))
        .unwrap();
    assert_eq!(
        corpse.corpse.as_ref().unwrap().asset.as_deref(),
        Some("creature.troll")
    );
    game.set_actor_asset(subject, Some("creature.changed.after.death".into()))
        .unwrap();
    let view = game.observe(observer).unwrap();
    let corpse = view
        .ground_items
        .iter()
        .find(|item| item.corpse.as_ref().is_some_and(|row| row.actor == subject))
        .unwrap();
    assert_eq!(
        corpse.corpse.as_ref().unwrap().asset.as_deref(),
        Some("creature.troll")
    );
    game.world.set_region_light(RegionId(1), false).unwrap();
    assert!(game
        .observe(observer)
        .unwrap()
        .ground_items
        .iter()
        .all(|item| item
            .corpse
            .as_ref()
            .is_none_or(|row| row.actor != subject || row.asset.is_none())));
}
