//! Relative creature timers follow active simulation ticks, never wall time or
//! the elapsed global time while an actor is frozen or detached.
use crate::{ActorId, Game, GameError};

#[cfg(test)]
thread_local! {
    static TIMER_SETTLEMENTS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

impl Game {
    /// Earliest scheduled creature change on the global simulation clock.
    /// Frozen and detached creatures cannot wake the active simulation.
    pub fn next_creature_event_tick(&self) -> Option<u64> {
        self.actors
            .scheduled_timers()
            .filter(|&(_, id)| !self.actor_frozen(id))
            .find_map(|(interval, _)| {
                // An event beyond the clock's representable range cannot fire.
                // Never saturate it to the current tick and spin at exhaustion.
                self.creature_time_at.checked_add(interval)
            })
    }

    /// Materialize elapsed active time before state changes or observations.
    /// Freeze/thaw settles first, so one interval never mixes clock states.
    pub(crate) fn settle_creature_time(&mut self) {
        let elapsed = self.tick - self.creature_time_at;
        self.creature_time_at = self.tick;
        self.advance_creature_time(elapsed);
    }

    pub(crate) fn advance_creature_time(&mut self, ticks: u64) {
        if ticks == 0 {
            return;
        }
        #[cfg(test)]
        TIMER_SETTLEMENTS.with(|count| count.set(count.get() + 1));
        let ids: Vec<_> = self
            .actors
            .timed()
            .filter(|&id| !self.actor_frozen(id))
            .collect();
        for id in ids {
            self.actors
                .get_mut(&id)
                .expect("indexed timed actor")
                .combat
                .as_mut()
                .expect("timed creature combat")
                .advance_active(ticks);
        }
    }

    /// Trusted effect application. Ability execution separately validates its
    /// checks, costs, target reach and visibility before applying the condition.
    /// A causer may since have died or detached; continuing fear needs no sight.
    pub fn apply_fear_condition(
        &mut self,
        actor: ActorId,
        source: ActorId,
        duration: u64,
    ) -> Result<crate::fear::FearUpdate, GameError> {
        self.settle_creature_time();
        if !self.actors.contains_key(&source) && !self.detached_actor(source) {
            return Err(GameError::UnknownActor);
        }
        let mut actor = self
            .actors
            .get_mut(&actor)
            .filter(|actor| actor.alive())
            .ok_or(GameError::UnknownActor)?;
        actor
            .combat
            .as_mut()
            .ok_or(GameError::InvalidLocation)?
            .apply_fear(source, duration)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::attributes::{Attributes, ManaBinding, Skill};
    use crate::creatures::{CreatureBuild, Species};
    use crate::progression::{Class, CreatureType, HdLedger, HdSource};
    use std::{collections::BTreeSet, num::NonZeroU64};
    use tor_world::{Location, Position, RegionId};

    #[test]
    fn moving_physics_batches_timers_until_the_next_boundary() {
        let mut game = Game::two_room_in_stone(42);
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
        let build = CreatureBuild::new(
            Species {
                id: "timer_subject".into(),
                kind: CreatureType::Humanoid,
                subtypes: BTreeSet::new(),
                default_attributes: Attributes::default(),
                anatomy: crate::AnatomySpec::humanoid(),
                melee: crate::attacks::MeleeAttack::new(Skill::HeavyWeaponry, 0, 60, 40, {
                    let component = crate::damage::DamageComponent::rolled(
                        crate::combat::DamageType::Impact,
                        None,
                        crate::dice::DicePool::new(1, 4, 0).unwrap(),
                    );
                    let primary = component.key();
                    crate::damage::DamageSpec::new(vec![component], Some(primary)).unwrap()
                })
                .unwrap(),
                grants: vec![],
            },
            HdLedger::seeded(vec![HdSource::Class(Class::Mage)], 7).unwrap(),
            ManaBinding::Intellect,
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
        game.apply_fear_condition(subject, source, 500).unwrap();
        game.set_actor_velocity(subject, [512, 0, 0]).unwrap();
        game.actors.get_mut(&subject).unwrap().ready_at = 150;
        let before = TIMER_SETTLEMENTS.with(|count| count.get());
        game.act(source, crate::Action::Wait).unwrap();
        assert_eq!(game.tick(), 99);
        assert_eq!(
            game.creature(subject).unwrap().fear().remaining(source),
            Some(401)
        );
        assert_eq!(
            TIMER_SETTLEMENTS.with(|count| count.get()) - before,
            1,
            "physics ticks updated timers without a scheduled change or boundary"
        );
    }
}
