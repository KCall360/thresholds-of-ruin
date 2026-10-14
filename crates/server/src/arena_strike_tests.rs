//! Arena report units stay distinct from admitted actions and rider checks.
use super::*;
use crate::scenario_package::{Manifest, Package, RegionDef};
use serde_json::json;
use std::sync::Arc;

fn sequence_arena() -> Scenario {
    let damage = |amount| {
        json!({"kind":"damage","damage":{"kind":"components","components":[{
        "category":"impact","amount":{"kind":"fixed","amount":amount}}]}})
    };
    let row = |offset, riders| {
        json!({"offset":offset,
        "check":{"actor":"user","skill":{"kind":"selected_weapon"},
            "against":{"kind":"defense","defense":"physical"}},
        "on_success":[damage(3)],"on_failure":[],"descriptors":[],
        "riders":if riders { vec![2,4].into_iter().map(|amount| json!({"trigger":"success",
            "check":{"skill":{"kind":"selected_weapon"},"defense":"physical"},
            "effects":[damage(amount)],"descriptors":[]})).collect::<Vec<_>>() } else { vec![] }})
    };
    let species = |wind_up, bonus, rows| {
        json!({"kind":"humanoid",
        "attributes":{"strength":0,"speed":0,"intellect":0,"willpower":0,"awareness":0,"presence":0},
        "melee":{"skill":"heavy_weaponry","bonus":bonus,"wind_up":wind_up,"recovery":10000,"strikes":rows},
        "modifiers":[{"scope":"always","op":{"kind":"add","stat":{"kind":"health"},"value":200}}]})
    };
    let creature = |species, faction| {
        json!({"species":species,"name":species,"faction":faction,
        "binding":"intellect","starting_skills":["heavy_weaponry","athletics"],
        "restoration":"authored","hit_dice":[{"source":"racial"}]})
    };
    let mut manifest: Manifest =
        toml::from_str(include_str!("../../../scenarios/two-room/scenario.toml")).unwrap();
    manifest.id = "strike-denominator".into();
    manifest.arena = Some(
        serde_json::from_value(
            json!({"participants":[1,2],"control":"all_ai","ticks":35,"actions":100}),
        )
        .unwrap(),
    );
    manifest.factions = serde_json::from_value(json!({"blue":["red"],"red":["blue"]})).unwrap();
    manifest.ai_profiles =
        serde_json::from_value(json!({"duel":{"memory_ticks":1000,"flee_percent":0}})).unwrap();
    manifest.creatures = serde_json::from_value(json!({"species":{
        "attacker":species(10, 1000, vec![row(0,true),row(10,true),row(20,true)]),
        "defender":species(10000, 0, vec![row(0,false)])}}))
    .unwrap();
    manifest.characters[0].creature =
        Some(serde_json::from_value(creature("attacker", "blue")).unwrap());
    manifest.characters[0].unselected = "ai".into();
    manifest.characters[0].ai = Some("duel".into());
    let region: RegionDef = serde_json::from_value(json!({"id":1,"name":"Duel","size":[5,3,2],
        "chamber":true,"gravity":[0,0,-1],"anchors":{"start":[1,1,0]},
        "actors":[{"id":2,"at":[2,1,0],"turn_ticks":100,"controller":"ai","ai":"duel",
            "creature":creature("defender","red")}]}))
    .unwrap();
    let package = Package::from_parts(
        manifest,
        vec![region],
        tor_simulation::rules::DEFAULT_RULESET_TOML,
    )
    .unwrap();
    Scenario {
        actors: vec![],
        regions: 1,
        package: Some(Arc::new(package)),
        ..Scenario::two_room(42)
    }
}

#[test]
fn ordered_strikes_arena_denominator_separates_one_admission_primaries_and_riders() {
    let mut engine = Engine::memory(sequence_arena()).unwrap();
    let mut decisions = Vec::new();
    let (mut primary, mut riders) = (0, 0);
    while let Some((actor, action)) = engine.next_ai_action() {
        assert!(
            decisions.len() < 100,
            "fixture must finish within the authored bound"
        );
        decisions.push((actor, action));
        engine.advance_ai(actor).unwrap();
        for event in engine.game.combat_events() {
            if let CombatEvent::Resolved { outcome, strike } = event {
                if outcome.actor != SimActor(1) {
                    continue;
                }
                assert!(outcome
                    .check
                    .as_ref()
                    .is_some_and(|check| check.actor == CheckActor::User && check.success));
                if strike.is_some() {
                    primary += 1;
                } else {
                    riders += 1;
                }
            }
        }
    }
    let own = decisions
        .iter()
        .filter(|(actor, _)| *actor == ActorId(1))
        .collect::<Vec<_>>();
    assert_eq!(
        own.len(),
        1,
        "only one attack admission from the measured actor"
    );
    assert!(
        matches!(&own[0].1, crate::journal::Action::UseAbility { id, invocation: crate::actions::AbilityInvocation::ActorTarget { target }, .. }
        if id.as_str() == "basic_melee" && *target == SimActor(2))
    );
    assert_eq!((primary, riders), (3, 6));
    let report = run(sequence_arena()).unwrap();
    assert_eq!(
        report.actions,
        decisions.len() as u64,
        "actions count AI decisions, not outcomes"
    );
    assert_eq!(report.termination, ArenaTermination::TickLimit);
    let source = report
        .participants
        .iter()
        .find(|actor| actor.actor == ActorId(1))
        .unwrap();
    let counter = &source.abilities["basic_melee"];
    assert_eq!((counter.resolutions, counter.applications), (3, 3));
    assert_eq!(
        (source.attack_checks, source.hits),
        (3, 3),
        "six rider checks do not inflate the primary denominator"
    );
    assert_eq!(
        source.damage_dealt, 27,
        "rider effects still contribute damage"
    );
    assert_eq!(
        report
            .participants
            .iter()
            .find(|actor| actor.actor == ActorId(2))
            .unwrap()
            .damage_received,
        27
    );
}
