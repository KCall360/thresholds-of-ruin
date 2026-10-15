//! Opt-in, bounded transaction observations. Never saved or sent to ordinary clients.
use crate::{resources::ByResource, ActorId, Game};
use std::collections::BTreeMap;

/// Limits diagnostic allocation independently of gameplay limits.
pub const MAX_ACTORS: usize = 1024;
pub const MAX_ITEMS_PER_ACTOR: usize = 128;
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct SurvivalFlows {
    #[serde(skip)]
    pub(crate) incomplete: bool,
    pub active_ticks: u64,
    pub periodic_health: u64,
    pub periodic_resources: ByResource<u64>,
    pub nutrition_spent: u64,
    pub nutrition_time_demand: u64,
    pub nutrition_regeneration_demand: u64,
    pub nutrition_clipped: u64,
    pub paid_start: ByResource<u64>,
    pub paid_resolution: ByResource<u64>,
    pub holds_acquired: ByResource<u64>,
    pub holds_released: ByResource<u64>,
    pub direct_health: u64,
    pub direct_resources: ByResource<u64>,
    pub nourishment_gain: u64,
    pub nourishment_debt_cleared: u64,
    pub nourishment_offered: u64,
    pub nourishment_wasted: u64,
    pub items_consumed: u64,
    pub food_consumed: u64,
    pub consumed_items: BTreeMap<crate::ItemId, u64>,
    pub nutrition_before: Option<crate::nutrition::Nutrition>,
    pub nutrition_after: Option<crate::nutrition::Nutrition>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct SurvivalBatch {
    pub incomplete: bool,
    pub actors: BTreeMap<ActorId, SurvivalFlows>,
}
impl SurvivalFlows {
    pub(crate) fn active(&mut self, outcome: &crate::creatures::ActiveTimeOutcome) {
        let clipped = outcome
            .nutrition_time_demand
            .checked_add(outcome.nutrition_regeneration_demand)
            .and_then(|demand| demand.checked_sub(u64::from(outcome.nutrition_spent)));
        let mut flow = Self {
            incomplete: outcome.diagnostics_incomplete || clipped.is_none(),
            active_ticks: outcome.active_ticks,
            periodic_health: u64::from(outcome.regenerated.health),
            nutrition_spent: u64::from(outcome.nutrition_spent),
            nutrition_time_demand: outcome.nutrition_time_demand,
            nutrition_regeneration_demand: outcome.nutrition_regeneration_demand,
            nutrition_clipped: clipped.unwrap_or(u64::MAX),
            ..Default::default()
        };
        for resource in crate::resources::Resource::ALL {
            *flow.periodic_resources.get_mut(resource) =
                u64::from(*outcome.regenerated.resources.get(resource));
        }
        self.merge(&flow);
    }
    pub(crate) fn effect(
        &mut self,
        effect: &crate::abilities::ResolvedEffect,
        before: Option<crate::nutrition::Nutrition>,
    ) {
        use crate::abilities::ResolvedEffect;
        let mut flow = Self::default();
        match effect {
            ResolvedEffect::Heal { amount } => flow.direct_health = u64::from(*amount),
            ResolvedEffect::Restore { resource, amount } => {
                *flow.direct_resources.get_mut(*resource) = u64::from(*amount)
            }
            ResolvedEffect::Nourish { amount } => {
                flow.nourishment_gain = u64::from(*amount);
                if *amount > 0 {
                    if let Some(crate::nutrition::Nutrition::Required { reserve, .. }) = before {
                        flow.nourishment_debt_cleared =
                            u64::try_from(-i64::from(reserve).min(0)).unwrap();
                    }
                }
            }
            _ => {}
        }
        self.merge(&flow);
    }
    pub(crate) fn boundary(&mut self, before: &crate::Actor, after: &crate::Actor) {
        self.nutrition_before = before.combat.as_ref().map(|c| c.creature().nutrition());
        self.nutrition_after = after.combat.as_ref().map(|c| c.creature().nutrition());
        let old = before
            .combat
            .as_ref()
            .map(|c| c.creature().costs().reservations());
        let new = after
            .combat
            .as_ref()
            .map(|c| c.creature().costs().reservations());
        if let Some(old) = old {
            for (id, cost) in old {
                if new.is_none_or(|new| !new.contains_key(id)) {
                    checked_add(
                        self.holds_released.get_mut(cost.resource),
                        u64::from(cost.resolution),
                        &mut self.incomplete,
                    );
                }
            }
        }
        for resource in crate::resources::Resource::ALL {
            // Completion pays the hold; only cancellation releases unpaid units.
            *self.holds_released.get_mut(resource) = self
                .holds_released
                .get(resource)
                .saturating_sub(*self.paid_resolution.get(resource));
        }
        if let Some(new) = new {
            for (id, cost) in new {
                if old.is_none_or(|old| !old.contains_key(id)) {
                    checked_add(
                        self.holds_acquired.get_mut(cost.resource),
                        u64::from(cost.resolution),
                        &mut self.incomplete,
                    );
                }
            }
        }
    }
    fn merge(&mut self, other: &Self) -> bool {
        let mut complete = !self.incomplete && !other.incomplete;
        fn add(left: &mut u64, right: u64, complete: &mut bool) {
            match left.checked_add(right) {
                Some(sum) => *left = sum,
                None => {
                    *left = u64::MAX;
                    *complete = false;
                }
            }
        }
        macro_rules! scalars { ($($field:ident),* $(,)?) => { $(add(&mut self.$field, other.$field, &mut complete);)* }; }
        scalars!(
            active_ticks,
            periodic_health,
            nutrition_spent,
            nutrition_time_demand,
            nutrition_regeneration_demand,
            nutrition_clipped,
            direct_health,
            nourishment_gain,
            nourishment_debt_cleared,
            nourishment_offered,
            nourishment_wasted,
            items_consumed,
            food_consumed
        );
        for resource in crate::resources::Resource::ALL {
            macro_rules! pools { ($($field:ident),* $(,)?) => { $(add(self.$field.get_mut(resource), *other.$field.get(resource), &mut complete);)* }; }
            pools!(
                periodic_resources,
                paid_start,
                paid_resolution,
                holds_acquired,
                holds_released,
                direct_resources
            );
        }
        for (item, quantity) in &other.consumed_items {
            if !self.consumed_items.contains_key(item)
                && self.consumed_items.len() >= MAX_ITEMS_PER_ACTOR
            {
                complete = false;
                continue;
            }
            add(
                self.consumed_items.entry(*item).or_default(),
                *quantity,
                &mut complete,
            );
        }
        if self.nutrition_before.is_none() {
            self.nutrition_before = other.nutrition_before;
        }
        if other.nutrition_after.is_some() {
            self.nutrition_after = other.nutrition_after;
        }
        self.incomplete = !complete;
        complete
    }
}
impl Game {
    pub(crate) fn record_survival(&mut self, actor: ActorId, flow: &SurvivalFlows) {
        let Some(batch) = self.survival_diagnostics.as_mut() else {
            return;
        };
        if !batch.actors.contains_key(&actor) && batch.actors.len() >= MAX_ACTORS {
            batch.incomplete = true;
            return;
        }
        if !batch.actors.entry(actor).or_default().merge(flow) {
            batch.incomplete = true;
        }
    }

    /// Hosts start a fresh collection for each private transaction candidate.
    pub fn begin_survival_diagnostics(&mut self, enabled: bool) {
        self.survival_diagnostics =
            enabled.then(|| tor_world::Shared::new(SurvivalBatch::default()));
    }
    /// Detach before retaining a rewind boundary or publishing a host transaction.
    pub fn take_survival_diagnostics(&mut self) -> Option<SurvivalBatch> {
        self.survival_diagnostics
            .take()
            .map(|batch| (*batch).clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn committed_active_time_is_recorded_but_queries_are_pure() {
        let mut game = Game::two_room(42, crate::test_creatures::content());
        let id = game
            .spawn_actor(
                tor_world::Location {
                    region: tor_world::RegionId(1),
                    position: tor_world::Position { x: 1, y: 1, z: 0 },
                },
                std::num::NonZeroU64::new(100).unwrap(),
            )
            .unwrap();
        crate::test_creatures::configure(
            &mut game,
            id,
            "neutral",
            crate::test_creatures::species(),
        );
        game.begin_survival_diagnostics(true);
        game.tick += 100;
        for _ in 0..3 {
            game.creature(id).unwrap();
        }
        assert!(game
            .survival_diagnostics
            .as_ref()
            .unwrap()
            .actors
            .is_empty());
        game.settle_actor_time(id).unwrap();
        assert_eq!(
            game.take_survival_diagnostics().unwrap().actors[&id].active_ticks,
            100
        );
    }
}

pub(crate) fn checked_add(left: &mut u64, right: u64, incomplete: &mut bool) {
    match left.checked_add(right) {
        Some(sum) => *left = sum,
        None => {
            *left = u64::MAX;
            *incomplete = true;
        }
    }
}

/// Borrows the existing pure resolver observation, never samples an operand twice.
pub(crate) struct NourishmentObserver<'a> {
    pub next: &'a mut dyn crate::resolution_diagnostics::ResolutionObserver,
    pub enabled: bool,
    pub incomplete: bool,
    pub offered: u64,
    pub wasted: u64,
}
impl crate::resolution_diagnostics::ResolutionObserver for NourishmentObserver<'_> {
    fn record(&mut self, step: crate::resolution_diagnostics::ResolutionStep<'_>) {
        self.next.record(step);
    }
    fn nourishment(&mut self, offered: u32, applied: u32) {
        if self.enabled {
            checked_add(&mut self.offered, u64::from(offered), &mut self.incomplete);
            match offered.checked_sub(applied) {
                Some(wasted) => {
                    checked_add(&mut self.wasted, u64::from(wasted), &mut self.incomplete)
                }
                None => self.incomplete = true,
            }
        }
        // Deliberately private: existing combat detail observers receive no new fields.
    }
}

#[cfg(test)]
mod flow_tests {
    use super::*;
    #[test]
    fn local_observation_overflow_invalidates_the_batch_without_panicking() {
        let mut game = Game::two_room(42, crate::test_creatures::content());
        game.begin_survival_diagnostics(true);
        let mut flow = SurvivalFlows {
            active_ticks: u64::MAX,
            ..Default::default()
        };
        flow.active(&crate::creatures::ActiveTimeOutcome {
            active_ticks: 1,
            ..Default::default()
        });
        game.record_survival(ActorId(1), &flow);
        assert!(game.take_survival_diagnostics().unwrap().incomplete);
    }
    #[test]
    fn bounded_counter_overflow_and_item_pressure_are_explicit() {
        let mut total = SurvivalFlows {
            active_ticks: u64::MAX,
            ..Default::default()
        };
        assert!(!total.merge(&SurvivalFlows {
            active_ticks: 1,
            ..Default::default()
        }));
        assert_eq!(total.active_ticks, u64::MAX);
        let mut total = SurvivalFlows::default();
        for id in 1..=MAX_ITEMS_PER_ACTOR {
            assert!(total.merge(&SurvivalFlows {
                consumed_items: [(crate::ItemId(id as u64), 1)].into(),
                ..Default::default()
            }));
        }
        assert!(!total.merge(&SurvivalFlows {
            consumed_items: [(crate::ItemId(u64::MAX), 1)].into(),
            ..Default::default()
        }));
        assert_eq!(total.consumed_items.len(), MAX_ITEMS_PER_ACTOR);
        let mut game = Game::two_room(42, crate::test_creatures::content());
        game.begin_survival_diagnostics(true);
        for id in 1..=MAX_ACTORS + 1 {
            game.record_survival(ActorId(id as u64), &SurvivalFlows::default());
        }
        let result = game.take_survival_diagnostics().unwrap();
        assert!(result.incomplete);
        assert_eq!(result.actors.len(), MAX_ACTORS);
    }

    #[test]
    fn nourishment_observation_never_extends_the_existing_combat_trace() {
        use crate::resolution_diagnostics::ResolutionObserver;
        let mut trace = crate::resolution_diagnostics::ResolutionTrace::default();
        let mut observer = NourishmentObserver {
            next: &mut trace,
            enabled: true,
            incomplete: false,
            offered: 0,
            wasted: 0,
        };
        observer.nourishment(1100, 4);
        assert_eq!((observer.offered, observer.wasted), (1100, 1096));
        assert!(!observer.incomplete);
        assert!(trace.steps().is_empty());
    }

    #[test]
    fn speculative_clone_counters_never_mutate_the_live_collection() {
        let mut game = Game::two_room(42, crate::test_creatures::content());
        game.begin_survival_diagnostics(true);
        let mut candidate = game.clone();
        candidate.record_survival(
            ActorId(1),
            &SurvivalFlows {
                items_consumed: 1,
                ..Default::default()
            },
        );
        assert!(game.take_survival_diagnostics().unwrap().actors.is_empty());
        assert_eq!(
            candidate.take_survival_diagnostics().unwrap().actors[&ActorId(1)].items_consumed,
            1
        );
    }

    #[test]
    fn direct_healing_stays_separate_from_periodic_settlement_and_checkpoints() {
        let mut game = Game::two_room(42, crate::test_creatures::content());
        let id = game
            .spawn_actor(
                tor_world::Location {
                    region: tor_world::RegionId(1),
                    position: tor_world::Position { x: 1, y: 1, z: 0 },
                },
                std::num::NonZeroU64::new(100).unwrap(),
            )
            .unwrap();
        crate::test_creatures::configure(
            &mut game,
            id,
            "neutral",
            crate::test_creatures::species(),
        );
        game.begin_survival_diagnostics(true);
        let mut outcome = game.actor_outcome(id).unwrap();
        let mut placement = crate::abilities::Placement {
            location: outcome.state.location,
            orientation: outcome.state.orientation,
        };
        outcome
            .state
            .combat
            .as_mut()
            .unwrap()
            .apply_effect(
                id,
                &crate::abilities::ResolvedEffect::Damage { amount: 1 },
                &mut outcome.state.public_plan,
                &mut placement,
                None,
            )
            .unwrap();
        outcome
            .state
            .combat
            .as_mut()
            .unwrap()
            .start_preparation_cost(
                crate::IntentionId(90),
                crate::costs::ResourceCost {
                    resource: crate::resources::Resource::Stamina,
                    start: 1,
                    resolution: 0,
                },
            )
            .unwrap();
        outcome
            .state
            .combat
            .as_mut()
            .unwrap()
            .with_recovery_context(None, |creature| {
                creature.cancel_cost(crate::IntentionId(90));
                Ok::<_, crate::GameError>(())
            })
            .unwrap();
        game.apply_outcome(outcome).unwrap();
        game.apply_effects(
            id,
            &[crate::rules::Effect::Restore {
                resource: crate::resources::Resource::Stamina,
                amount: crate::rules::AmountExpr::Fixed { amount: 1000 },
            }],
        )
        .unwrap();
        game.apply_effects(
            id,
            &[crate::rules::Effect::Heal {
                amount: crate::rules::AmountExpr::Fixed { amount: 1000 },
            }],
        )
        .unwrap();
        let mut shared = crate::checkpoint::SharedState::default();
        let snapshot = game.checkpoint(&mut shared);
        let mut restored =
            Game::restore_checkpoint(snapshot, &shared, game.content().clone()).unwrap();
        assert!(restored.take_survival_diagnostics().is_none());
        let flows = game.take_survival_diagnostics().unwrap();
        assert_eq!(flows.actors[&id].direct_health, 1);
        assert_eq!(flows.actors[&id].periodic_health, 0);
        assert_eq!(flows.actors[&id].direct_resources.stamina, 1);
        assert_eq!(flows.actors[&id].periodic_resources.stamina, 0);
        assert_eq!(flows.actors[&id].nutrition_regeneration_demand, 0);
    }
}
