//! Runtime-only input authority for attached human actors. Never restored by rewind.
use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
struct InputAuthorityBasis {
    operands: Option<tor_simulation::InputBasis>,
    readiness: Readiness,
    intentions: Vec<IntentionStatus>,
}

#[derive(Clone, Debug)]
struct RegisteredInput {
    // None is permanently exhausted for this registration; never wrap or reuse.
    generation: Option<u64>,
    basis: InputAuthorityBasis,
}

#[derive(Debug, Default)]
pub(super) struct InputGenerations(BTreeMap<ActorId, RegisteredInput>);

pub(super) struct PreparedInputGenerations(Vec<(ActorId, RegisteredInput)>);

impl InputGenerations {
    pub(super) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl Engine {
    /// Idempotent across viewers of the same actor. Service owns last-client removal.
    pub(crate) fn register_input_observer(&mut self, actor: ActorId) -> Result<u64, Failure> {
        self.causal_boundary(actor)?;
        if !self.input_generations.0.contains_key(&actor) {
            let basis = self.input_authority_basis(&self.game, self.branch(), actor, None);
            self.input_generations.0.insert(
                actor,
                RegisteredInput {
                    generation: Some(0),
                    basis,
                },
            );
        }
        self.input_generation(actor)
    }

    pub(crate) fn unregister_input_observer(&mut self, actor: ActorId) {
        self.input_generations.0.remove(&actor);
    }

    /// Private runtime generation, not a public state revision or saved fingerprint.
    pub(crate) fn input_generation(&self, actor: ActorId) -> Result<u64, Failure> {
        self.input_generations
            .0
            .get(&actor)
            .and_then(|entry| entry.generation)
            .ok_or_else(|| Failure::new(ErrorCode::Unauthorized, "Input authority is unavailable"))
    }

    fn input_authority_basis(
        &self,
        game: &Game,
        branch: &BranchId,
        actor: ActorId,
        incoming: Option<&JournalEntry>,
    ) -> InputAuthorityBasis {
        InputAuthorityBasis {
            operands: game.input_basis(SimActor(actor.0)),
            readiness: self.input_readiness_at(game, branch, actor, incoming),
            intentions: self.pending_intentions_at(game, branch, actor, incoming),
        }
    }

    /// Stage from the final candidate before persistence; no registry mutation.
    pub(super) fn prepare_input_generations(
        &self,
        candidate: &Candidate,
        incoming: &JournalEntry,
    ) -> PreparedInputGenerations {
        let mut updates = Vec::new();
        for (&actor, registered) in &self.input_generations.0 {
            let basis = self.input_authority_basis(
                &candidate.game,
                candidate.branch(),
                actor,
                Some(incoming),
            );
            if candidate.branch() != self.branch() || basis != registered.basis {
                updates.push((
                    actor,
                    RegisteredInput {
                        generation: registered.generation.and_then(|value| value.checked_add(1)),
                        basis,
                    },
                ));
            }
        }
        PreparedInputGenerations(updates)
    }

    /// Infallible, after successful persistence admission and journal append.
    pub(super) fn publish_input_generations(&mut self, updates: PreparedInputGenerations) {
        for (actor, entry) in updates.0 {
            self.input_generations.0.insert(actor, entry);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Engine {
        let mut engine = Engine::memory(Scenario::two_room(42)).unwrap();
        engine.enable_wizard().unwrap();
        engine
    }

    fn teleport(engine: &mut Engine, request: &str, position: Position) {
        let actor = ActorId(1);
        engine
            .command(
                "test",
                "test",
                actor,
                request,
                &engine.branch().clone(),
                Command::Wizard {
                    display_basis: None,
                    expected_boundary: engine.causal_boundary(actor).unwrap(),
                    operation: WizardOperation::Teleport { actor, position },
                },
            )
            .unwrap();
    }

    #[test]
    fn input_generations_track_committed_operand_aba_not_refresh_endpoints() {
        let mut engine = fixture();
        let actor = ActorId(1);
        assert_eq!(engine.register_input_observer(actor).unwrap(), 0);
        let original = engine.input_generations.0[&actor].basis.clone();
        teleport(
            &mut engine,
            "away",
            Position {
                region: 2,
                x: 1,
                y: 1,
                z: 0,
            },
        );
        assert_ne!(engine.input_generations.0[&actor].basis, original);
        assert_eq!(engine.input_generation(actor).unwrap(), 1);
        teleport(
            &mut engine,
            "back",
            Position {
                region: 1,
                x: 1,
                y: 1,
                z: 0,
            },
        );
        assert_eq!(engine.input_generations.0[&actor].basis, original);
        assert_eq!(engine.input_generation(actor).unwrap(), 2);
        assert_eq!(
            engine.register_input_observer(actor).unwrap(),
            2,
            "a second viewer cannot reset authority"
        );
        assert!(engine.seed_profile_history(1).is_err());
        assert!(engine.diagnostic_copy().input_generations.is_empty());
        assert!(Engine::replay(engine.archive.clone(), None, None)
            .unwrap()
            .input_generations
            .is_empty());
        engine.unregister_input_observer(actor);
        assert!(engine.input_generation(actor).is_err());
        assert!(engine.input_generations.is_empty());
    }

    #[test]
    fn input_generations_track_public_intention_phase_aba_per_commit() {
        let mut engine = fixture();
        let actor = ActorId(1);
        engine.register_input_observer(actor).unwrap();
        let admitted = engine
            .command(
                "test",
                "test",
                actor,
                "admit",
                &engine.branch().clone(),
                Command::AdmitIntention {
                    expected_boundary: engine.causal_boundary(actor).unwrap(),
                    action: Action::Wait,
                },
            )
            .unwrap();
        assert_eq!(engine.input_generation(actor).unwrap(), 1);
        let queued = engine.input_generations.0[&actor].basis.clone();
        assert_eq!(queued.intentions, engine.pending_intentions(actor));
        assert_eq!(queued.intentions[0].phase, IntentionPhase::Queued);
        engine.suspend_queued_intention(actor).unwrap().unwrap();
        assert_eq!(engine.input_generation(actor).unwrap(), 2);
        assert_eq!(
            engine.input_generations.0[&actor].basis.intentions[0].phase,
            IntentionPhase::Suspended
        );
        engine
            .command(
                "test",
                "test",
                actor,
                "resume",
                &engine.branch().clone(),
                Command::ResumeIntention {
                    expected_boundary: engine.causal_boundary(actor).unwrap(),
                    admission: admitted.entry.id,
                },
            )
            .unwrap();
        assert_eq!(engine.input_generations.0[&actor].basis, queued);
        assert_eq!(
            engine.input_generation(actor).unwrap(),
            3,
            "queued->suspended->queued remains stale even without a Session refresh"
        );
    }

    #[test]
    fn input_generations_ignore_unregistered_remote_changes_and_abandoned_candidates() {
        let mut scenario = Scenario::two_room(42);
        scenario.actors.push(ActorSetup {
            position: Position {
                region: 2,
                x: 1,
                y: 1,
                z: 0,
            },
            turn_ticks: 100,
        });
        let mut engine = Engine::memory(scenario).unwrap();
        engine.enable_wizard().unwrap();
        let actor = ActorId(1);
        engine.register_input_observer(actor).unwrap();
        engine
            .command(
                "test",
                "test",
                actor,
                "remote",
                &engine.branch().clone(),
                Command::Wizard {
                    display_basis: None,
                    expected_boundary: engine.causal_boundary(actor).unwrap(),
                    operation: WizardOperation::Teleport {
                        actor: ActorId(2),
                        position: Position {
                            region: 2,
                            x: 2,
                            y: 1,
                            z: 0,
                        },
                    },
                },
            )
            .unwrap();
        assert_eq!(engine.input_generations.0.len(), 1);
        assert_eq!(engine.input_generation(actor).unwrap(), 0);
        let mut candidate = Candidate::capture(&engine);
        candidate
            .game
            .act(
                SimActor(1),
                tor_simulation::Action::Move(tor_world::Direction::East),
            )
            .unwrap();
        let entry = &engine.archive.records.last().unwrap().entry;
        let staged = engine.prepare_input_generations(&candidate, entry);
        assert_eq!(staged.0.len(), 1, "positive changed candidate");
        assert_eq!(staged.0[0].1.generation, Some(1));
        drop(staged);
        assert_eq!(
            engine.input_generation(actor).unwrap(),
            0,
            "staging and rejecting a candidate cannot consume authority"
        );
        assert_eq!(
            engine.input_generations.0[&actor].basis,
            engine.input_authority_basis(&engine.game, engine.branch(), actor, None)
        );
    }

    #[test]
    fn input_generations_branch_reset_and_exhaustion_never_reuse_authority() {
        let mut engine = fixture();
        let actor = ActorId(1);
        engine.register_input_observer(actor).unwrap();
        let before = engine.input_generations.0[&actor].basis.clone();
        engine
            .command(
                "test",
                "test",
                actor,
                "rewind",
                &engine.branch().clone(),
                Command::Wizard {
                    display_basis: None,
                    expected_boundary: engine.causal_boundary(actor).unwrap(),
                    operation: WizardOperation::Rewind { target: None },
                },
            )
            .unwrap();
        assert_eq!(engine.input_generations.0[&actor].basis, before);
        assert_eq!(engine.input_generation(actor).unwrap(), 1);
        engine
            .input_generations
            .0
            .get_mut(&actor)
            .unwrap()
            .generation = Some(u64::MAX);
        teleport(
            &mut engine,
            "exhaust",
            Position {
                region: 2,
                x: 1,
                y: 1,
                z: 0,
            },
        );
        assert!(engine.input_generation(actor).is_err());
        teleport(
            &mut engine,
            "still-exhausted",
            Position {
                region: 1,
                x: 1,
                y: 1,
                z: 0,
            },
        );
        assert!(
            engine.input_generation(actor).is_err(),
            "exhaustion must not block commits or wrap"
        );
    }
}
