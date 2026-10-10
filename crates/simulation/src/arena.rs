//! Bounded encounter progress, shared by ordinary actor execution and physics.
use crate::{combat::CombatEvent, Game};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArenaLimits {
    pub ticks: u64,
    pub actions: u64,
}

impl Default for ArenaLimits {
    fn default() -> Self {
        Self {
            ticks: 100_000,
            actions: 10_000,
        }
    }
}

impl ArenaLimits {
    pub fn valid(self) -> bool {
        (1..=100_000).contains(&self.ticks) && (1..=10_000).contains(&self.actions)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ArenaStop {
    ActionLimit,
    TickLimit,
    Elimination {
        #[serde(deserialize_with = "Option::deserialize")]
        winner: Option<String>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArenaRun {
    pub limits: ArenaLimits,
    pub started_at: u64,
    pub actions: u64,
    pub paused: bool,
    pub advance_remaining: u64,
    #[serde(deserialize_with = "Option::deserialize")]
    pub stop: Option<ArenaStop>,
}

impl ArenaRun {
    pub(crate) fn new(limits: ArenaLimits, tick: u64) -> Option<Self> {
        if !limits.valid() || tick.checked_add(limits.ticks).is_none() {
            return None;
        }
        Some(Self {
            limits,
            started_at: tick,
            actions: 0,
            paused: false,
            advance_remaining: 0,
            stop: None,
        })
    }

    fn deadline(&self) -> Option<u64> {
        self.started_at.checked_add(self.limits.ticks)
    }
}

impl Game {
    /// Backend encounter evidence. Network clients require their own disclosure.
    pub fn arena_run(&self) -> Option<&ArenaRun> {
        self.combat.arena.as_ref()
    }

    /// Pausing preserves the ordinary queue, preparations and global clock.
    pub fn arena_execution_enabled(&self) -> bool {
        self.combat
            .arena
            .as_ref()
            .is_none_or(|run| !run.paused || run.advance_remaining > 0)
    }

    /// Trusted control changes are journaled by the server's wizard boundary.
    /// Step permits committed actions; admissions and failed actions cost none.
    pub fn control_arena(&mut self, paused: bool, advance: u64) -> Result<(), crate::GameError> {
        let run = self
            .combat
            .arena
            .as_mut()
            .ok_or(crate::GameError::InvalidLocation)?;
        if run.stop.is_some()
            || (!paused && advance != 0)
            || advance > run.limits.actions - run.actions
        {
            return Err(crate::GameError::InvalidLocation);
        }
        run.paused = paused;
        run.advance_remaining = advance;
        // Exhausting a step budget deliberately stops before the next wake.
        // When execution is granted again, resume ordinary timers before a
        // future actor can be selected; granting permission is not an action.
        if !paused || advance > 0 {
            self.advance_to_next_decision();
        }
        Ok(())
    }

    pub(crate) fn arena_deadline(&self) -> Option<u64> {
        self.combat
            .arena
            .as_ref()
            .filter(|run| run.stop.is_none())?
            .deadline()
    }

    pub(crate) fn count_arena_action(&mut self) {
        if let Some(run) = self.combat.arena.as_mut().filter(|run| run.stop.is_none()) {
            run.actions += 1;
            if run.paused {
                run.advance_remaining = run
                    .advance_remaining
                    .checked_sub(1)
                    .expect("committed arena action requires execution permission");
            }
        }
    }

    fn arena_elimination(&self) -> Option<ArenaStop> {
        let mut teams = BTreeSet::new();
        let mut surviving = BTreeSet::new();
        for id in &self.combat.characters {
            // Missing/detached participants are not evidence of death. Loading
            // pins belong to the arena recipe/engine, not this result query.
            let actor = self.actors.get(id)?;
            let combat = actor.combat.as_ref()?;
            teams.insert(combat.spec.faction.as_str());
            if actor.alive() {
                surviving.insert(combat.spec.faction.as_str());
            }
        }
        if surviving.is_empty() {
            Some(ArenaStop::Elimination { winner: None })
        } else if teams.len() > 1 && surviving.len() == 1 {
            Some(ArenaStop::Elimination {
                winner: surviving.first().map(|team| (*team).to_owned()),
            })
        } else {
            None
        }
    }

    pub(crate) fn arena_state_valid(&self) -> bool {
        let Some(run) = self.combat.arena.as_ref() else {
            return false;
        };
        let Some(deadline) = run.deadline() else {
            return false;
        };
        if !run.limits.valid()
            || run.started_at > self.tick
            || self.tick > deadline
            || run.actions > run.limits.actions
            || run.advance_remaining > run.limits.actions.saturating_sub(run.actions)
            || (!run.paused && run.advance_remaining != 0)
            || (run.stop.is_some() && run.advance_remaining != 0)
        {
            return false;
        }
        let elimination = self.arena_elimination();
        match &run.stop {
            None => {
                !self.combat.outcome.terminal
                    && run.actions < run.limits.actions
                    && self.tick < deadline
                    && elimination.is_none()
            }
            Some(ArenaStop::ActionLimit) => {
                self.combat.outcome.terminal
                    && run.actions == run.limits.actions
                    && self.tick < deadline
                    && elimination.is_none()
            }
            Some(ArenaStop::TickLimit) => {
                self.combat.outcome.terminal
                    && self.tick == deadline
                    && run.actions < run.limits.actions
                    && elimination.is_none()
            }
            Some(reason @ ArenaStop::Elimination { .. }) => {
                self.combat.outcome.terminal && elimination.as_ref() == Some(reason)
            }
        }
    }

    pub(crate) fn check_arena_end(&mut self) {
        let Some(run) = self.combat.arena.as_ref().filter(|run| run.stop.is_none()) else {
            return;
        };
        let reason = self.arena_elimination().or_else(|| {
            if run.actions >= run.limits.actions {
                Some(ArenaStop::ActionLimit)
            } else if run.deadline().is_some_and(|deadline| self.tick >= deadline) {
                Some(ArenaStop::TickLimit)
            } else {
                None
            }
        });
        let Some(reason) = reason else {
            return;
        };
        self.settle_creature_time();
        let run = self.combat.arena.as_mut().expect("active arena");
        run.stop = Some(reason);
        run.advance_remaining = 0;
        self.combat.outcome.terminal = true;
        let actors: Vec<_> = self.actors.keys().copied().collect();
        for actor in actors {
            let preparation = self
                .actors
                .get_mut(&actor)
                .expect("known actor")
                .cancel_preparation();
            if let Some(preparation) = preparation {
                self.combat.events.push(CombatEvent::Interrupted {
                    actor,
                    intention: preparation.intention,
                });
            }
        }
        let queued: Vec<_> = self
            .queued_intentions()
            .map(|entry| (entry.actor, entry.id))
            .collect();
        for (actor, id) in queued {
            self.cancel_intention(actor, id)
                .expect("queued work without preparation");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limits_and_absolute_deadlines_are_bounded() {
        assert!(ArenaRun::new(ArenaLimits::default(), 0).is_some());
        for limits in [
            ArenaLimits {
                ticks: 0,
                actions: 1,
            },
            ArenaLimits {
                ticks: 1,
                actions: 0,
            },
            ArenaLimits {
                ticks: 100_001,
                actions: 1,
            },
            ArenaLimits {
                ticks: 1,
                actions: 10_001,
            },
        ] {
            assert!(ArenaRun::new(limits, 0).is_none());
        }
        assert!(ArenaRun::new(ArenaLimits::default(), u64::MAX).is_none());
    }

    #[test]
    fn nullable_stop_and_winner_fields_are_required() {
        let run = ArenaRun::new(ArenaLimits::default(), 0).unwrap();
        let mut value = serde_json::to_value(run).unwrap();
        value.as_object_mut().unwrap().remove("stop");
        assert!(serde_json::from_value::<ArenaRun>(value).is_err());
        assert!(
            serde_json::from_value::<ArenaStop>(serde_json::json!({"type":"elimination"})).is_err()
        );
    }
}
