//! Public autonomous admission retains defensive plans across the real checkpoint boundary.
use super::creature_fixture as fixture;
use std::{collections::BTreeMap, num::NonZeroU64};
use tor_simulation::{
    ai::AiProfile,
    attributes::{ManaBinding, Skill},
    checkpoint::SharedState,
    content_ids::{AbilityId, ConditionFamilyId, ConditionId},
    creatures::{Content, CreatureBuild, ScenarioCatalog},
    progression::{HdLedger, HdSource},
    resources::Resource,
    rules::{
        ConditionDef, ConditionLifecycle, Eviction, Modifier, ModifierOp, Ruleset, Scope, Stacking,
    },
    Action, CreatureIdentity, Game,
};
use tor_world::{Location, Position, RegionId};

#[test]
fn defensive_ai_admission_replays_the_same_paid_plan_then_stops_reapplying() {
    let mut document = fixture::ruleset().document().clone();
    let condition = ConditionId::new("integration_ward").unwrap();
    document.conditions.push(ConditionDef {
        id: condition.clone(),
        family: ConditionFamilyId::new("integration_ward").unwrap(),
        lifecycle: ConditionLifecycle::Timed,
        timing: tor_simulation::rules::ConditionTiming::expiring(),
        restrictions: vec![],
        preparation: tor_simulation::rules::ConditionPreparation::Preserve,
        name: "Integration Ward".into(),
        modifiers: vec![Modifier {
            scope: Scope::Always,
            op: ModifierOp::TalentReduction {
                category: tor_simulation::combat::DamageType::Impact,
                amount: 2,
            },
        }],
        stacking: Stacking::Longest,
        source_scoped: false,
        descriptors: vec![],
        capacity: Eviction::Reject,
    });
    let ability = AbilityId::new("integration_defense").unwrap();
    document.abilities.push(serde_json::from_value(serde_json::json!({
        "id":"integration_defense","name":"Integration Defense","tags":[],"basic_attack":false,
        "invocation":"actor_target","cost":{"resource":"stamina","start":1,"resolution":1},
        "cooldown":1000,"timing":{"source":{"kind":"fixed","prepare":100,"recover":100},"scaled_by":null},
        "reach":{"kind":"self_only"},"strikes":{"kind":"authored","strikes":[{
            "offset":0,"unlock":null,"check":null,"on_success":[{
                "kind":"apply_condition","condition":"integration_ward","duration":1000}],
            "on_failure":[],"descriptors":[],"riders":[]}]},
        "interaction": "non_hostile", "ai":{"priority":30,"hostile":false,"minimum_resource_percent":100,"range_preference":"any"}
    })).unwrap());
    let mut species = fixture::species();
    species.modifiers.push(Modifier {
        scope: Scope::Always,
        op: ModifierOp::GrantAbility {
            ability: ability.clone(),
        },
    });
    let content = Content::new(
        Ruleset::validate(document).unwrap(),
        ScenarioCatalog {
            species: BTreeMap::from([(species.id.clone(), species.clone())]),
            templates: BTreeMap::new(),
        },
    )
    .unwrap();
    let mut game = Game::two_room_in_stone(42, content.clone());
    let mut actors = Vec::new();
    for (x, faction) in [(1, "hero"), (2, "foe")] {
        let actor = game
            .spawn_actor(
                Location {
                    region: RegionId(1),
                    position: Position { x, y: 1, z: 0 },
                },
                NonZeroU64::new(100).unwrap(),
            )
            .unwrap();
        let build = CreatureBuild::new(
            content.clone(),
            species.id.clone(),
            HdLedger::seeded(vec![HdSource::Racial], 42).unwrap(),
            ManaBinding::Intellect,
            [Skill::Deception, Skill::Persuasion],
        )
        .unwrap();
        game.configure_creature(
            actor,
            CreatureIdentity {
                name: faction.into(),
                faction: faction.into(),
            },
            build,
        )
        .unwrap();
        actors.push(actor);
    }
    let (human, mob) = (actors[0], actors[1]);
    game.spawn_actor(
        Location {
            region: RegionId(1),
            position: Position { x: 0, y: 0, z: 0 },
        },
        NonZeroU64::new(100).unwrap(),
    )
    .unwrap();
    game.configure_run(
        human,
        [human].into(),
        None,
        [("foe".into(), ["hero".into()].into())].into(),
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
    game.refresh_navigation();
    game.act(human, Action::Wait).unwrap();
    let expected = Action::UseAbility {
        id: ability,
        invocation: tor_simulation::abilities::AbilityInvocation::actor_target(mob),
        supplies: Default::default(),
    };
    assert_eq!(game.next_ai_action(), Some((mob, expected.clone())));
    let before = game.clone();
    let balance = game
        .creature(mob)
        .unwrap()
        .costs()
        .resources()
        .balance(Resource::Stamina);
    let original = game.admit_ai_intention(mob).unwrap();
    assert_eq!(
        game.creature(mob).unwrap().costs(),
        before.creature(mob).unwrap().costs()
    );
    let executed = game.execute_next_intention().unwrap();
    assert_eq!(executed.action, Some(expected.clone()));
    executed.outcome.unwrap();
    game.pause_preparation(mob).unwrap();
    let mut shared = SharedState::default();
    let checkpoint =
        serde_json::from_value(serde_json::to_value(game.checkpoint(&mut shared)).unwrap())
            .unwrap();
    let shared = serde_json::from_value(serde_json::to_value(shared).unwrap()).unwrap();
    let mut restored = Game::restore_checkpoint(checkpoint, &shared, content).unwrap();
    assert_eq!(game, restored);
    for candidate in [&mut game, &mut restored] {
        let started_at = candidate.tick();
        let frozen = candidate.preparation(mob).unwrap().plan.clone();
        let costs = candidate.creature(mob).unwrap().costs().clone();
        assert_eq!(candidate.next_ai_action(), Some((mob, expected.clone())));
        let resumed = candidate.admit_ai_intention(mob).unwrap();
        candidate.execute_next_intention().unwrap().outcome.unwrap();
        let pending = candidate.preparation(mob).unwrap();
        assert_eq!(pending.origin_intention(), Some(original));
        assert_eq!(pending.intention, Some(resumed));
        assert_eq!(pending.plan, frozen);
        assert_eq!(candidate.creature(mob).unwrap().costs(), &costs);
        for _ in 0..10 {
            if candidate.preparation(mob).is_none() && candidate.next_actor() == Some(mob) {
                break;
            }
            candidate
                .act(candidate.next_actor().unwrap(), Action::Wait)
                .unwrap();
        }
        assert!(candidate.preparation(mob).is_none());
        // Gross payment and actor-active recovery are distinct. Advance the
        // one captured hold through its real intervals instead of inferring
        // spending from a final balance that may already be full again.
        assert_eq!(costs.resources().balance(Resource::Stamina), balance - 1);
        assert_eq!(costs.reservations().len(), 1);
        assert_eq!(
            costs.reservations()[&original]
                .entries()
                .find(|cost| cost.resource == Resource::Stamina)
                .unwrap()
                .resolution,
            1
        );
        let mut expected_costs = costs;
        expected_costs.advance_active(frozen.preparation());
        expected_costs.finish(original).unwrap();
        expected_costs.advance_active(candidate.tick() - started_at - frozen.preparation());
        assert_eq!(candidate.creature(mob).unwrap().costs(), &expected_costs);
        assert!(candidate
            .creature(mob)
            .unwrap()
            .conditions()
            .instances()
            .any(|entry| entry.condition == condition));
        assert_ne!(candidate.next_ai_action(), Some((mob, expected.clone())));
    }
    assert_eq!(game, restored);
}
