use serde_json::json;
use std::{path::Path, sync::Arc};
use tor_protocol::{ActorId, CombatEventView};
use tor_server::{
    journal::{Action, Command, Direction, IntentionEndKind, JournalContent},
    scenario_package::{Manifest, Package, RegionDef},
    CommandResult, Engine, SavePolicy, Scenario,
};

fn scenario() -> Scenario {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two-room");
    let mut manifest: Manifest =
        toml::from_str(&std::fs::read_to_string(root.join("scenario.toml")).unwrap()).unwrap();
    let strike = |offset| {
        json!({"offset":offset,"unlock":null,"check":null,
        "on_success":[{"kind":"damage","damage":{"kind":"components","components":[{
            "category":"impact","descriptor":null,"amount":{"kind":"fixed","amount":1}}]}}],
        "on_failure":[],"descriptors":[],"riders":[]})
    };
    let mut rules: tor_simulation::rules::RulesetDocument =
        toml::from_str(tor_simulation::rules::DEFAULT_RULESET_TOML).unwrap();
    rules.abilities.push(serde_json::from_value(json!({"id":"triple_strike","name":"Triple Strike",
        "tags":[],"basic_attack":false,"cost":null,
        "timing":{"source":{"kind":"fixed","prepare":100,"recover":100},"scaled_by":"speed"},
        "reach":{"kind":"melee"},"strikes":{"kind":"authored","strikes":[strike(0),strike(100),strike(200)]},
        "ai":{"priority":100,"hostile":true,"minimum_resource_percent":0,"range_preference":"any"}})).unwrap());
    manifest.creatures = serde_json::from_value(json!({"species":{"sequence_fighter":{
        "kind":"humanoid","attributes":{"strength":0,"speed":0,"intellect":0,"willpower":0,"awareness":0,"presence":0},
        "melee":{"skill":"heavy_weaponry","bonus":0,"wind_up":100,"recovery":100,"strikes":[strike(0)]},
        "modifiers":[{"scope":"always","op":{"kind":"grant_ability","ability":"triple_strike"}},
            {"scope":"always","op":{"kind":"add","stat":{"kind":"health"},"value":200}}]
    }}})).unwrap();
    let creature = |name| {
        json!({"species":"sequence_fighter","name":name,"faction":"neutral",
        "binding":"intellect","starting_skills":["heavy_weaponry","athletics"],"restoration":"authored", "nutrition": {"kind":"exempt"},
        "hit_dice":[{"source":"racial"}]})
    };
    manifest.characters[0].creature = Some(serde_json::from_value(creature("Escaper")).unwrap());
    let mut regions: Vec<RegionDef> = [1, 2]
        .into_iter()
        .map(|id| {
            toml::from_str(
                &std::fs::read_to_string(root.join(format!("regions/{id}.toml"))).unwrap(),
            )
            .unwrap()
        })
        .collect();
    regions[0].items.clear();
    regions[0].actors = vec![
        serde_json::from_value(json!({"id":2,"at":[2,1,0],"turn_ticks":150,
        "controller":"external","creature":creature("Sequence sentinel")}))
        .unwrap(),
    ];
    let package =
        Package::from_parts(manifest, regions, &toml::to_string(&rules).unwrap()).unwrap();
    Scenario {
        actors: vec![],
        package: Some(Arc::new(package)),
        ..Scenario::two_room(42)
    }
}

fn command(engine: &mut Engine, actor: ActorId, id: &str, command: Command) -> CommandResult {
    engine
        .command(
            "player",
            "test",
            actor,
            id,
            &engine.branch().clone(),
            command,
        )
        .unwrap()
}
fn target_wait(engine: &mut Engine, id: &str) -> CommandResult {
    let revision = engine.revision(ActorId(2)).unwrap();
    command(
        engine,
        ActorId(2),
        id,
        Command::Act {
            expected_revision: revision,
            action: Action::Wait,
        },
    )
}
fn start_prefix(engine: &mut Engine) -> (CommandResult, Command) {
    let attack = Command::AdmitIntention {
        expected_revision: engine.revision(ActorId(1)).unwrap(),
        action: Action::UseAbility {
            id: tor_simulation::content_ids::AbilityId::new("triple_strike").unwrap(),
            invocation: tor_server::journal::AbilityInvocation::ActorTarget {
                target: tor_simulation::ActorId(2),
            },
            supplies: Default::default(),
        },
    };
    let admitted = command(engine, ActorId(1), "sequence-admission", attack.clone());
    let started = engine.execute_next_intention().unwrap().unwrap();
    assert!(started.entry.intention_ends.is_empty());
    let first = target_wait(engine, "first-target-wait");
    assert!(!first
        .entry
        .intention_ends
        .iter()
        .any(|end| end.actor == ActorId(1)));
    let own = engine.state(ActorId(1)).unwrap();
    let progress = own
        .observation
        .combat
        .as_ref()
        .unwrap()
        .sequence
        .as_ref()
        .unwrap();
    assert_eq!((progress.completed, progress.total), (1, 3));
    assert_eq!(progress.next_strike_remaining, 50);
    assert_eq!(progress.cancel_recovery_remaining, 250);
    (admitted, attack)
}

#[test]
fn ordered_strikes_normal_package_publishes_prefix_privately_and_one_terminal_completion() {
    let mut engine = Engine::memory(scenario()).unwrap();
    let before = engine
        .state(ActorId(2))
        .unwrap()
        .observation
        .combat
        .unwrap()
        .hp;
    let (admitted, _) = start_prefix(&mut engine);
    let target = engine.state(ActorId(2)).unwrap();
    let combat = target.observation.combat.as_ref().unwrap();
    assert!(combat.sequence.is_none());
    assert_eq!(combat.hp, before - 1);
    assert!(combat.events.iter().any(|event| matches!(
        event,
        CombatEventView::Ability {
            strike: Some(0),
            ..
        }
    )));
    let visible = serde_json::to_value(combat).unwrap();
    for event in visible["events"].as_array().unwrap() {
        for private in ["total", "check", "tokens", "plan", "intention"] {
            assert!(event.get(private).is_none(), "leaked {private}");
        }
    }
    let final_wait = target_wait(&mut engine, "final-target-wait");
    let JournalContent::IntentionAdmitted { intention, .. } = admitted.entry.content else {
        panic!("admission");
    };
    assert_eq!(
        final_wait
            .entry
            .intention_ends
            .iter()
            .filter(|end| end.intention == intention && end.kind == IntentionEndKind::Resolved)
            .count(),
        1
    );
    assert!(engine
        .state(ActorId(1))
        .unwrap()
        .observation
        .combat
        .unwrap()
        .sequence
        .is_none());
    assert_eq!(
        engine
            .state(ActorId(2))
            .unwrap()
            .observation
            .combat
            .unwrap()
            .hp,
        before - 3
    );
}

#[test]
fn ordered_strikes_cancel_and_escape_are_atomic_exact_retryable_transactions() {
    for step in [None, Some(Direction::West)] {
        let mut engine = Engine::memory(scenario()).unwrap();
        let (admitted, _) = start_prefix(&mut engine);
        let before = engine.state(ActorId(1)).unwrap();
        let target = engine.state(ActorId(2)).unwrap();
        let counts = engine.profile_counts();
        let rejected = Command::CancelIntention {
            expected_revision: before.revision,
            admission: admitted.entry.id.clone(),
            step: Some(Direction::East),
        };
        assert!(engine
            .command(
                "player",
                "test",
                ActorId(1),
                "blocked-escape",
                &engine.branch().clone(),
                rejected
            )
            .is_err());
        assert_eq!(engine.state(ActorId(1)).unwrap(), before);
        assert_eq!(engine.state(ActorId(2)).unwrap(), target);
        assert_eq!(engine.profile_counts(), counts);
        let cancel = Command::CancelIntention {
            expected_revision: before.revision,
            admission: admitted.entry.id.clone(),
            step,
        };
        let accepted = command(&mut engine, ActorId(1), "cancel-sequence", cancel.clone());
        assert_eq!(accepted.entry.intention_ends.len(), 1);
        assert_eq!(
            accepted.entry.intention_ends[0].kind,
            IntentionEndKind::Cancelled
        );
        let after = engine.state(ActorId(1)).unwrap();
        let combat = after.observation.combat.as_ref().unwrap();
        assert!(combat.sequence.is_none());
        assert_eq!(
            combat.recovery_remaining,
            if step.is_some() { 350 } else { 250 }
        );
        let replayed = command(&mut engine, ActorId(1), "cancel-sequence", cancel);
        assert!(replayed.duplicate);
        assert_eq!(replayed.entry, accepted.entry);
        assert_eq!(engine.state(ActorId(1)).unwrap(), after);
        let changed = Command::CancelIntention {
            expected_revision: before.revision,
            admission: admitted.entry.id,
            step: Some(Direction::North),
        };
        assert!(engine
            .command(
                "player",
                "test",
                ActorId(1),
                "cancel-sequence",
                &engine.branch().clone(),
                changed
            )
            .is_err());
        assert_eq!(engine.state(ActorId(1)).unwrap(), after);
    }
}

#[test]
fn ordered_strikes_saved_prefix_resumes_without_replaying_paid_strikes_or_admissions() {
    for interval in [1, 4096] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("sequence.db");
        let mut engine = Engine::open_with_policy(
            &path,
            scenario(),
            SavePolicy {
                checkpoint_interval: interval,
                ..Default::default()
            },
        )
        .unwrap();
        let target_before = engine
            .state(ActorId(2))
            .unwrap()
            .observation
            .combat
            .unwrap()
            .hp;
        let (admitted, attack) = start_prefix(&mut engine);
        let before = engine
            .state(ActorId(1))
            .unwrap()
            .observation
            .combat
            .unwrap()
            .sequence;
        engine.flush().unwrap();
        drop(engine);
        let mut engine = Engine::open(&path, Scenario::two_room(99)).unwrap();
        let reopened = engine.state(ActorId(1)).unwrap();
        let combat = reopened.observation.combat.as_ref().unwrap();
        assert_eq!(combat.sequence, before);
        assert!(
            combat.preparation_active,
            "Engine restores exact saved work"
        );
        // Service startup journals this pause before a new controller can act.
        // Engine loading alone must not rewrite the authoritative saved state.
        let pause = engine.pause_preparation(ActorId(1)).unwrap().unwrap();
        assert!(pause.entry.intention_ends.is_empty());
        let restored = engine.state(ActorId(1)).unwrap();
        let combat = restored.observation.combat.as_ref().unwrap();
        assert_eq!(combat.sequence, before);
        assert!(!combat.preparation_active);
        assert_eq!(
            engine
                .state(ActorId(2))
                .unwrap()
                .observation
                .combat
                .unwrap()
                .hp,
            target_before - 1
        );
        let duplicate = command(&mut engine, ActorId(1), "sequence-admission", attack);
        assert!(duplicate.duplicate);
        assert_eq!(engine.state(ActorId(1)).unwrap(), restored);
        let resume = Command::ResumeIntention {
            expected_revision: restored.revision,
            admission: admitted.entry.id,
        };
        command(&mut engine, ActorId(1), "resume-sequence", resume);
        engine.execute_next_intention().unwrap().unwrap();
        let ended = target_wait(&mut engine, "resume-target-wait");
        assert!(ended
            .entry
            .intention_ends
            .iter()
            .any(|end| end.actor == ActorId(1) && end.kind == IntentionEndKind::Resolved));
        assert_eq!(
            engine
                .state(ActorId(2))
                .unwrap()
                .observation
                .combat
                .unwrap()
                .hp,
            target_before - 3
        );
        engine.flush().unwrap();
    }
}
