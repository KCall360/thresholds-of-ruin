//! Paid native item completion; hosted source-free acceptance is server-owned.
use super::*;
use crate::attributes::{Attribute, Attributes};
use crate::rules::*;
use crate::{Action, ActorId, Game, ItemId, Work};
const OWNER: ActorId = ActorId(1);
const PEER: ActorId = ActorId(2);

fn fixture(strength: u16, effect: Effect) -> (Game, ItemId) {
    let mut game = super::ray_tests::fixture_with(|_, _, species| {
        let mut attributes = species.default_attributes.values();
        attributes.strength = strength;
        species.default_attributes = Attributes::new(attributes).unwrap();
    });
    let mut spec = crate::ItemSpec::ordinary("unmarked growth dose".into());
    spec.identity = "fixture.growth_dose".into();
    spec.concealed = true;
    spec.class = crate::ItemClass::Potion;
    spec.stackable = true;
    spec.consumable = Some(crate::ConsumableSpec {
        descriptors: vec![],
        discovery: crate::ItemDiscovery::OnUse,
        effects: vec![effect],
    });
    let at = game.actors[&OWNER].location;
    let item = game.place_item_stack(70, at, Some(OWNER), 8, spec).unwrap();
    assert_eq!(game.actors[&OWNER].turn_ticks.get(), 100);
    (game, item)
}
fn start(game: &mut Game, item: ItemId) -> u64 {
    let plan = game.plan_work(OWNER, Work::Drink { item }).unwrap();
    let due = game.tick() + plan.preparation();
    game.actors.get_mut(&PEER).unwrap().ready_at = game.tick();
    let id = game
        .admit_intention(OWNER, Action::Drink { item }, crate::IntentionOrigin::Human)
        .unwrap();
    let execution = game.execute_next_intention().unwrap();
    assert_eq!(execution.intention.id, id);
    execution.outcome.unwrap();
    assert_eq!(game.preparation(OWNER).unwrap().plan, plan);
    due
}
fn finish(game: &mut Game, due: u64) {
    game.with_recovery_transition(true, |candidate| {
        candidate.tick = due;
        candidate.resolve_preparations()
    })
    .unwrap();
}
fn drink(game: &mut Game, item: ItemId) -> ResolvedEffect {
    let quantity = game.items[&item].quantity;
    let rng = game.rules_rng;
    let due = start(game, item);
    assert_eq!(game.items[&item].quantity, quantity);
    finish(game, due);
    assert_eq!(game.items[&item].quantity, quantity - 1);
    assert!(game.actors[&OWNER]
        .knowledge
        .contains("fixture.growth_dose"));
    assert_eq!(game.rules_rng, rng);
    game.combat_events()
        .iter()
        .find_map(|event| match event {
            crate::combat::CombatEvent::Resolved { outcome, .. } => outcome
                .effects
                .iter()
                .find(|effect| {
                    matches!(
                        effect,
                        ResolvedEffect::GainStrength { .. } | ResolvedEffect::RaiseLevel { .. }
                    )
                })
                .cloned(),
            _ => None,
        })
        .expect("canonical growth receipt")
}

#[test]
fn paid_strength_dose_acquires_six_and_seven_then_cap_is_paid_noop() {
    let (mut game, item) = fixture(5, Effect::GainStrength);
    assert_eq!(game.creature(OWNER).unwrap().build().strength_acquired(), 0);
    for (natural, acquired, change) in [
        (6, 1, StrengthGain::Acquired),
        (7, 2, StrengthGain::Acquired),
        (7, 2, StrengthGain::Unchanged),
    ] {
        let receipt = drink(&mut game, item);
        let creature = game.creature(OWNER).unwrap();
        assert_eq!(creature.build().strength_acquired(), acquired);
        assert_eq!(
            creature.derived().attributes.get(Attribute::Strength),
            natural
        );
        assert_eq!(receipt, ResolvedEffect::GainStrength { change });
    }
}

#[test]
fn paid_strength_restores_one_deficit_exclusively_before_acquiring() {
    let (mut game, item) = fixture(4, Effect::GainStrength);
    game.apply_effects(
        OWNER,
        &[Effect::LoseAttribute {
            attribute: Attribute::Strength,
            amount: AmountExpr::Fixed { amount: 2 },
        }],
    )
    .unwrap();
    assert_eq!(
        game.creature(OWNER)
            .unwrap()
            .restorable_losses()
            .attributes
            .strength,
        2
    );
    for deficit in [1, 0] {
        assert_eq!(
            drink(&mut game, item),
            ResolvedEffect::GainStrength {
                change: StrengthGain::Restored
            }
        );
        let creature = game.creature(OWNER).unwrap();
        assert_eq!(creature.restorable_losses().attributes.strength, deficit);
        assert_eq!(creature.build().strength_acquired(), 0);
    }
    assert_eq!(
        drink(&mut game, item),
        ResolvedEffect::GainStrength {
            change: StrengthGain::Acquired
        }
    );
    assert_eq!(game.creature(OWNER).unwrap().build().strength_acquired(), 1);
}

#[test]
fn paid_raise_level_stacks_one_additional_funded_level_while_disabled() {
    let (mut game, item) = fixture(3, Effect::RaiseLevel);
    assert!(matches!(
        game.creature(OWNER).unwrap().build().future_growth(),
        crate::earned_growth::FutureGrowth::Disabled {}
    ));
    let baseline = game.creature(OWNER).unwrap().build().ledger().total_hd();
    let policy = game.content.rules.document().growth.clone();
    let mut total = 0;
    for offset in 0..3 {
        let tier = baseline.checked_add(offset).unwrap();
        let price = policy.threshold(tier).unwrap();
        assert!(price > 0);
        let receipt = drink(&mut game, item);
        total += price;
        let creature = game.creature(OWNER).unwrap();
        assert_eq!(creature.build().ledger().total_hd(), baseline);
        assert_eq!(creature.earned_progress().held_xp(), total);
        assert_eq!(receipt, ResolvedEffect::RaiseLevel { amount: price });
        assert_eq!(creature.earned_progress().earned_xp(), total);
        assert_eq!(
            creature.earned_progress().highest_earned_tier(),
            tier.checked_add(1).unwrap()
        );
    }
}

#[test]
fn growth_effect_receipts_and_unit_plans_are_strict() {
    for effect in [
        ResolvedEffect::GainStrength {
            change: StrengthGain::Acquired,
        },
        ResolvedEffect::RaiseLevel { amount: u64::MAX },
    ] {
        let value = serde_json::to_value(&effect).unwrap();
        assert_eq!(
            serde_json::from_value::<ResolvedEffect>(value.clone()).unwrap(),
            effect
        );
        let mut extra = value.clone();
        extra["extra"] = true.into();
        assert!(serde_json::from_value::<ResolvedEffect>(extra).is_err());
        let mut missing = value;
        missing.as_object_mut().unwrap().remove(
            if matches!(effect, ResolvedEffect::GainStrength { .. }) {
                "change"
            } else {
                "amount"
            },
        );
        assert!(serde_json::from_value::<ResolvedEffect>(missing).is_err());
    }
    for effect in [Effect::GainStrength, Effect::RaiseLevel] {
        let value = serde_json::to_value(&effect).unwrap();
        assert_eq!(
            serde_json::from_value::<Effect>(value.clone()).unwrap(),
            effect
        );
        let mut forged = value;
        forged["amount"] = 1.into();
        assert!(serde_json::from_value::<Effect>(forged).is_err());
    }
}
#[test]
fn growth_paid_pending_checkpoint_replays_exact_provenance_and_banked_credit() {
    for effect in [Effect::GainStrength, Effect::RaiseLevel] {
        let (mut game, item) = fixture(4, effect);
        let due = start(&mut game, item);
        let mut shared = crate::checkpoint::SharedState::default();
        let snapshot = game.checkpoint(&mut shared);
        let encoded = serde_json::to_vec(&(snapshot, &shared)).unwrap();
        let (snapshot, shared): (crate::checkpoint::Snapshot, crate::checkpoint::SharedState) =
            serde_json::from_slice(&encoded).unwrap();
        let mut restored =
            Game::restore_checkpoint(snapshot, &shared, game.content().clone()).unwrap();
        assert_eq!(game, restored);
        finish(&mut game, due);
        finish(&mut restored, due);
        assert_eq!(game, restored);
        assert_eq!(restored.items[&item].quantity, 7);
    }
}
#[test]
fn strength_late_build_epoch_failure_rolls_back_paid_completion_and_ownership() {
    let (mut game, item) = fixture(4, Effect::GainStrength);
    let due = start(&mut game, item);
    let mut record = serde_json::to_value(crate::creatures::record::CreatureRecord::capture(
        &game.creature(OWNER).unwrap(),
    ))
    .unwrap();
    record["build_epoch"] = serde_json::json!(u64::MAX);
    let exhausted = serde_json::from_value::<crate::creatures::record::CreatureRecord>(record)
        .unwrap()
        .restore(game.content.clone(), vec![])
        .unwrap();
    let identity = game.actors[&OWNER].combat.as_ref().unwrap().identity();
    game.actors.get_mut(&OWNER).unwrap().combat = Some(
        crate::actor_creatures::CombatState::from_creature(identity, exhausted),
    );
    let before = game.clone();
    assert!(game
        .with_effect_publication(|candidate| {
            candidate.tick = due;
            candidate.resolve_preparations()
        })
        .is_err());
    assert_eq!(game, before);
    assert_eq!(game.items[&item].quantity, 8);
    assert_eq!(game.creature(OWNER).unwrap().build().strength_acquired(), 0);
}
#[test]
fn paid_raise_level_uses_selected_growth_and_later_source_reenable_spends_banked_levels() {
    use crate::earned_growth::{FutureGrowth, GrowthSource};
    let (mut game, item) = fixture(3, Effect::RaiseLevel);
    let baseline = game.creature(OWNER).unwrap().build().ledger().total_hd();
    let policy = game.content.rules.document().growth.clone();
    let first = policy.threshold(baseline).unwrap();
    let second = policy.threshold(baseline.checked_add(1).unwrap()).unwrap();
    assert_eq!(
        drink(&mut game, item),
        ResolvedEffect::RaiseLevel { amount: first }
    );
    assert_eq!(
        drink(&mut game, item),
        ResolvedEffect::RaiseLevel { amount: second }
    );
    let mut build = game.creature(OWNER).unwrap().build().clone();
    build
        .set_future_growth(FutureGrowth::Selected {
            source: GrowthSource::Racial {},
        })
        .unwrap();
    game.rebuild_creature(OWNER, build).unwrap();
    assert_eq!(
        game.creature(OWNER).unwrap().build().ledger().total_hd(),
        baseline.checked_add(2).unwrap()
    );
    assert_eq!(game.creature(OWNER).unwrap().earned_progress().held_xp(), 0);
    assert_eq!(
        game.creature(OWNER).unwrap().earned_progress().spent_xp(),
        first + second
    );
    let third = policy.threshold(baseline.checked_add(2).unwrap()).unwrap();
    assert_eq!(
        drink(&mut game, item),
        ResolvedEffect::RaiseLevel { amount: third }
    );
    assert_eq!(
        game.creature(OWNER).unwrap().build().ledger().total_hd(),
        baseline.checked_add(3).unwrap()
    );
}
#[test]
fn direct_growth_return_uses_canonical_applied_change_not_pure_placeholder() {
    let (mut game, _) = fixture(5, Effect::GainStrength);
    assert!(game.apply_effects(OWNER, &[Effect::GainStrength]).unwrap());
    assert!(game.apply_effects(OWNER, &[Effect::GainStrength]).unwrap());
    assert!(!game.apply_effects(OWNER, &[Effect::GainStrength]).unwrap());
    assert_eq!(game.creature(OWNER).unwrap().build().strength_acquired(), 2);
    assert!(game.apply_effects(OWNER, &[Effect::RaiseLevel]).unwrap());
    assert!(game.creature(OWNER).unwrap().earned_progress().held_xp() > 0);
}
