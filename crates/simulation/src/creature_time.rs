//! Relative creature timers follow active simulation ticks, never wall time or
//! the elapsed global time while an actor is frozen or detached.
use crate::{ActorId, Game, GameError};

#[cfg(test)]
thread_local! {
    static TIMER_SETTLEMENTS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
pub(crate) fn count_settlement() {
    TIMER_SETTLEMENTS.with(|count| count.set(count.get() + 1));
}

impl Game {
    /// Absolute deadlines are updated only when their actor changes. Frozen
    /// actors keep their remaining interval and cannot wake the simulation.
    pub fn next_creature_event_tick(&self) -> Option<u64> {
        self.actors
            .scheduled_timers()
            .next()
            .map(|(deadline, _)| deadline)
    }

    /// Settle only expired deadlines. Fractional progress on all other actors
    /// remains lazy until they are touched or projected for an observation.
    pub(crate) fn settle_creature_time(&mut self) -> Result<(), GameError> {
        let due: Vec<_> = self
            .actors
            .scheduled_timers()
            .take_while(|(deadline, _)| *deadline <= self.tick)
            .map(|(_, id)| id)
            .collect();
        for id in due {
            self.settle_actor_time(id)?;
        }
        Ok(())
    }

    pub(crate) fn settle_actor_time(&mut self, id: ActorId) -> Result<(), GameError> {
        let clock = self.actor_clock(id);
        if self
            .actors
            .get(&id)
            .is_none_or(|actor| clock <= actor.active_since)
        {
            return Ok(());
        }
        let outcome = self.settled_actor_outcome(id)?;
        self.apply_outcome(outcome)
    }

    /// Settle earned active work under the old nutrition policy, then rephase
    /// its fraction. Neither stopping nor resuming gives catch-up healing.
    pub fn set_recovery_nutrition(
        &mut self,
        id: ActorId,
        nutrition: crate::health_recovery::RecoveryNutrition,
    ) -> Result<(), GameError> {
        let mut outcome = self.settled_actor_outcome(id)?;
        let combat = outcome.state.combat.as_mut().ok_or(GameError::Rule(
            crate::abilities::RuleError::InvalidDefinition,
        ))?;
        combat
            .set_recovery_nutrition(nutrition)
            .map_err(|_| GameError::Rule(crate::abilities::RuleError::InvalidDefinition))?;
        self.apply_outcome(outcome)
    }

    pub fn restore_resource(
        &mut self,
        id: ActorId,
        resource: crate::resources::Resource,
        amount: u32,
    ) -> Result<u32, GameError> {
        let mut outcome = self.settled_actor_outcome(id)?;
        if !outcome.state.alive() {
            return Err(GameError::UnknownActor);
        }
        let combat = outcome.state.combat.as_mut().ok_or(GameError::Rule(
            crate::abilities::RuleError::InvalidDefinition,
        ))?;
        let restored = combat.restore_resource(resource, amount);
        self.apply_outcome(outcome)?;
        Ok(restored)
    }

    /// Trusted effect application. A causer can be dead or detached; an existing
    /// condition needs neither current reach nor sight of its causer.
    pub fn apply_condition(
        &mut self,
        actor: ActorId,
        condition: &crate::content_ids::ConditionId,
        source: ActorId,
        duration: u64,
    ) -> Result<crate::conditions::ConditionUpdate, GameError> {
        if !self.actors.contains_key(&source) && !self.detached_actor(source) {
            return Err(GameError::UnknownActor);
        }
        let mut outcome = self.settled_actor_outcome(actor)?;
        if !outcome.state.alive() {
            return Err(GameError::UnknownActor);
        }
        let combat = outcome.state.combat.as_mut().ok_or(GameError::Rule(
            crate::abilities::RuleError::InvalidDefinition,
        ))?;
        let update = combat.apply_condition(condition, source, duration)?;
        self.apply_outcome(outcome)?;
        Ok(update)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::attributes::ManaBinding;
    use crate::content_ids::ClassId;
    use crate::creatures::CreatureBuild;
    use crate::progression::{HdLedger, HdSource};
    use std::{collections::BTreeSet, num::NonZeroU64};
    use tor_world::{Location, Position, RegionId};

    #[test]
    fn expiring_temporary_mana_cancels_completion_identically_with_moving_physics() {
        use crate::{
            content_ids::{AbilityId, ConditionId},
            rules::{
                ConditionDef, Eviction, Modifier, ModifierOp, Scope, Stacking, Stat, TimingSource,
            },
        };
        let original = timer_build();
        let borrowed = ConditionId::new("borrowed_mana").unwrap();
        let bolt = AbilityId::new("magic_bolt").unwrap();
        let mut document = original.ruleset().document().clone();
        let mana = document
            .resources
            .iter_mut()
            .find(|resource| resource.resource == crate::resources::Resource::Mana)
            .unwrap();
        mana.capacity.base = 0;
        mana.capacity.required_flag = None;
        let definition = document
            .abilities
            .iter_mut()
            .find(|ability| ability.id == bolt)
            .unwrap();
        definition.timing.source = TimingSource::Fixed {
            prepare: 10,
            recover: 1,
        };
        definition.timing.scaled_by = None;
        document.conditions.push(ConditionDef {
            id: borrowed.clone(),
            family: crate::content_ids::ConditionFamilyId::new(borrowed.as_str()).unwrap(),
            lifecycle: crate::rules::ConditionLifecycle::Timed,
            name: "Borrowed mana".into(),
            modifiers: vec![Modifier {
                scope: Scope::Always,
                op: ModifierOp::Add {
                    stat: Stat::ResourceMax {
                        resource: crate::resources::Resource::Mana,
                    },
                    value: 2,
                },
            }],
            stacking: Stacking::Longest,
            source_scoped: false,
            descriptors: vec![],
            capacity: Eviction::Oldest,
        });
        let mut catalog = original.content().catalog().clone();
        let species = catalog.species.get_mut(original.species_id()).unwrap();
        species.modifiers.extend([
            Modifier {
                scope: Scope::Always,
                op: ModifierOp::Set {
                    stat: Stat::Health,
                    value: 100,
                },
            },
            Modifier {
                scope: Scope::Always,
                op: ModifierOp::GrantAbility {
                    ability: bolt.clone(),
                },
            },
        ]);
        let content = crate::creatures::Content::new(
            crate::rules::Ruleset::validate(document).unwrap(),
            catalog,
        )
        .unwrap();
        let build = CreatureBuild::from_recorded(
            content.clone(),
            original.species_id().clone(),
            original.initial_attributes(),
            original.ledger().clone(),
            original.binding(),
            original.choices().to_vec(),
            vec![],
        )
        .unwrap();
        let mut results = Vec::new();
        for path in ["stationary", "moving", "direct_completion"] {
            let mut game = Game::two_room_in_stone(42, content.clone());
            game.set_gravity(RegionId(1), [0, 0, 0]).unwrap();
            game.set_gravity(RegionId(2), [0, 0, 0]).unwrap();
            for x in [1, 2, 3] {
                let actor = game
                    .spawn_actor(
                        Location {
                            region: RegionId(1),
                            position: Position { x, y: 1, z: 0 },
                        },
                        NonZeroU64::new(100).unwrap(),
                    )
                    .unwrap();
                if x < 3 {
                    game.configure_creature(
                        actor,
                        crate::CreatureIdentity {
                            name: "subject".into(),
                            faction: "neutral".into(),
                        },
                        build.clone(),
                    )
                    .unwrap();
                }
            }
            game.apply_condition(ActorId(1), &borrowed, ActorId(1), 10)
                .unwrap();
            game.restore_resource(ActorId(1), crate::resources::Resource::Mana, 2)
                .unwrap();
            let funded = game.creature(ActorId(1)).unwrap();
            assert_eq!(
                funded
                    .costs()
                    .resources()
                    .maximum(crate::resources::Resource::Mana),
                2
            );
            assert_eq!(
                funded
                    .costs()
                    .resources()
                    .balance(crate::resources::Resource::Mana),
                2
            );
            let intention = game
                .admit_intention(
                    ActorId(1),
                    crate::Action::UseAbility {
                        id: bolt.clone(),
                        target: ActorId(2),
                    },
                    crate::IntentionOrigin::Human,
                )
                .unwrap();
            game.execute_next_intention().unwrap().outcome.unwrap();
            assert_eq!(game.preparation(ActorId(1)).unwrap().remaining, 10);
            for actor in [ActorId(2), ActorId(3)] {
                game.actors.get_mut(&actor).unwrap().ready_at = 20;
            }
            if path == "moving" {
                game.set_actor_velocity(ActorId(3), [1, 0, 0]).unwrap();
            }
            let rng = game.rules_rng;
            if path == "direct_completion" {
                game.tick = 10;
            } else {
                assert_eq!(game.advance_physics(10).unwrap(), 10);
            }
            game.resolve_preparations().unwrap();
            assert!(game.preparation(ActorId(1)).is_none());
            let creature = game.creature(ActorId(1)).unwrap();
            assert_eq!(
                creature
                    .costs()
                    .resources()
                    .maximum(crate::resources::Resource::Mana),
                0
            );
            assert_eq!(
                creature
                    .costs()
                    .resources()
                    .balance(crate::resources::Resource::Mana),
                0
            );
            assert!(creature.costs().reservations().is_empty());
            assert_eq!(game.health(ActorId(2)).unwrap().0, 100);
            assert_eq!(
                game.rules_rng, rng,
                "canceled work cannot roll even on a physics tick"
            );
            assert_eq!(
                game.combat_events(),
                &[crate::combat::CombatEvent::Interrupted {
                    actor: ActorId(1),
                    intention: Some(intention)
                }]
            );
            results.push((
                game.creature(ActorId(1)).unwrap().into_owned(),
                game.creature(ActorId(2)).unwrap().into_owned(),
                game.combat_events().to_vec(),
            ));
        }
        assert_eq!(results[0], results[1]);
        assert_eq!(results[0], results[2]);
    }

    #[test]
    fn frozen_deadlines_are_not_visited_and_restore_thaw_reindexes_remaining_time() {
        use crate::checkpoint::SharedState;
        use crate::streaming::{MemoryRecords, RegionTransition};
        let world = tor_world::World::new(
            vec![
                tor_world::Region {
                    id: RegionId(1),
                    name: "active".into(),
                    bounds: tor_world::Extent::new(300, 3, 1).unwrap(),
                },
                tor_world::Region {
                    id: RegionId(2),
                    name: "frozen".into(),
                    bounds: tor_world::Extent::new(300, 3, 1).unwrap(),
                },
            ],
            vec![],
        )
        .unwrap();
        let build = timer_build();
        let mut game = Game::new(world, 42, build.content().clone());
        let source = game
            .spawn_actor(
                Location {
                    region: RegionId(1),
                    position: Position { x: 1, y: 1, z: 0 },
                },
                NonZeroU64::new(100).unwrap(),
            )
            .unwrap();
        let fear = crate::content_ids::ConditionId::new("fear").unwrap();
        for x in 1..=256 {
            let actor = game
                .spawn_actor(
                    Location {
                        region: RegionId(2),
                        position: Position { x, y: 1, z: 0 },
                    },
                    NonZeroU64::new(100).unwrap(),
                )
                .unwrap();
            game.configure_creature(
                actor,
                crate::CreatureIdentity {
                    name: "subject".into(),
                    faction: "neutral".into(),
                },
                build.clone(),
            )
            .unwrap();
            game.apply_condition(actor, &fear, source, 10).unwrap();
        }
        let mut records = MemoryRecords::default();
        let frozen = RegionTransition {
            active: BTreeSet::from([RegionId(1)]),
            loaded: BTreeSet::from([RegionId(1), RegionId(2)]),
        };
        game.apply_region_transition(&frozen, &mut records).unwrap();
        game.apply_region_transition(
            &RegionTransition {
                active: frozen.loaded.clone(),
                loaded: frozen.loaded.clone(),
            },
            &mut records,
        )
        .unwrap();
        assert_eq!(game.tick, 0);
        assert_eq!(
            game.next_creature_event_tick(),
            Some(10),
            "thawing in the same tick must restore deadline membership"
        );
        game.apply_region_transition(&frozen, &mut records).unwrap();
        game.tick = 100;
        game.actors.get_mut(&source).unwrap().ready_at = 100;
        // Mutating a frozen timer must not accidentally reinsert its deadline.
        game.apply_condition(ActorId(2), &fear, source, 20).unwrap();
        let before = crate::diagnostics::work_counts().timer_candidates;
        for _ in 0..8 {
            assert_eq!(game.next_creature_event_tick(), None);
            game.settle_creature_time().unwrap();
        }
        assert_eq!(
            crate::diagnostics::work_counts().timer_candidates - before,
            0
        );
        assert_eq!(game.actors[&ActorId(2)].active_since, 0);
        assert_eq!(
            game.creature(ActorId(2))
                .unwrap()
                .conditions()
                .remaining(&fear, source),
            Some(20)
        );
        let mut shared = SharedState::default();
        let snapshot = game.checkpoint(&mut shared);
        let mut restored =
            Game::restore_checkpoint(snapshot, &shared, game.content().clone()).unwrap();
        let before = crate::diagnostics::work_counts().timer_candidates;
        assert_eq!(restored.next_creature_event_tick(), None);
        restored.settle_creature_time().unwrap();
        assert_eq!(
            crate::diagnostics::work_counts().timer_candidates - before,
            0
        );
        assert_eq!(restored, game);
        restored
            .apply_region_transition(
                &RegionTransition {
                    active: BTreeSet::from([RegionId(1)]),
                    loaded: BTreeSet::from([RegionId(1)]),
                },
                &mut records,
            )
            .unwrap();
        restored
            .apply_region_transition(&frozen, &mut records)
            .unwrap();
        let before = crate::diagnostics::work_counts().timer_candidates;
        assert_eq!(restored.next_creature_event_tick(), None);
        restored.settle_creature_time().unwrap();
        assert_eq!(
            crate::diagnostics::work_counts().timer_candidates - before,
            0,
            "reattaching frozen actors cannot publish their stale deadlines"
        );
        let thawed = RegionTransition {
            active: frozen.loaded.clone(),
            loaded: frozen.loaded,
        };
        restored
            .apply_region_transition(&thawed, &mut records)
            .unwrap();
        let before = crate::diagnostics::work_counts().timer_candidates;
        assert_eq!(restored.next_creature_event_tick(), Some(110));
        assert_eq!(
            crate::diagnostics::work_counts().timer_candidates - before,
            1
        );
        assert_eq!(restored.actors[&ActorId(2)].active_since, 100);
        assert_eq!(
            restored
                .creature(ActorId(2))
                .unwrap()
                .conditions()
                .remaining(&fear, source),
            Some(20)
        );
        restored.tick = 110;
        let before = crate::diagnostics::work_counts().timer_candidates;
        restored.settle_creature_time().unwrap();
        assert_eq!(
            crate::diagnostics::work_counts().timer_candidates - before,
            256,
            "255 due actors plus one future boundary, with no frozen scan"
        );
        assert_eq!(restored.next_creature_event_tick(), Some(120));
        assert_eq!(
            game.next_creature_event_tick(),
            None,
            "thawing a restored snapshot must not change the retained frozen game"
        );
        assert_eq!(game.actors[&ActorId(2)].active_since, 0);
    }

    #[test]
    fn moving_physics_batches_timers_until_the_next_boundary() {
        let build = timer_build();
        let mut game = Game::two_room_in_stone(42, build.content().clone());
        let source = game
            .spawn_actor(
                Location {
                    region: RegionId(1),
                    position: Position { x: 1, y: 1, z: 0 },
                },
                NonZeroU64::new(99).unwrap(),
            )
            .unwrap();
        let subject = game
            .spawn_actor(
                Location {
                    region: RegionId(1),
                    position: Position { x: 2, y: 1, z: 0 },
                },
                NonZeroU64::new(150).unwrap(),
            )
            .unwrap();
        game.configure_creature(
            subject,
            crate::CreatureIdentity {
                name: "subject".into(),
                faction: "neutral".into(),
            },
            build,
        )
        .unwrap();
        game.apply_condition(
            subject,
            &crate::content_ids::ConditionId::new("fear").unwrap(),
            source,
            500,
        )
        .unwrap();
        game.set_actor_velocity(subject, [512, 0, 0]).unwrap();
        game.actors.get_mut(&subject).unwrap().ready_at = 150;
        let before = TIMER_SETTLEMENTS.with(|count| count.get());
        game.act(source, crate::Action::Wait).unwrap();
        assert_eq!(game.tick(), 99);
        assert_eq!(
            game.creature(subject).unwrap().conditions().remaining(
                &crate::content_ids::ConditionId::new("fear").unwrap(),
                source
            ),
            Some(401)
        );
        assert_eq!(
            TIMER_SETTLEMENTS.with(|count| count.get()) - before,
            0,
            "unrelated physics must not eagerly materialize creature timers"
        );
    }
    fn timer_build() -> CreatureBuild {
        let species = crate::test_creatures::unmodified_species("timer_subject");
        let id = species.id.clone();
        let content = crate::creatures::Content::new(
            crate::test_support::test_ruleset(),
            crate::creatures::ScenarioCatalog {
                species: std::collections::BTreeMap::from([(id.clone(), species)]),
                templates: Default::default(),
            },
        )
        .unwrap();
        CreatureBuild::new(
            content,
            id,
            HdLedger::seeded(vec![HdSource::Class(ClassId::new("mage").unwrap())], 7).unwrap(),
            ManaBinding::Intellect,
        )
        .unwrap()
    }

    #[test]
    fn injury_settles_only_its_target_among_many_timed_actors() {
        let world = tor_world::World::new(
            vec![tor_world::Region {
                id: RegionId(1),
                name: "arena".into(),
                bounds: tor_world::Extent::new(300, 3, 1).unwrap(),
            }],
            vec![],
        )
        .unwrap();
        let build = timer_build();
        let mut game = Game::new(world, 42, build.content().clone());
        for x in 1..=256 {
            let id = game
                .spawn_actor(
                    Location {
                        region: RegionId(1),
                        position: Position { x, y: 1, z: 0 },
                    },
                    NonZeroU64::new(100).unwrap(),
                )
                .unwrap();
            game.configure_creature(
                id,
                crate::CreatureIdentity {
                    name: "subject".into(),
                    faction: "neutral".into(),
                },
                build.clone(),
            )
            .unwrap();
            if x > 1 {
                game.apply_condition(
                    id,
                    &crate::content_ids::ConditionId::new("fear").unwrap(),
                    ActorId(1),
                    300,
                )
                .unwrap();
            }
        }
        game.tick = 50;
        let untouched = game.actors.get(&ActorId(3)).unwrap().clone();
        let count = TIMER_SETTLEMENTS.with(|count| count.get());
        crate::test_creatures::injure(&mut game, ActorId(2), 1).unwrap();
        assert_eq!(TIMER_SETTLEMENTS.with(|count| count.get()) - count, 1);
        assert_eq!(game.actors.get(&ActorId(3)), Some(&untouched));
        assert_eq!(game.actors.get(&ActorId(2)).unwrap().active_since, 50);
        assert_eq!(game.next_creature_event_tick(), Some(300));
    }
    #[test]
    fn target_health_expiry_interrupts_due_attack_during_moving_physics() {
        use crate::rules::{ConditionDef, Eviction, Modifier, ModifierOp, Scope, Stacking, Stat};
        let original = timer_build();
        let id = crate::content_ids::ConditionId::new("temporary_health").unwrap();
        let mut document = original.ruleset().document().clone();
        let attack = document
            .abilities
            .iter_mut()
            .find(|ability| ability.basic_attack)
            .unwrap();
        attack.timing.source = crate::rules::TimingSource::Fixed {
            prepare: 10,
            recover: 1,
        };
        attack.timing.scaled_by = None;
        document.conditions.push(ConditionDef {
            id: id.clone(),
            family: crate::content_ids::ConditionFamilyId::new(id.as_str()).unwrap(),
            lifecycle: crate::rules::ConditionLifecycle::Timed,
            name: "Temporary health".into(),
            modifiers: vec![Modifier {
                scope: Scope::Always,
                op: ModifierOp::Add {
                    stat: Stat::Health,
                    value: 10,
                },
            }],
            stacking: Stacking::Longest,
            source_scoped: true,
            descriptors: vec![],
            capacity: Eviction::Oldest,
        });
        let content = crate::creatures::Content::new(
            crate::rules::Ruleset::validate(document).unwrap(),
            (*original.content().catalog).clone(),
        )
        .unwrap();
        let build = CreatureBuild::from_recorded(
            content.clone(),
            original.species_id().clone(),
            original.initial_attributes(),
            original.ledger().clone(),
            original.binding(),
            original.choices().to_vec(),
            vec![],
        )
        .unwrap();
        let mut game = Game::two_room_in_stone(42, content);
        for (x, faction) in [(1, "left"), (2, "right")] {
            let actor = game
                .spawn_actor(
                    Location {
                        region: RegionId(1),
                        position: Position { x, y: 1, z: 0 },
                    },
                    NonZeroU64::new(100).unwrap(),
                )
                .unwrap();
            game.configure_creature(
                actor,
                crate::CreatureIdentity {
                    name: faction.into(),
                    faction: faction.into(),
                },
                build.clone(),
            )
            .unwrap();
        }
        let baseline = game.health(ActorId(2)).unwrap().1;
        game.apply_condition(ActorId(2), &id, ActorId(1), 10)
            .unwrap();
        crate::test_creatures::injure(&mut game, ActorId(2), baseline + 5).unwrap();
        game.set_gravity(RegionId(1), [0, 0, 0]).unwrap();
        game.set_gravity(RegionId(2), [0, 0, 0]).unwrap();
        let moving = game
            .spawn_actor(
                Location {
                    region: RegionId(1),
                    position: Position { x: 3, y: 1, z: 0 },
                },
                NonZeroU64::new(100).unwrap(),
            )
            .unwrap();
        let attack = game.basic_attack(ActorId(1)).unwrap();
        let intention = game
            .admit_intention(
                ActorId(1),
                crate::Action::UseAbility {
                    id: attack,
                    target: ActorId(2),
                },
                crate::IntentionOrigin::Human,
            )
            .unwrap();
        game.execute_next_intention().unwrap().outcome.unwrap();
        for actor in [ActorId(2), moving] {
            game.actors.get_mut(&actor).unwrap().ready_at = 20;
        }
        game.set_actor_velocity(moving, [1, 0, 0]).unwrap();
        let rng = game.rules_rng;
        assert_eq!(game.advance_physics(10).unwrap(), 10);
        game.resolve_preparations().unwrap();
        assert!(!game.alive(ActorId(2)));
        assert!(game.preparation(ActorId(1)).is_none());
        assert_eq!(game.rules_rng, rng);
        assert!(game.combat_events().iter().any(|event| matches!(event,
            crate::combat::CombatEvent::Interrupted { actor: ActorId(1), intention: Some(id) } if *id == intention)));
        assert!(!game
            .combat_events()
            .iter()
            .any(|event| matches!(event, crate::combat::CombatEvent::Resolved { .. })));
    }

    #[test]
    fn health_condition_expiry_at_arena_deadline_has_elimination_precedence() {
        use crate::rules::{ConditionDef, Eviction, Modifier, ModifierOp, Scope, Stacking, Stat};
        let original = timer_build();
        let id = crate::content_ids::ConditionId::new("temporary_health").unwrap();
        let mut document = original.ruleset().document().clone();
        document.conditions.push(ConditionDef {
            id: id.clone(),
            family: crate::content_ids::ConditionFamilyId::new(id.as_str()).unwrap(),
            lifecycle: crate::rules::ConditionLifecycle::Timed,
            name: "Temporary health".into(),
            modifiers: vec![Modifier {
                scope: Scope::Always,
                op: ModifierOp::Add {
                    stat: Stat::Health,
                    value: 10,
                },
            }],
            stacking: Stacking::Longest,
            source_scoped: true,
            descriptors: vec![],
            capacity: Eviction::Oldest,
        });
        let content = crate::creatures::Content::new(
            crate::rules::Ruleset::validate(document).unwrap(),
            (*original.content().catalog).clone(),
        )
        .unwrap();
        let build = CreatureBuild::from_recorded(
            content.clone(),
            original.species_id().clone(),
            original.initial_attributes(),
            original.ledger().clone(),
            original.binding(),
            original.choices().to_vec(),
            vec![],
        )
        .unwrap();
        let mut game = Game::two_room_in_stone(42, content);
        for (x, faction) in [(1, "left"), (2, "right")] {
            let actor = game
                .spawn_actor(
                    Location {
                        region: RegionId(1),
                        position: Position { x, y: 1, z: 0 },
                    },
                    NonZeroU64::new(100).unwrap(),
                )
                .unwrap();
            game.configure_creature(
                actor,
                crate::CreatureIdentity {
                    name: faction.into(),
                    faction: faction.into(),
                },
                build.clone(),
            )
            .unwrap();
        }
        let baseline = game.health(ActorId(2)).unwrap().1;
        game.apply_condition(ActorId(2), &id, ActorId(1), 10)
            .unwrap();
        crate::test_creatures::injure(&mut game, ActorId(2), baseline + 5).unwrap();
        game.configure_arena_run_with_limits(
            ActorId(1),
            BTreeSet::from([ActorId(1), ActorId(2)]),
            Default::default(),
            crate::arena::ArenaLimits {
                ticks: 10,
                actions: 100,
            },
        )
        .unwrap();
        game.tick = 10;
        for actor in [ActorId(1), ActorId(2)] {
            game.actors.get_mut(&actor).unwrap().ready_at = 10;
        }
        game.check_arena_policy().unwrap();
        assert_eq!(
            game.arena_run().unwrap().stop(),
            Some(&crate::arena::ArenaStop::Elimination {
                winner: Some("left".into())
            })
        );
        assert!(!game.alive(ActorId(2)));
        assert!(game.arena_state_valid());
        let mut shared = crate::checkpoint::SharedState::default();
        let snapshot = game.checkpoint(&mut shared);
        let restored = Game::restore_checkpoint(snapshot, &shared, game.content().clone()).unwrap();
        assert_eq!(restored, game);
    }
}
