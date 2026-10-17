//! Relationship-owned damage and immutable sequence identity.
use super::*;
use crate::rules::*;
use crate::{ActorId, Game};

fn policy() -> crate::linked_hold::HoldPolicy {
    crate::linked_hold::HoldPolicy {
        sustaining: crate::content_ids::FlagId::new("magical").unwrap(),
        preparation: 100, recovery: 100, protection: 400,
    }
}
fn fixture(reliable: bool) -> Game {
    super::strike_tests::sequence_game_custom(&[0, 10, 20, 30], |ability| {
        let StrikeSource::Authored { strikes } = &mut ability.strikes else { unreachable!() };
        for strike in strikes {
            strike.check = Some(CheckDef { actor: CheckActor::User,
                skill: SkillSelector::Named { skill: crate::attributes::Skill::Athletics },
                against: Against::Defense { defense: DefenseKind::Physical } });
            strike.on_success = vec![Effect::HoldDamage { category: crate::combat::DamageType::Impact, descriptor: None }];
            strike.riders = vec![RiderDef { trigger: RiderTrigger::Success, condition: None, check: None,
                effects: vec![Effect::ApplyLinkedHold { policy: policy() }], descriptors: vec![] }];
        }
    }, |_, species| {
        species.modifiers.push(Modifier { scope: Scope::Always, op: ModifierOp::Flag { flag: policy().sustaining } });
        for (stat, value) in [(AbilityStat::HoldDamageIncrement, 1), (AbilityStat::CheckBonus, if reliable { 100 } else { 0 })] {
            species.modifiers.push(Modifier { scope: Scope::Always, op: ModifierOp::Add {
                stat: Stat::Ability { ability: crate::content_ids::AbilityId::new("basic_melee").unwrap(), stat }, value } });
        }
    }).0
}
fn resolve_at(game: &mut Game, tick: u64) {
    game.tick = tick;
    game.actors.get_mut(&ActorId(2)).unwrap().ready_at = tick;
    game.resolve_preparations().unwrap();
}
fn primary_success(game: &Game) -> bool {
    game.combat_events().iter().rev().find_map(|event| match event {
        crate::combat::CombatEvent::Resolved { outcome, strike: Some(_), .. } => outcome.check.map(|check| check.success),
        _ => None,
    }).unwrap()
}
#[test]
fn hold_damage_initial_zero_miss_increments_once_later_hit_uses_exact_bonus() {
    let original = fixture(false);
    let mut found = false;
    for seed in 1..=256 {
        let mut game = original.clone(); game.rules_rng = seed;
        resolve_at(&mut game, 10);
        if !primary_success(&game) { continue; }
        let id = game.linked_holds(ActorId(2))[0].id;
        assert_eq!(game.linked_hold(ActorId(2), id).unwrap().escalation, 0);
        assert_eq!(game.health(ActorId(2)).unwrap().0, 100);
        resolve_at(&mut game, 20);
        if primary_success(&game) { continue; }
        assert_eq!(game.linked_hold(ActorId(2), id).unwrap().escalation, 1);
        assert_eq!(game.health(ActorId(2)).unwrap().0, 100);
        let once = game.clone(); game.resolve_preparations().unwrap(); assert_eq!(game, once);
        resolve_at(&mut game, 30);
        if !primary_success(&game) { continue; }
        assert_eq!(game.linked_hold(ActorId(2), id).unwrap().escalation, 2);
        assert_eq!(game.health(ActorId(2)).unwrap().0, 98);
        found = true; break;
    }
    assert!(found, "bounded seeds include ordinary hit/miss/hit");
}
#[test]
fn hold_damage_retained_sequence_never_rebinds_replacement_and_decode_requires_creation_branch() {
    let mut game = fixture(true);
    resolve_at(&mut game, 10);
    let old = game.linked_holds(ActorId(2))[0].id;
    resolve_at(&mut game, 20);
    assert_eq!(game.linked_hold(ActorId(2), old).unwrap().escalation, 1);
    let pending = game.preparation(ActorId(1)).unwrap();
    assert_eq!(pending.progress.hold_binding().unwrap().id, old);
    let mut wire = serde_json::to_value(pending).unwrap();
    wire["progress"]["linked_hold"]["created_at"].as_object_mut().unwrap().remove("primary_success");
    assert!(serde_json::from_value::<crate::combat::Preparation>(wire).is_err());
    game.release_linked_hold(ActorId(2), old).unwrap();
    let replacement = game.apply_linked_hold(ActorId(1), ActorId(2), policy()).unwrap().unwrap();
    assert_ne!(old, replacement);
    let health = game.health(ActorId(2)).unwrap().0;
    resolve_at(&mut game, 30);
    assert_eq!(game.health(ActorId(2)).unwrap().0, health);
    assert_eq!(game.linked_hold(ActorId(2), replacement).unwrap().escalation, 0);
    assert_eq!(game.preparation(ActorId(1)).unwrap().progress.hold_binding().unwrap().id, old);
}
#[test]
fn hold_damage_authored_shape_rejects_rider_failure_duplicate_and_ranged_channels() {
    let game = fixture(true);
    let document = game.content().rules().document().clone();
    for case in 0..4 {
        let mut changed = document.clone();
        let ability = changed.abilities.iter_mut().find(|ability| ability.basic_attack).unwrap();
        let StrikeSource::Authored { strikes } = &mut ability.strikes else { unreachable!() };
        let effect = strikes[0].on_success[0].clone();
        match case {
            0 => strikes[0].on_failure.push(effect),
            1 => strikes[0].riders[0].effects.push(effect),
            2 => strikes[0].on_success.push(effect),
            _ => ability.reach = Reach::Sight { range: 5 },
        }
        assert!(Ruleset::validate(changed).is_err(), "case {case}");
    }
}
