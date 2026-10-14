//! Shared diagnostic capture follows frozen plan recipients.
use super::*;
#[test]
fn self_step_diagnostic_uses_its_frozen_self_recipient_and_timing() {
    for blocked in [false, true] {
        let mut game = guarded_step_game(100);
        game.set_combat_diagnostics(true);
        let intention = start_step(&mut game, "north");
        if blocked {
            game.set_wall(cell(2, 1, 0), true).unwrap();
        }
        game.act(crate::ActorId(2), crate::Action::Wait).unwrap();
        let records = game.combat_diagnostics().unwrap().records();
        assert_eq!(
            records.len(),
            1,
            "self selection remains an ordinary primary diagnostic"
        );
        let record = records.back().unwrap();
        assert_eq!(
            (record.actor, record.target),
            (crate::ActorId(1), crate::ActorId(1))
        );
        assert_eq!(
            (
                record.started,
                record.tick,
                record.preparation,
                record.recovery
            ),
            (0, 100, 100, 200)
        );
        assert_eq!(record.intention, Some(intention));
        assert!(record.outcome.effects.iter().any(|effect|matches!(effect,
            crate::abilities::ResolvedEffect::Translate {mode:crate::abilities::MovementMode::Voluntary,result}
                if matches!(result,crate::abilities::MovementResult::Blocked)==blocked)));
    }
}
