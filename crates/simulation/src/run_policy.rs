//! Run-specific policy. The action and damage kernels call these hooks without
//! branching on adventure or arena state.
use crate::{
    arena::ArenaRun,
    combat::{Objective, RunOutcome},
    ActorId, Game, GameError,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdventureRun {
    #[serde(deserialize_with = "Option::deserialize")]
    pub objective: Option<Objective>,
    pub outcome: RunOutcome,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "state",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum RunMode {
    Adventure(AdventureRun),
    Arena(ArenaRun),
}
impl Default for RunMode {
    fn default() -> Self {
        Self::Adventure(AdventureRun::default())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunError {
    NotArena,
    AlreadyConfigured,
    Finished,
    Paused,
    InvalidControl,
    InvalidParticipants,
    InvalidLimits,
    ActionLimit,
}

impl RunMode {
    pub fn outcome(&self) -> RunOutcome {
        match self {
            Self::Adventure(run) => run.outcome.clone(),
            Self::Arena(run) => RunOutcome {
                victor: None,
                deceased: run.deceased,
                terminal: run.stop().is_some(),
            },
        }
    }
    pub fn objective(&self) -> Option<&Objective> {
        match self {
            Self::Adventure(run) => run.objective.as_ref(),
            Self::Arena(_) => None,
        }
    }
    pub fn arena(&self) -> Option<&ArenaRun> {
        match self {
            Self::Adventure(_) => None,
            Self::Arena(run) => Some(run),
        }
    }
    pub fn arena_mut(&mut self) -> Option<&mut ArenaRun> {
        match self {
            Self::Adventure(_) => None,
            Self::Arena(run) => Some(run),
        }
    }
    pub fn execution_enabled(&self) -> bool {
        match self {
            Self::Adventure(run) => !run.outcome.terminal,
            Self::Arena(run) => {
                run.stop().is_none() && (!run.paused() || run.advance_remaining() > 0)
            }
        }
    }
    pub fn deadline(&self) -> Option<u64> {
        match self {
            Self::Adventure(_) => None,
            Self::Arena(run) => run.stop().is_none().then(|| run.deadline()).flatten(),
        }
    }
    pub fn on_death(&mut self, actor: ActorId, selected: Option<ActorId>) {
        if selected != Some(actor) {
            return;
        }
        match self {
            Self::Adventure(run) => {
                run.outcome.deceased = Some(actor);
                run.outcome.terminal = true;
            }
            Self::Arena(run) => {
                run.deceased = Some(actor);
            }
        }
    }
    pub fn on_action_committed(&mut self) -> Result<(), RunError> {
        match self {
            Self::Adventure(run) => {
                if run.outcome.terminal {
                    Err(RunError::Finished)
                } else {
                    Ok(())
                }
            }
            Self::Arena(run) => run.on_action_committed(),
        }
    }
}

impl Game {
    pub fn run_outcome(&self) -> RunOutcome {
        self.combat.run.outcome()
    }
    pub(crate) fn run_deadline(&self) -> Option<u64> {
        self.combat.run.deadline()
    }
    pub(crate) fn on_action_committed(&mut self) -> Result<(), GameError> {
        self.combat
            .run
            .on_action_committed()
            .map_err(GameError::Run)
    }
    pub(crate) fn on_actor_death(&mut self, actor: ActorId) -> Result<(), GameError> {
        self.combat.run.on_death(actor, self.combat.selected);
        self.check_run_policy()
    }
    pub(crate) fn run_policy_valid(&self) -> bool {
        let outcome = self.run_outcome();
        if !outcome
            .victor
            .is_none_or(|id| self.combat.characters.contains(&id))
            || !outcome
                .deceased
                .is_none_or(|id| self.combat.selected == Some(id) && !self.alive(id))
        {
            return false;
        }
        match &self.combat.run {
            RunMode::Adventure(run) => {
                run.objective.as_ref().is_none_or(|objective| {
                    self.world.knows(objective.anchor)
                        && objective
                            .item
                            .is_none_or(|id| id.0 > 0 && id.0 < self.next_item_id)
                }) && (run.outcome.deceased.is_none() || run.outcome.terminal)
            }
            RunMode::Arena(_) => self.arena_state_valid(),
        }
    }
    pub(crate) fn check_run_policy(&mut self) -> Result<(), GameError> {
        match &self.combat.run {
            RunMode::Adventure(run) => {
                if run.outcome.terminal || run.outcome.victor.is_some() {
                    return Ok(());
                }
                let Some(objective) = &run.objective else {
                    return Ok(());
                };
                let winner = self.combat.characters.iter().copied().find(|id| {
                    self.alive(*id)
                        && self.actors[id].location == objective.anchor
                        && objective.item.is_none_or(|item| {
                            self.items.get(&item).is_some_and(|item| {
                                item.location == crate::ItemLocation::Carried(*id)
                            })
                        })
                });
                if let Some(winner) = winner {
                    let terminal = !objective.continue_play;
                    if let RunMode::Adventure(run) = &mut self.combat.run {
                        run.outcome.victor = Some(winner);
                        run.outcome.terminal = terminal;
                    }
                }
                Ok(())
            }
            RunMode::Arena(_) => self.check_arena_policy(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arena::ArenaLimits;
    #[test]
    fn selected_death_is_terminal_only_for_adventure_policy() {
        let mut adventure = RunMode::default();
        let mut arena = RunMode::Arena(ArenaRun::new(ArenaLimits::default(), 0).unwrap());
        for policy in [&mut adventure, &mut arena] {
            policy.on_death(ActorId(1), Some(ActorId(1)));
        }
        assert!(adventure.outcome().terminal);
        assert!(!arena.outcome().terminal);
        assert_eq!(arena.outcome().deceased, Some(ActorId(1)));
    }
    #[test]
    fn paused_actions_require_a_step_budget_and_fail_without_mutation() {
        let mut run = ArenaRun::new(ArenaLimits::default(), 0).unwrap();
        run.control(true, 0).unwrap();
        let mut policy = RunMode::Arena(run);
        let before = policy.clone();
        assert_eq!(policy.on_action_committed(), Err(RunError::Paused));
        assert_eq!(policy, before);
        policy.arena_mut().unwrap().control(true, 1).unwrap();
        policy.on_action_committed().unwrap();
        assert_eq!(policy.arena().unwrap().actions, 1);
        assert!(!policy.execution_enabled());
    }
    #[test]
    fn tagged_policy_cannot_decode_a_mismatched_arena_or_adventure_shape() {
        let invalid = serde_json::json!({"kind":"adventure","state":ArenaRun::new(ArenaLimits::default(),0).unwrap()});
        assert!(serde_json::from_value::<RunMode>(invalid).is_err());
    }
    #[test]
    fn legacy_paired_policy_fields_are_rejected() {
        let old = serde_json::json!({
            "run_mode": "arena", "arena": ArenaRun::new(ArenaLimits::default(), 0).unwrap(),
            "objective": null, "outcome": RunOutcome::default()
        });
        assert!(serde_json::from_value::<RunMode>(old).is_err());
        let mut tagged = serde_json::to_value(RunMode::default()).unwrap();
        tagged["arena"] = serde_json::Value::Null;
        assert!(serde_json::from_value::<RunMode>(tagged).is_err());
    }
}
