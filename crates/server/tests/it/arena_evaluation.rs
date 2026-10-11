use crate::support;
use tor_server::{
    arena_evaluation,
    scenario_package::{ArenaControl, Package},
    Scenario,
};

fn scenario(actions: u64, ticks: u64) -> Scenario {
    let mut scenario = support::load("mob-arena", 42);
    let package = scenario.package.take().unwrap();
    let mut manifest = package.manifest.clone();
    let arena = manifest.arena.as_mut().unwrap();
    arena.control = ArenaControl::AllAi;
    arena.start_paused = false;
    arena.actions = actions;
    arena.ticks = ticks;
    scenario.package = Some(std::sync::Arc::new(
        Package::from_parts(manifest, package.region_defs().unwrap()).unwrap(),
    ));
    scenario
}

#[test]
fn ordinary_engine_arena_report_is_deterministic_bounded_and_conserves_damage() {
    let report = arena_evaluation::run(scenario(32, 100000)).unwrap();
    assert_eq!(report, arena_evaluation::run(scenario(32, 100000)).unwrap());
    assert_eq!(report.actions, 32);
    assert_eq!(
        report.termination,
        arena_evaluation::ArenaTermination::ActionLimit
    );
    assert_eq!(report.participants.len(), 4);
    let dealt: u64 = report.participants.iter().map(|p| p.damage_dealt).sum();
    let received: u64 = report.participants.iter().map(|p| p.damage_received).sum();
    assert!(dealt > 0);
    assert_eq!(dealt, received);
    for participant in &report.participants {
        assert_eq!(
            u64::from(participant.initial.health - participant.final_state.health),
            participant.damage_received
        );
        assert!(participant
            .final_state
            .resources
            .iter()
            .all(|r| r.reserved == 0));
    }
    assert!(report
        .participants
        .iter()
        .all(|p| p.hits <= p.attack_checks));
    assert!(report.participants.iter().any(|p| !p.abilities.is_empty()));
    let value = serde_json::to_value(&report).unwrap();
    assert_eq!(value["actions"], "32");
    assert_eq!(report.input_hash.len(), 64);
}

#[test]
fn arena_report_has_explicit_tick_limit_and_rejects_nonarenas_or_manual_control() {
    let report = arena_evaluation::run(scenario(10000, 7)).unwrap();
    assert_eq!(report.tick, 7);
    assert_eq!(
        report.termination,
        arena_evaluation::ArenaTermination::TickLimit
    );
    assert!(arena_evaluation::run(Scenario::two_room(42)).is_err());
    assert!(arena_evaluation::run(support::load("mob-arena", 42)).is_err());
    let mut changed = scenario(32, 100000);
    changed.seed = 43;
    assert_ne!(
        arena_evaluation::run(scenario(32, 100000))
            .unwrap()
            .input_hash,
        arena_evaluation::run(changed).unwrap().input_hash
    );
}

#[test]
fn arena_report_retains_elimination_evidence_and_survivor_faction() {
    let report = arena_evaluation::run(scenario(10000, 100000)).unwrap();
    let arena_evaluation::ArenaTermination::Elimination {
        winner: Some(winner),
    } = &report.termination
    else {
        panic!(
            "seeded duel should reach elimination, got {:?}",
            report.termination
        );
    };
    assert!(report.actions < 10000);
    assert!(report
        .participants
        .iter()
        .filter(|p| !p.final_state.dead)
        .all(|p| &p.faction == winner));
    assert!(report.participants.iter().any(|p| p.final_state.dead));
}
