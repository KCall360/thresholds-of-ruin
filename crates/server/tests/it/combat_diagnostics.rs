use crate::support;
use tempfile::tempdir;
use tor_protocol::{ActorId, CombatTraceStep, ErrorCode};
use tor_server::journal::{Command, WizardOperation};
use tor_server::{Engine, Scenario};

fn scenario() -> Scenario {
    let mut scenario = support::load("mob-arena", 42);
    let package = scenario.package.take().unwrap();
    let mut manifest = package.manifest.clone();
    let arena = manifest.arena.as_mut().unwrap();
    arena.start_paused = true;
    arena.control = tor_server::scenario_package::ArenaControl::AllAi;
    scenario.package = Some(std::sync::Arc::new(
        tor_server::scenario_package::Package::from_parts(manifest, package.region_defs().unwrap())
            .unwrap(),
    ));
    scenario
}
fn wizard(engine: &mut Engine, operation: WizardOperation) {
    let expected_revision = engine.revision(ActorId(1)).unwrap();
    support::submit(
        engine,
        ActorId(1),
        Command::Wizard {
            expected_revision,
            operation,
        },
    )
    .unwrap();
}
fn run(engine: &mut Engine) {
    wizard(
        engine,
        WizardOperation::ArenaControl {
            paused: true,
            advance: 32,
        },
    );
    let mut steps = 0;
    while let Some((actor, _)) = engine.next_ai_action() {
        engine.advance_ai(actor).unwrap();
        steps += 1;
        assert!(steps <= 32, "arena step budget was exceeded");
    }
}

#[test]
fn backend_numeric_capture_is_authorized_readonly_and_projects_actual_seeded_resolutions() {
    let mut engine = Engine::memory(scenario()).unwrap();
    for through in [None, Some(u64::MAX)] {
        assert_eq!(
            engine.inspect_combat_diagnostics(through).unwrap_err().code,
            ErrorCode::Unauthorized
        );
    }
    for enabled in [false, true] {
        assert_eq!(
            engine
                .configure_combat_diagnostics(enabled)
                .unwrap_err()
                .code,
            ErrorCode::Unauthorized
        );
    }
    engine.enable_wizard().unwrap();
    let state = engine.state(ActorId(1)).unwrap();
    let history = engine.history(ActorId(1), "player", None, 100).unwrap();
    assert!(!engine.inspect_combat_diagnostics(None).unwrap().enabled);
    let enabled = engine.configure_combat_diagnostics(true).unwrap();
    assert!(enabled.enabled && enabled.records.is_empty());
    assert_eq!(engine.state(ActorId(1)).unwrap(), state);
    assert_eq!(
        engine.history(ActorId(1), "player", None, 100).unwrap(),
        history
    );
    run(&mut engine);
    let state = engine.state(ActorId(1)).unwrap();
    let history = engine.history(ActorId(1), "player", None, 100).unwrap();
    let report = engine.inspect_combat_diagnostics(None).unwrap();
    assert!(report.validate().is_ok());
    assert!(!report.records.is_empty());
    assert!(report
        .records
        .iter()
        .all(|record| !record.trace.steps.is_empty()));
    assert!(report.records.iter().any(|record| record.charge.is_some()));
    assert!(report.records.iter().any(|record| record
        .trace
        .steps
        .iter()
        .any(|step| matches!(step, CombatTraceStep::Check { .. }))));
    assert_eq!(engine.inspect_combat_diagnostics(None).unwrap(), report);
    assert_eq!(engine.configure_combat_diagnostics(true).unwrap(), report);
    assert_eq!(
        engine
            .inspect_combat_diagnostics(Some(report.captured + 1))
            .unwrap_err()
            .code,
        ErrorCode::InvalidAction
    );
    if report.records[0].sequence > report.dropped + 1 {
        let page = engine
            .inspect_combat_diagnostics(Some(report.records[0].sequence - 1))
            .unwrap();
        assert!(page.validate().is_ok());
        assert_eq!(
            page.records.last().unwrap().sequence,
            report.records[0].sequence - 1
        );
    }
    assert_eq!(engine.state(ActorId(1)).unwrap(), state);
    assert_eq!(
        engine.history(ActorId(1), "player", None, 100).unwrap(),
        history
    );
    let disabled = engine.configure_combat_diagnostics(false).unwrap();
    assert!(!disabled.enabled && disabled.records.is_empty() && disabled.captured == 0);
    assert_eq!(engine.state(ActorId(1)).unwrap(), state);
}

#[test]
fn numeric_capture_replays_exactly_and_runtime_enablement_survives_rewind_but_not_restart() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("combat.db");
    let mut engine = Engine::open(&path, scenario()).unwrap();
    engine.enable_wizard().unwrap();
    engine.configure_combat_diagnostics(true).unwrap();
    run(&mut engine);
    let report = engine.inspect_combat_diagnostics(None).unwrap();
    wizard(&mut engine, WizardOperation::Rewind { target: None });
    let rewound = engine.inspect_combat_diagnostics(None).unwrap();
    assert!(rewound.enabled && rewound.records.is_empty());
    run(&mut engine);
    let replayed = engine.inspect_combat_diagnostics(None).unwrap();
    // Rewind preserves future intention namespaces, so payment owners are fresh.
    // Numerical resolution remains identical despite those fresh identities.
    assert_eq!(replayed.records.len(), report.records.len());
    for (actual, expected) in replayed.records.iter().zip(&report.records) {
        assert_eq!(actual.trace, expected.trace);
        assert_eq!(
            (actual.tick, actual.actor, actual.target, actual.damage),
            (
                expected.tick,
                expected.actor,
                expected.target,
                expected.damage
            )
        );
    }
    engine.flush().unwrap();
    let state = engine.state(ActorId(1)).unwrap();
    drop(engine);
    let mut restored = Engine::open(&path, scenario()).unwrap();
    assert_eq!(restored.state(ActorId(1)).unwrap(), state);
    assert_eq!(
        restored.inspect_combat_diagnostics(None).unwrap_err().code,
        ErrorCode::Unauthorized
    );
    restored.enable_wizard().unwrap();
    let report = restored.inspect_combat_diagnostics(None).unwrap();
    assert!(!report.enabled && report.records.is_empty());
}

#[test]
fn rejected_commands_and_rewind_do_not_resurrect_disabled_capture() {
    let mut engine = Engine::memory(scenario()).unwrap();
    engine.enable_wizard().unwrap();
    engine.configure_combat_diagnostics(true).unwrap();
    run(&mut engine);
    let report = engine.inspect_combat_diagnostics(None).unwrap();
    let state = engine.state(ActorId(1)).unwrap();
    let history = engine.history(ActorId(1), "player", None, 100).unwrap();
    let expected_revision = engine.revision(ActorId(1)).unwrap();
    let failed = support::submit(
        &mut engine,
        ActorId(1),
        Command::Wizard {
            expected_revision,
            operation: WizardOperation::Teleport {
                actor: ActorId(999),
                position: tor_server::journal::Position {
                    region: 1,
                    x: 1,
                    y: 1,
                    z: 0,
                },
            },
        },
    );
    assert!(failed.is_err());
    assert_eq!(engine.inspect_combat_diagnostics(None).unwrap(), report);
    assert_eq!(engine.state(ActorId(1)).unwrap(), state);
    assert_eq!(
        engine.history(ActorId(1), "player", None, 100).unwrap(),
        history
    );
    engine.configure_combat_diagnostics(false).unwrap();
    wizard(&mut engine, WizardOperation::Rewind { target: None });
    run(&mut engine);
    let report = engine.inspect_combat_diagnostics(None).unwrap();
    assert!(!report.enabled && report.records.is_empty() && report.captured == 0);
}
