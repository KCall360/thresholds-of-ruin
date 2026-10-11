//! Bounded encounter progress, shared by ordinary actor execution and physics.
use crate::Game;
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
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ArenaControl {
    Running,
    Paused { remaining: u64 },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ArenaProgress {
    Active { control: ArenaControl },
    Stopped { reason: ArenaStop },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArenaRun {
    pub limits: ArenaLimits,
    pub started_at: u64,
    pub actions: u64,
    pub progress: ArenaProgress,
    #[serde(deserialize_with = "Option::deserialize")]
    pub deceased: Option<crate::ActorId>,
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
            progress: ArenaProgress::Active {
                control: ArenaControl::Running,
            },
            deceased: None,
        })
    }

    pub fn stop(&self) -> Option<&ArenaStop> {
        match &self.progress {
            ArenaProgress::Active { .. } => None,
            ArenaProgress::Stopped { reason } => Some(reason),
        }
    }
    pub fn paused(&self) -> bool {
        matches!(
            self.progress,
            ArenaProgress::Active {
                control: ArenaControl::Paused { .. }
            }
        )
    }
    pub fn advance_remaining(&self) -> u64 {
        match self.progress {
            ArenaProgress::Active {
                control: ArenaControl::Paused { remaining },
            } => remaining,
            ArenaProgress::Active {
                control: ArenaControl::Running,
            }
            | ArenaProgress::Stopped { .. } => 0,
        }
    }
    pub(crate) fn control(
        &mut self,
        paused: bool,
        advance: u64,
    ) -> Result<(), crate::run_policy::RunError> {
        use crate::run_policy::RunError;
        if self.stop().is_some() {
            return Err(RunError::Finished);
        }
        if (!paused && advance != 0) || advance > self.limits.actions.saturating_sub(self.actions) {
            return Err(RunError::InvalidControl);
        }
        self.progress = ArenaProgress::Active {
            control: if paused {
                ArenaControl::Paused { remaining: advance }
            } else {
                ArenaControl::Running
            },
        };
        Ok(())
    }
    pub(crate) fn on_action_committed(&mut self) -> Result<(), crate::run_policy::RunError> {
        use crate::run_policy::RunError;
        let ArenaProgress::Active { control } = &mut self.progress else {
            return Err(RunError::Finished);
        };
        if self.actions >= self.limits.actions {
            return Err(RunError::ActionLimit);
        }
        if let ArenaControl::Paused { remaining } = control {
            *remaining = remaining.checked_sub(1).ok_or(RunError::Paused)?;
        }
        self.actions += 1;
        Ok(())
    }

    pub(crate) fn deadline(&self) -> Option<u64> {
        self.started_at.checked_add(self.limits.ticks)
    }
}

impl Game {
    /// Backend encounter evidence. Network clients require their own disclosure.
    pub fn arena_run(&self) -> Option<&ArenaRun> {
        self.combat.run.arena()
    }

    /// Pausing preserves the ordinary queue, preparations and global clock.
    pub fn arena_execution_enabled(&self) -> bool {
        self.combat.run.execution_enabled()
    }

    /// Pause and step are policy controls, never synthetic actor actions.
    pub fn control_arena(&mut self, paused: bool, advance: u64) -> Result<(), crate::GameError> {
        use crate::run_policy::RunError;
        let mut candidate = self.clone();
        let run = candidate
            .combat
            .run
            .arena_mut()
            .ok_or(crate::GameError::Run(RunError::NotArena))?;
        run.control(paused, advance)
            .map_err(crate::GameError::Run)?;
        if !paused || advance > 0 {
            candidate.advance_to_next_decision()?;
        }
        *self = candidate;
        Ok(())
    }

    fn arena_elimination(&self) -> Option<ArenaStop> {
        let mut teams = BTreeSet::new();
        let mut surviving = BTreeSet::new();
        for id in &self.combat.characters {
            // Missing/detached participants are not evidence of death. Loading
            // pins belong to the arena recipe/engine, not this result query.
            let actor = self.actors.get(id)?;
            let combat = actor.combat.as_ref()?;
            teams.insert(combat.identity.faction.as_str());
            if actor.alive() {
                surviving.insert(combat.identity.faction.as_str());
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
        let Some(run) = self.combat.run.arena() else {
            return false;
        };
        let Some(deadline) = run.deadline() else {
            return false;
        };
        if !run.limits.valid()
            || run.started_at > self.tick
            || self.tick > deadline
            || run.actions > run.limits.actions
            || run.advance_remaining() > run.limits.actions.saturating_sub(run.actions)
        {
            return false;
        }
        let elimination = self.arena_elimination();
        match run.stop() {
            None => {
                run.actions < run.limits.actions && self.tick < deadline && elimination.is_none()
            }
            Some(ArenaStop::ActionLimit) => {
                run.actions == run.limits.actions && self.tick <= deadline && elimination.is_none()
            }
            Some(ArenaStop::TickLimit) => {
                self.tick == deadline && run.actions < run.limits.actions && elimination.is_none()
            }
            Some(reason @ ArenaStop::Elimination { .. }) => elimination.as_ref() == Some(reason),
        }
    }

    pub(crate) fn check_arena_policy(&mut self) -> Result<(), crate::GameError> {
        let Some(run) = self.combat.run.arena().filter(|run| run.stop().is_none()) else {
            return Ok(());
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
            return Ok(());
        };
        let run = self.combat.run.arena_mut().expect("active arena");
        run.progress = ArenaProgress::Stopped { reason };
        self.settle_creature_time()?;
        // Settling expiring maximum-health modifiers can kill a participant.
        // Elimination has precedence even when discovered at the final deadline.
        if let Some(reason) = self.arena_elimination() {
            self.combat.run.arena_mut().expect("stopped arena").progress =
                ArenaProgress::Stopped { reason };
        }
        let actors: Vec<_> = self.actors.keys().copied().collect();
        for actor in actors {
            let mut outcome = self.actor_outcome(actor)?;
            outcome.interrupt(crate::lifecycle::Interruption::Discard, self.tick);
            self.apply_outcome(outcome)?;
        }
        let queued: Vec<_> = self
            .queued_intentions()
            .map(|entry| (entry.actor, entry.id))
            .collect();
        for (actor, id) in queued {
            self.cancel_intention(actor, id)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coincident_caps_use_action_precedence_and_preserve_valid_state() {
        use crate::{test_creatures, ActorId};
        use std::{collections::BTreeMap, num::NonZeroU64};
        use tor_world::{Location, Position, RegionId};
        let mut game = Game::two_room_in_stone(42, test_creatures::content());
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
            test_creatures::configure(&mut game, actor, faction, test_creatures::species());
        }
        game.configure_arena_run_with_limits(
            ActorId(1),
            BTreeSet::from([ActorId(1), ActorId(2)]),
            BTreeMap::new(),
            ArenaLimits {
                ticks: 7,
                actions: 1,
            },
        )
        .unwrap();
        game.tick = 7;
        game.combat.run.arena_mut().unwrap().actions = 1;
        game.check_arena_policy().unwrap();
        assert_eq!(
            game.arena_run().unwrap().stop(),
            Some(&ArenaStop::ActionLimit)
        );
        assert!(game.arena_state_valid());
        assert!(!game.arena_execution_enabled());
    }

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
    fn tagged_progress_and_nullable_winner_fields_are_required() {
        let run = ArenaRun::new(ArenaLimits::default(), 0).unwrap();
        let mut value = serde_json::to_value(run).unwrap();
        value.as_object_mut().unwrap().remove("progress");
        assert!(serde_json::from_value::<ArenaRun>(value).is_err());
        assert!(
            serde_json::from_value::<ArenaStop>(serde_json::json!({"type":"elimination"})).is_err()
        );
    }
}
