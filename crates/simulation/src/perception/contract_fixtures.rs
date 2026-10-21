//! Small source-owned fixtures shared by the perception/lifecycle contracts.
use crate::perception::{GuiseAppearance, PerceptionValue};
use crate::{
    abilities::AbilityInvocation,
    attributes::{ManaBinding, Skill},
    content_ids::{AbilityId, ConditionId, SpeciesId},
    creatures::{Content, CreatureBuild, ScenarioCatalog, Species},
    progression::{HdLedger, HdSource, HitDie},
    rules::{AbilityDef, Effect, Modifier, ModifierOp, Ruleset, RulesetDocument, Scope},
    Action, ActorId, CreatureIdentity, Game, IntentionId, IntentionOrigin,
};
use std::{collections::BTreeMap, num::NonZeroU64};
use tor_world::{Extent, Location, Position, Region, RegionId, World};

pub(crate) const OWNER: ActorId = ActorId(1);
pub(crate) const SUBJECT: ActorId = ActorId(2);

pub(crate) fn at(region: u64, x: i32, y: i32) -> Location {
    Location {
        region: RegionId(region),
        position: Position { x, y, z: 0 },
    }
}

pub(crate) fn appearance() -> GuiseAppearance {
    GuiseAppearance {
        name: "weathered chest".into(),
        description: "A closed wooden chest.".into(),
        class: crate::ItemClass::Tool,
        asset: Some("fixture_chest".into()),
    }
}

pub(crate) fn guise() -> PerceptionValue {
    PerceptionValue::ObjectGuise {
        skill: crate::attributes::Skill::Stealth,
        edge: 0,
        appearance: Some(appearance()),
    }
}

pub(crate) fn ambush() -> PerceptionValue {
    PerceptionValue::AmbushFromGuise { enabled: true }
}

fn modifier(value: PerceptionValue) -> Modifier {
    Modifier {
        scope: Scope::Always,
        op: ModifierOp::Perception { value },
    }
}

fn condition(name: &str, values: Vec<PerceptionValue>) -> crate::rules::ConditionDef {
    serde_json::from_value(serde_json::json!({
        "id":name, "family":name, "name":name, "lifecycle":"timed",
        "timing":{"kind":"actor_active","on_expiry":{"kind":"remove"}},
        "restrictions":[], "preparation":"preserve",
        "modifiers":values.into_iter().map(modifier).collect::<Vec<_>>(),
        "stacking":{"kind":"longest"}, "source_scoped":false,
        "descriptors":[], "capacity":"reject"
    }))
    .unwrap()
}

fn ability(name: &str, reach: serde_json::Value, effects: Vec<Effect>) -> AbilityDef {
    serde_json::from_value(serde_json::json!({
        "id":name, "name":name, "tags":[], "basic_attack":false,
        "invocation":"actor_target", "cost":null,
        "timing":{"source":{"kind":"fixed","prepare":50,"recover":50},"scaled_by":null},
        "reach":reach,
        "strikes":{"kind":"authored","strikes":[{
            "offset":0,"unlock":null,"check":null,"on_success":effects,
            "on_failure":[],"descriptors":[],"riders":[]
        }]},
        "interaction":"non_hostile",
        "ai":{"priority":0,"hostile":false,"minimum_resource_percent":0,"range_preference":"any"}
    }))
    .unwrap()
}

pub(crate) fn fixture(owner: &[PerceptionValue], subject: &[PerceptionValue]) -> Game {
    fixture_with(owner, subject, |_, _, _| {})
}

pub(crate) fn fixture_with(
    owner: &[PerceptionValue],
    subject: &[PerceptionValue],
    edit: impl FnOnce(&mut RulesetDocument, &mut Species, &mut Species),
) -> Game {
    fixture_with_height(owner, subject, 1, edit)
}

pub(crate) fn fixture_with_height(
    owner: &[PerceptionValue],
    subject: &[PerceptionValue],
    height: i32,
    edit: impl FnOnce(&mut RulesetDocument, &mut Species, &mut Species),
) -> Game {
    let mut document = crate::test_creatures::ruleset().document().clone();
    document.conditions.extend([
        condition(
            "fixture_blind",
            vec![PerceptionValue::Sight { enabled: false }],
        ),
        condition(
            "fixture_detection",
            vec![PerceptionValue::SeeInvisible { enabled: true }],
        ),
        condition("fixture_guise", vec![guise()]),
        condition("fixture_ambush", vec![ambush()]),
        condition(
            "fixture_suppression",
            vec![
                PerceptionValue::ObjectGuise {
                    skill: crate::attributes::Skill::Stealth,
                    edge: 0,
                    appearance: None,
                },
                PerceptionValue::AmbushFromGuise { enabled: false },
            ],
        ),
    ]);
    document.abilities.extend([
        ability(
            "fixture_search",
            serde_json::json!({"kind":"self_only"}),
            vec![Effect::SearchAdjacentGuises],
        ),
        ability(
            "fixture_sight",
            serde_json::json!({"kind":"sight","range":6}),
            vec![],
        ),
        ability(
            "fixture_ally",
            serde_json::json!({"kind":"self_or_ally","range":{"kind":"touch"}}),
            vec![],
        ),
        ability(
            "fixture_read_plan",
            serde_json::json!({"kind":"sight","range":6}),
            vec![Effect::ObservePublicPlan],
        ),
    ]);
    let mut special_melee = crate::test_creatures::paid_melee();
    special_melee.cost = None;
    document.abilities.push(special_melee);
    let mut a = crate::test_creatures::species();
    a.id = SpeciesId::new("fixture_perception_owner").unwrap();
    let mut b = a.clone();
    b.id = SpeciesId::new("fixture_perception_subject").unwrap();
    for (species, values) in [(&mut a, owner), (&mut b, subject)] {
        species
            .modifiers
            .extend(values.iter().cloned().map(modifier));
        for name in [
            "fixture_search",
            "fixture_sight",
            "fixture_ally",
            "fixture_read_plan",
            "fixture_paid_melee",
        ] {
            species.modifiers.push(Modifier {
                scope: Scope::Always,
                op: ModifierOp::GrantAbility { ability: id(name) },
            });
        }
    }
    edit(&mut document, &mut a, &mut b);
    let content = Content::new(
        Ruleset::validate(document).unwrap(),
        ScenarioCatalog {
            species: BTreeMap::from([(a.id.clone(), a), (b.id.clone(), b)]),
            templates: BTreeMap::new(),
        },
    )
    .unwrap();
    let world = World::new(
        (1..=2)
            .map(|region| Region {
                id: RegionId(region),
                name: format!("perception room {region}"),
                bounds: Extent::new(9, 7, height).unwrap(),
            })
            .collect(),
        vec![],
    )
    .unwrap();
    let mut game = Game::new(world, 42, content);
    assert_eq!(
        spawn(&mut game, "fixture_perception_owner", at(1, 2, 2)),
        OWNER
    );
    assert_eq!(
        spawn(&mut game, "fixture_perception_subject", at(1, 3, 2)),
        SUBJECT
    );
    game
}

pub(crate) fn spawn(game: &mut Game, species: &str, location: Location) -> ActorId {
    let build = CreatureBuild::new(
        game.content().clone(),
        SpeciesId::new(species).unwrap(),
        HdLedger::new(vec![HitDie::new(HdSource::Racial, 42)]).unwrap(),
        ManaBinding::Intellect,
        [Skill::Deception, Skill::Persuasion],
    )
    .unwrap();
    let actor = game
        .spawn_actor(location, NonZeroU64::new(100).unwrap())
        .unwrap();
    game.configure_creature(
        actor,
        CreatureIdentity {
            name: format!("private creature {}", actor.0),
            faction: "private affiliation".into(),
        },
        build,
    )
    .unwrap();
    game.set_actor_asset(actor, Some("private_creature_asset".into()))
        .unwrap();
    actor
}

pub(crate) fn id(name: &str) -> AbilityId {
    AbilityId::new(name).unwrap()
}

pub(crate) fn action(name: &str, invocation: AbilityInvocation) -> Action {
    Action::UseAbility {
        id: id(name),
        invocation,
        supplies: Default::default(),
    }
}

pub(crate) fn attack(target: ActorId) -> Action {
    action("basic_melee", AbilityInvocation::actor_target(target))
}

pub(crate) fn contact(target: ActorId) -> Action {
    action("basic_melee", AbilityInvocation::contact_target(target))
}

pub(crate) fn apply(game: &mut Game, actor: ActorId, name: &str, duration: u64) {
    game.apply_condition(actor, &ConditionId::new(name).unwrap(), actor, duration)
        .unwrap();
}

pub(crate) fn start(game: &mut Game, actor: ActorId, action: Action) -> IntentionId {
    let intention = game
        .admit_intention(actor, action, IntentionOrigin::Human)
        .unwrap();
    let execution = game
        .execute_next_intention()
        .expect("admitted actor is next");
    assert_eq!(execution.intention.id, intention);
    execution.outcome.unwrap();
    intention
}

pub(crate) fn restore(game: &Game) -> Game {
    let mut shared = crate::checkpoint::SharedState::default();
    let snapshot =
        serde_json::from_slice(&serde_json::to_vec(&game.checkpoint(&mut shared)).unwrap())
            .unwrap();
    let shared = serde_json::from_slice(&serde_json::to_vec(&shared).unwrap()).unwrap();
    Game::restore_checkpoint(snapshot, &shared, game.content().clone()).unwrap()
}
