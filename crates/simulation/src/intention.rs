//! Simulation-owned admitted work. Admission is distinct from execution, and
//! execution rebuilds transient preparation against the current authoritative state.
use crate::{Action, ActionOutcome, ActorId, Game, GameError};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_QUEUED_INTENTIONS: usize = 4096;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct IntentionId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IntentionOrigin {
    Human,
    Autonomous,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IntentionState {
    Queued,
    Suspended,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "action",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum IntentionWork {
    Action(Action),
    AiDecision,
}

/// Region-local movement meaning at admission, including portal frame changes.
/// This is a guard, not a prepared action or cached permission to move.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MovementContext {
    from: tor_world::Location,
    orientation: u8,
    to: tor_world::Location,
    next_orientation: u8,
}

impl MovementContext {
    pub(crate) fn matches(
        &self,
        from: tor_world::Location,
        orientation: u8,
        to: tor_world::Location,
        next_orientation: u8,
    ) -> bool {
        (self.from, self.orientation, self.to, self.next_orientation)
            == (from, orientation, to, next_orientation)
    }
}

fn deserialize_movement_context<'de, D>(
    deserializer: D,
) -> Result<Option<MovementContext>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Option::<MovementContext>::deserialize(deserializer)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QueuedIntention {
    pub id: IntentionId,
    pub actor: ActorId,
    pub work: IntentionWork,
    pub origin: IntentionOrigin,
    pub state: IntentionState,
    #[serde(deserialize_with = "deserialize_movement_context")]
    pub movement_context: Option<MovementContext>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IntentionExecution {
    pub intention: QueuedIntention,
    /// A concrete action is selected only at execution for deferred AI work.
    pub action: Option<Action>,
    pub outcome: Result<ActionOutcome, GameError>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct IntentionQueue {
    next_id: u64,
    entries: BTreeMap<ActorId, QueuedIntention>,
}

impl Default for IntentionQueue {
    fn default() -> Self {
        Self {
            next_id: 1,
            entries: BTreeMap::new(),
        }
    }
}

impl Game {
    /// Validate currently available work without moving time or applying effects.
    /// A later execution can still fail if the actor, target or topology changes.
    pub fn admit_intention(
        &mut self,
        actor: ActorId,
        action: Action,
        origin: IntentionOrigin,
    ) -> Result<IntentionId, GameError> {
        let next = self.check_intention_slot(actor)?;
        self.validate_intention_action(actor, action)?;
        Ok(self.insert_intention(actor, IntentionWork::Action(action), origin, next))
    }

    /// Queue the decision itself; no observation, search or RNG work runs here.
    pub fn admit_ai_intention(&mut self, actor: ActorId) -> Result<IntentionId, GameError> {
        let next = self.check_intention_slot(actor)?;
        if self.combat.outcome.terminal
            || !self.is_ai(actor)
            || self.health(actor).is_none_or(|(hp, _)| hp == 0)
        {
            return Err(GameError::UnknownActor);
        }
        Ok(self.insert_intention(
            actor,
            IntentionWork::AiDecision,
            IntentionOrigin::Autonomous,
            next,
        ))
    }

    fn check_intention_slot(&self, actor: ActorId) -> Result<u64, GameError> {
        if self.intentions.entries.contains_key(&actor) {
            return Err(GameError::ActorBusy);
        }
        if self.intentions.entries.len() >= MAX_QUEUED_INTENTIONS {
            return Err(GameError::QueueFull);
        }
        self.intentions
            .next_id
            .checked_add(1)
            .ok_or(GameError::IdentityExhausted)
    }

    fn insert_intention(
        &mut self,
        actor: ActorId,
        work: IntentionWork,
        origin: IntentionOrigin,
        next: u64,
    ) -> IntentionId {
        let id = IntentionId(self.intentions.next_id);
        let movement_context = match work {
            IntentionWork::Action(Action::Move(direction)) => {
                self.movement_context(actor, direction)
            }
            _ => None,
        };
        self.intentions.entries.insert(
            actor,
            QueuedIntention {
                id,
                actor,
                work,
                origin,
                state: IntentionState::Queued,
                movement_context,
            },
        );
        self.intentions.next_id = next;
        id
    }

    pub fn pending_intention(&self, actor: ActorId) -> Option<&QueuedIntention> {
        self.intentions.entries.get(&actor)
    }

    fn movement_context(
        &self,
        actor: ActorId,
        direction: tor_world::Direction,
    ) -> Option<MovementContext> {
        let state = self.actors.get(&actor)?;
        let (to, next_orientation) = self.actor_translation(actor, direction)?;
        Some(MovementContext {
            from: state.location,
            orientation: state.orientation,
            to,
            next_orientation,
        })
    }

    /// Backend recovery inspects only the bounded admitted work, including
    /// unavailable actors whose work still requires a terminal scheduler result.
    pub fn queued_intentions(&self) -> impl Iterator<Item = &QueuedIntention> {
        self.intentions.entries.values()
    }

    fn intention_actor_unavailable(&self, actor: ActorId) -> bool {
        self.known_actor_region(actor).is_none()
            || self.actors.get(&actor).is_some_and(|actor| !actor.alive())
    }

    fn selected_intention(&self) -> Option<&QueuedIntention> {
        // Unavailable actors cannot become due. Resolve their work in stable
        // admission order, while preserving unloaded actors in the identity directory.
        let unavailable = self
            .intentions
            .entries
            .values()
            .filter(|entry| self.intention_actor_unavailable(entry.actor))
            .min_by_key(|entry| entry.id);
        unavailable.or_else(|| {
            self.intentions
                .entries
                .get(&self.next_actor()?)
                .filter(|entry| entry.state == IntentionState::Queued)
        })
    }

    /// Host scheduling uses the same selection as execution, including terminal
    /// work whose actor can never become due. Selection has no simulation effects.
    pub fn next_intention_actor(&self) -> Option<ActorId> {
        self.selected_intention().map(|entry| entry.actor)
    }

    /// Preserve the actor scheduler's ordering, including waiting human boundaries.
    /// A failed execution consumes the intention but never substitutes another target.
    pub fn execute_next_intention(&mut self) -> Option<IntentionExecution> {
        let intention = self.selected_intention()?.clone();
        let actor = intention.actor;
        if self.intention_actor_unavailable(actor) {
            self.intentions.entries.remove(&intention.actor);
            let action = match intention.work {
                IntentionWork::Action(action) => Some(action),
                IntentionWork::AiDecision => None,
            };
            return Some(IntentionExecution {
                intention,
                action,
                outcome: Err(GameError::UnknownActor),
            });
        }
        let (action, outcome) = match intention.work {
            IntentionWork::Action(action) => (
                Some(action),
                if matches!(action, Action::Move(_)) && intention.movement_context.is_none() {
                    Err(GameError::InvalidLocation)
                } else {
                    self.act_with_context(
                        actor,
                        action,
                        Some(intention.id),
                        intention.movement_context,
                    )
                },
            ),
            IntentionWork::AiDecision => {
                match self.act_ai_with_intention(actor, Some(intention.id)) {
                    Ok((action, outcome)) => (Some(action), Ok(outcome)),
                    Err(error) => (None, Err(error)),
                }
            }
        };
        self.intentions.entries.remove(&actor);
        Some(IntentionExecution {
            intention,
            action,
            outcome,
        })
    }

    pub fn cancel_intention(
        &mut self,
        actor: ActorId,
        id: IntentionId,
    ) -> Result<QueuedIntention, GameError> {
        if self
            .pending_intention(actor)
            .is_none_or(|entry| entry.id != id)
        {
            return Err(GameError::InvalidIntention);
        }
        Ok(self
            .intentions
            .entries
            .remove(&actor)
            .expect("checked intention"))
    }

    pub fn suspend_human_intentions(&mut self) {
        for intention in self.intentions.entries.values_mut() {
            if intention.origin == IntentionOrigin::Human {
                intention.state = IntentionState::Suspended;
            }
        }
    }

    pub fn suspend_intention(&mut self, actor: ActorId, id: IntentionId) -> Result<(), GameError> {
        if self.pending_intention(actor).is_none_or(|entry| {
            entry.id != id
                || entry.origin != IntentionOrigin::Human
                || entry.state != IntentionState::Queued
        }) {
            return Err(GameError::InvalidIntention);
        }
        self.intentions
            .entries
            .get_mut(&actor)
            .expect("checked intention")
            .state = IntentionState::Suspended;
        Ok(())
    }

    pub fn resume_intention(&mut self, actor: ActorId, id: IntentionId) -> Result<(), GameError> {
        if self.pending_intention(actor).is_none_or(|entry| {
            entry.id != id
                || entry.origin != IntentionOrigin::Human
                || entry.state != IntentionState::Suspended
        }) {
            return Err(GameError::InvalidIntention);
        }
        self.intentions
            .entries
            .get_mut(&actor)
            .expect("checked intention")
            .state = IntentionState::Queued;
        Ok(())
    }

    pub fn continue_intention_ids(&mut self, later: &Game) {
        self.intentions.next_id = self.intentions.next_id.max(later.intentions.next_id);
    }

    pub(crate) fn intentions_valid(&self) -> bool {
        let mut ids = BTreeSet::new();
        self.intentions.next_id != 0
            && self.intentions.entries.len() <= MAX_QUEUED_INTENTIONS
            && self.intentions.entries.iter().all(|(actor, entry)| {
                actor.0 != 0
                    && *actor == entry.actor
                    && entry.id.0 != 0
                    && entry.id.0 < self.intentions.next_id
                    && ids.insert(entry.id)
                    && match (entry.work, entry.movement_context) {
                        (IntentionWork::Action(Action::Move(_)), Some(context)) => {
                            context.from.region.0 != 0
                                && context.to.region.0 != 0
                                && context.orientation < 24
                                && context.next_orientation < 24
                        }
                        (IntentionWork::Action(Action::Move(_)), None) => false,
                        (_, context) => context.is_none(),
                    }
                    && match entry.work {
                        IntentionWork::Action(Action::Attack { target }) => {
                            target.0 != 0 && target != *actor
                        }
                        IntentionWork::Action(Action::SetDoor { door, .. }) => door != 0,
                        IntentionWork::Action(
                            Action::Take { item, quantity } | Action::Drop { item, quantity },
                        ) => item.0 != 0 && quantity.is_none_or(|quantity| quantity != 0),
                        IntentionWork::Action(Action::Move(_) | Action::Wait) => true,
                        IntentionWork::AiDecision => entry.origin == IntentionOrigin::Autonomous,
                    }
            })
            && self
                .actors
                .values()
                .filter_map(|actor| actor.combat.as_ref()?.pending.as_ref()?.intention)
                .all(|id| id.0 != 0 && id.0 < self.intentions.next_id && ids.insert(id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Action, ActorId, Game, GameError};
    use std::num::NonZeroU64;
    use tor_world::{Direction, Location, Position, RegionId};

    fn fixture() -> (Game, ActorId) {
        let mut game = Game::new((*Game::two_room(42).world).clone(), 42);
        let actor = game
            .spawn_actor(
                Location {
                    region: RegionId(1),
                    position: Position { x: 1, y: 1, z: 0 },
                },
                NonZeroU64::new(100).unwrap(),
            )
            .unwrap();
        (game, actor)
    }

    #[test]
    fn queued_move_does_not_reinterpret_direction_after_teleport_or_frame_change() {
        for teleport in [true, false] {
            let (mut game, actor) = fixture();
            let id = game
                .admit_intention(actor, Action::Move(Direction::East), IntentionOrigin::Human)
                .unwrap();
            if teleport {
                game.teleport(
                    actor,
                    Location {
                        region: RegionId(1),
                        position: Position { x: 3, y: 1, z: 0 },
                    },
                )
                .unwrap();
            } else {
                game.actors.get_mut(&actor).unwrap().orientation = 1;
            }
            let mut expected = game.clone();
            expected.cancel_intention(actor, id).unwrap();
            let execution = game.execute_next_intention().unwrap();
            assert_eq!(execution.intention.id, id);
            assert_eq!(execution.outcome, Err(GameError::InvalidLocation));
            assert_eq!(
                game, expected,
                "failed guard must not move time, state or RNG"
            );
        }
    }

    #[test]
    fn saved_movement_context_is_required_and_preserves_execution() {
        let (mut game, actor) = fixture();
        game.admit_intention(actor, Action::Move(Direction::East), IntentionOrigin::Human)
            .unwrap();
        let mut shared = crate::checkpoint::SharedState::default();
        let snapshot = game.checkpoint(&mut shared);
        let original = serde_json::to_value((snapshot, shared)).unwrap();
        let (snapshot, shared) = serde_json::from_value(original.clone()).unwrap();
        let mut restored = Game::restore_checkpoint(snapshot, &shared).unwrap();
        assert_eq!(
            restored.execute_next_intention(),
            game.execute_next_intention()
        );
        assert_eq!(restored, game);
        for invalid_context in [
            serde_json::Value::Null,
            serde_json::json!({
                "from": {"region": 0, "position": {"x": 1, "y": 1, "z": 0}},
                "orientation": 0, "to": {"region": 1, "position": {"x": 2, "y": 1, "z": 0}},
                "next_orientation": 24
            }),
        ] {
            let mut invalid = original.clone();
            invalid[0]["intentions"]["entries"][actor.0.to_string()]["movement_context"] =
                invalid_context;
            let (snapshot, shared) = serde_json::from_value(invalid).unwrap();
            assert!(Game::restore_checkpoint(snapshot, &shared).is_none());
        }
        let mut missing = original;
        missing[0]["intentions"]["entries"][actor.0.to_string()]
            .as_object_mut()
            .unwrap()
            .remove("movement_context");
        assert!(serde_json::from_value::<(
            crate::checkpoint::Snapshot,
            crate::checkpoint::SharedState
        )>(missing)
        .is_err());
    }

    #[test]
    fn queued_move_preserves_rotated_portal_destination_and_frame() {
        use tor_world::{Extent, Passage, Region, World};
        let mut world = World::new(
            (1..=2)
                .map(|id| Region {
                    id: RegionId(id),
                    name: id.to_string(),
                    bounds: Extent::new(3, 3, 1).unwrap(),
                })
                .collect(),
            vec![],
        )
        .unwrap();
        let from = Location {
            region: RegionId(1),
            position: Position { x: 2, y: 1, z: 0 },
        };
        let to = Location {
            region: RegionId(2),
            position: Position { x: 1, y: 0, z: 0 },
        };
        world
            .connect(
                Passage {
                    from,
                    direction: Direction::East,
                    to,
                },
                1,
            )
            .unwrap();
        let mut game = Game::new(world, 42);
        let actor = game
            .spawn_actor(from, NonZeroU64::new(100).unwrap())
            .unwrap();
        let id = game
            .admit_intention(actor, Action::Move(Direction::East), IntentionOrigin::Human)
            .unwrap();
        let context = game
            .pending_intention(actor)
            .unwrap()
            .movement_context
            .unwrap();
        assert_eq!((context.to, context.next_orientation), (to, 1));
        let mut remapped = game.clone();
        let mut changed_world = World::new(
            (1..=2)
                .map(|id| Region {
                    id: RegionId(id),
                    name: id.to_string(),
                    bounds: Extent::new(3, 3, 1).unwrap(),
                })
                .collect(),
            vec![],
        )
        .unwrap();
        changed_world
            .connect(
                Passage {
                    from,
                    direction: Direction::East,
                    to,
                },
                3,
            )
            .unwrap();
        remapped.world = tor_world::Shared::new(changed_world);
        assert!(
            remapped.actor_translation(actor, Direction::East).is_some(),
            "the changed portal still permits movement, but has a different meaning"
        );
        let mut rejected = remapped.clone();
        rejected.cancel_intention(actor, id).unwrap();
        assert_eq!(
            remapped.execute_next_intention().unwrap().outcome,
            Err(GameError::InvalidLocation)
        );
        assert_eq!(remapped, rejected);
        let mut expected = game.clone();
        expected.cancel_intention(actor, id).unwrap();
        expected.act(actor, Action::Move(Direction::East)).unwrap();
        assert!(game.execute_next_intention().unwrap().outcome.is_ok());
        assert_eq!(game.actors[&actor].location, to);
        assert_eq!(game.actors[&actor].orientation, 1);
        assert_eq!(game, expected);
    }

    #[test]
    fn admitted_attack_identity_survives_wind_up_restore_and_impact() {
        let (mut game, actor) = fixture();
        let target = game
            .spawn_actor(
                Location {
                    region: RegionId(1),
                    position: Position { x: 2, y: 1, z: 0 },
                },
                NonZeroU64::new(100).unwrap(),
            )
            .unwrap();
        for id in [actor, target] {
            game.configure_combat(id, crate::combat::CombatSpec::default())
                .unwrap();
        }
        let intention = game
            .admit_intention(actor, Action::Attack { target }, IntentionOrigin::Human)
            .unwrap();
        game.execute_next_intention().unwrap().outcome.unwrap();
        let preparation = game
            .preparation(actor)
            .expect("target's turn holds wind-up open");
        assert_eq!(
            serde_json::to_value(preparation).unwrap()["intention"],
            serde_json::json!(intention)
        );
        let mut missing = serde_json::to_value(preparation).unwrap();
        missing.as_object_mut().unwrap().remove("intention");
        assert!(serde_json::from_value::<crate::combat::Preparation>(missing).is_err(),
            "saved progress requires an explicit admission context, including null for direct calls");
        let mut shared = crate::checkpoint::SharedState::default();
        let snapshot = game.checkpoint(&mut shared);
        for corrupt in [0, intention.0 + 1] {
            let mut value = serde_json::to_value(&snapshot).unwrap();
            value["actors"]["1"]["combat"]["pending"]["intention"] = serde_json::json!(corrupt);
            assert!(
                Game::restore_checkpoint(serde_json::from_value(value).unwrap(), &shared).is_none()
            );
        }
        let mut restored = Game::restore_checkpoint(snapshot, &shared).unwrap();
        assert_eq!(restored, game);
        restored.act(target, Action::Wait).unwrap();
        assert!(restored.preparation(actor).is_none());
        let resolved = restored
            .combat_events()
            .iter()
            .find_map(|event| {
                let value = serde_json::to_value(event).unwrap();
                value.get("Resolved").cloned()
            })
            .expect("attack impact");
        assert_eq!(resolved["intention"], serde_json::json!(intention));
    }

    #[test]
    fn admitted_attack_identity_is_assigned_before_same_execution_impact() {
        let (mut game, actor) = fixture();
        let target = game
            .spawn_actor(
                Location {
                    region: RegionId(1),
                    position: Position { x: 2, y: 1, z: 0 },
                },
                NonZeroU64::new(100).unwrap(),
            )
            .unwrap();
        let mut spec = crate::combat::CombatSpec::default();
        spec.attack.wind_up = 1;
        for id in [actor, target] {
            game.configure_combat(id, spec.clone()).unwrap();
        }
        game.actors.get_mut(&target).unwrap().ready_at = 100;
        let intention = game
            .admit_intention(actor, Action::Attack { target }, IntentionOrigin::Human)
            .unwrap();
        game.execute_next_intention().unwrap().outcome.unwrap();
        assert!(game.preparation(actor).is_none());
        assert!(game.combat_events().iter().any(|event| matches!(event,
            crate::combat::CombatEvent::Resolved { intention: Some(id), .. } if *id == intention)));
    }

    #[test]
    fn interrupted_wind_up_retains_its_admission_identity() {
        let (mut game, actor) = fixture();
        let target = game
            .spawn_actor(
                Location {
                    region: RegionId(1),
                    position: Position { x: 2, y: 1, z: 0 },
                },
                NonZeroU64::new(100).unwrap(),
            )
            .unwrap();
        for id in [actor, target] {
            game.configure_combat(id, crate::combat::CombatSpec::default())
                .unwrap();
        }
        let intention = game
            .admit_intention(actor, Action::Attack { target }, IntentionOrigin::Human)
            .unwrap();
        game.execute_next_intention().unwrap().outcome.unwrap();
        game.apply_damage(
            actor,
            &BTreeMap::from([(crate::combat::DamageType::Vital, 1)]),
        );
        let preparation = game.preparation(actor).unwrap();
        assert_eq!(preparation.intention, Some(intention));
        assert!(!preparation.active);
        assert!(game.combat_events().iter().any(|event| matches!(event,
            crate::combat::CombatEvent::Interrupted { intention: Some(id), .. } if *id == intention)));
        let mut shared = crate::checkpoint::SharedState::default();
        let snapshot = game.checkpoint(&mut shared);
        assert_eq!(Game::restore_checkpoint(snapshot, &shared).unwrap(), game);
    }

    #[test]
    fn admission_has_no_gameplay_effect_until_simulation_executes() {
        let (mut game, actor) = fixture();
        let before = game.clone();
        let id = game
            .admit_intention(actor, Action::Move(Direction::East), IntentionOrigin::Human)
            .unwrap();
        assert_eq!(game.tick(), before.tick());
        assert_eq!(game.actors[&actor].location, before.actors[&actor].location);
        assert_eq!(game.actors[&actor].ready_at, before.actors[&actor].ready_at);
        assert_eq!(game.pending_intention(actor).unwrap().id, id);
        let executed = game.execute_next_intention().unwrap();
        assert_eq!(executed.intention.id, id);
        assert!(executed.outcome.is_ok());
        assert_eq!(
            game.actors[&actor].location.position,
            Position { x: 2, y: 1, z: 0 }
        );
        assert!(game.pending_intention(actor).is_none());
    }

    #[test]
    fn queued_actions_follow_actor_scheduler_order_not_submission_order() {
        let (mut game, first) = fixture();
        let second = game
            .spawn_actor(
                Location {
                    region: RegionId(1),
                    position: Position { x: 3, y: 1, z: 0 },
                },
                NonZeroU64::new(100).unwrap(),
            )
            .unwrap();
        let second_id = game
            .admit_intention(second, Action::Wait, IntentionOrigin::Human)
            .unwrap();
        let first_id = game
            .admit_intention(first, Action::Wait, IntentionOrigin::Human)
            .unwrap();
        assert_eq!(
            game.execute_next_intention().unwrap().intention.id,
            first_id
        );
        assert_eq!(
            game.execute_next_intention().unwrap().intention.id,
            second_id
        );
        assert!(game.execute_next_intention().is_none());
    }

    #[test]
    fn deferred_ai_selects_once_at_execution_and_matches_direct_simulation() {
        let (mut game, actor) = fixture();
        game.configure_combat(
            actor,
            crate::combat::CombatSpec {
                faction: "foe".into(),
                ..Default::default()
            },
        )
        .unwrap();
        let hero = game
            .spawn_actor(
                Location {
                    region: RegionId(1),
                    position: Position { x: 2, y: 1, z: 0 },
                },
                NonZeroU64::new(100).unwrap(),
            )
            .unwrap();
        game.configure_combat(
            hero,
            crate::combat::CombatSpec {
                faction: "hero".into(),
                ..Default::default()
            },
        )
        .unwrap();
        game.configure_run(
            hero,
            BTreeSet::from([hero]),
            None,
            BTreeMap::from([("foe".into(), BTreeSet::from(["hero".into()]))]),
        )
        .unwrap();
        game.configure_ai(actor, crate::ai::AiProfile::default())
            .unwrap();
        game.act(hero, Action::Wait).unwrap();
        game.refresh_navigation();
        let before = crate::diagnostics::work_counts().route_searches;
        let id = game.admit_ai_intention(actor).unwrap();
        assert_eq!(crate::diagnostics::work_counts().route_searches, before);
        let mut expected = game.clone();
        expected.cancel_intention(actor, id).unwrap();
        // Supply the same provenance so equality checks rules, RNG and AI state
        // as well as the newly retained admission identity.
        let (action, outcome) = expected.act_ai_with_intention(actor, Some(id)).unwrap();
        let before = crate::diagnostics::work_counts().route_searches;
        let execution = game.execute_next_intention().unwrap();
        assert_eq!(crate::diagnostics::work_counts().route_searches - before, 1);
        assert_eq!(execution.action, Some(action));
        assert_eq!(execution.outcome, Ok(outcome));
        assert_eq!(game, expected);
    }

    #[test]
    fn execution_revalidates_the_original_target_and_records_failure_without_effect() {
        let (mut game, actor) = fixture();
        let at = game.actors[&actor].location;
        let item = game.place_item(at, "token".into()).unwrap();
        let id = game
            .admit_intention(
                actor,
                Action::Take {
                    item,
                    quantity: None,
                },
                IntentionOrigin::Human,
            )
            .unwrap();
        game.items.remove(&item);
        let tick = game.tick();
        let execution = game.execute_next_intention().unwrap();
        assert_eq!(execution.intention.id, id);
        assert_eq!(execution.outcome, Err(GameError::ItemUnavailable));
        assert_eq!(game.tick(), tick);
        assert!(game.pending_intention(actor).is_none());
        assert!(game.observe(actor).unwrap().inventory.is_empty());
    }

    #[test]
    fn saved_queue_preserves_identity_and_requires_explicit_resume_when_suspended() {
        let (mut game, actor) = fixture();
        let id = game
            .admit_intention(actor, Action::Wait, IntentionOrigin::Human)
            .unwrap();
        let before = game.clone();
        assert!(
            game.resume_intention(actor, id).is_err(),
            "queued work is not suspended work"
        );
        assert!(game
            .suspend_intention(actor, IntentionId(id.0 + 1))
            .is_err());
        assert_eq!(game, before);
        game.suspend_intention(actor, id).unwrap();
        let suspended = game.clone();
        assert!(game.suspend_intention(actor, id).is_err());
        assert_eq!(game, suspended);
        game.suspend_human_intentions();
        let mut shared = crate::checkpoint::SharedState::default();
        let snapshot = game.checkpoint(&mut shared);
        let bytes = serde_json::to_vec(&(snapshot, shared)).unwrap();
        let (snapshot, shared) = serde_json::from_slice(&bytes).unwrap();
        let mut restored = Game::restore_checkpoint(snapshot, &shared).unwrap();
        assert_eq!(restored, game);
        assert_eq!(restored.pending_intention(actor).unwrap().id, id);
        assert_eq!(restored.next_intention_actor(), None);
        assert!(restored.execute_next_intention().is_none());
        restored.resume_intention(actor, id).unwrap();
        assert_eq!(restored.next_intention_actor(), Some(actor));
        assert_eq!(restored.execute_next_intention().unwrap().intention.id, id);
    }

    #[test]
    fn malformed_saved_action_structure_is_rejected_without_rechecking_availability() {
        let (mut game, actor) = fixture();
        let item = game
            .place_item(game.actors[&actor].location, "token".into())
            .unwrap();
        game.admit_intention(
            actor,
            Action::Take {
                item,
                quantity: Some(1),
            },
            IntentionOrigin::Human,
        )
        .unwrap();
        let mut shared = crate::checkpoint::SharedState::default();
        let snapshot = game.checkpoint(&mut shared);
        let original = serde_json::to_value((snapshot, shared)).unwrap();
        for field in ["quantity", "item"] {
            let mut invalid = original.clone();
            invalid[0]["intentions"]["entries"][actor.0.to_string()]["work"]["action"]["value"]
                [field] = serde_json::json!(0);
            let (snapshot, shared) = serde_json::from_value(invalid).unwrap();
            assert!(
                Game::restore_checkpoint(snapshot, &shared).is_none(),
                "invalid {field}"
            );
        }
    }

    #[test]
    fn rewind_restores_pending_work_without_reusing_abandoned_future_identities() {
        let (mut game, actor) = fixture();
        let first = game
            .admit_intention(actor, Action::Wait, IntentionOrigin::Human)
            .unwrap();
        let mut boundary = game.clone();
        game.execute_next_intention().unwrap();
        let abandoned = game
            .admit_intention(actor, Action::Wait, IntentionOrigin::Human)
            .unwrap();
        boundary.continue_intention_ids(&game);
        assert_eq!(boundary.pending_intention(actor).unwrap().id, first);
        boundary.cancel_intention(actor, first).unwrap();
        let next = boundary
            .admit_intention(actor, Action::Wait, IntentionOrigin::Human)
            .unwrap();
        assert!(next > abandoned);
        assert!(boundary.cancel_intention(actor, first).is_err());
        assert_eq!(boundary.pending_intention(actor).unwrap().id, next);
    }

    #[test]
    fn capacity_and_busy_rejections_preserve_state_and_identity_allocation() {
        let mut world = tor_world::World::new(vec![], vec![]).unwrap();
        world
            .add_region(tor_world::Region {
                id: RegionId(1),
                name: "queue".into(),
                bounds: tor_world::Extent::new(MAX_QUEUED_INTENTIONS as i32 + 2, 3, 1).unwrap(),
            })
            .unwrap();
        let mut game = Game::new(world, 42);
        let mut actors = Vec::new();
        for x in 0..=MAX_QUEUED_INTENTIONS {
            actors.push(
                game.spawn_actor(
                    Location {
                        region: RegionId(1),
                        position: Position {
                            x: x as i32,
                            y: 1,
                            z: 0,
                        },
                    },
                    NonZeroU64::new(100).unwrap(),
                )
                .unwrap(),
            );
        }
        for &actor in &actors[..MAX_QUEUED_INTENTIONS] {
            game.admit_intention(actor, Action::Wait, IntentionOrigin::Human)
                .unwrap();
        }
        let full = game.clone();
        assert_eq!(
            game.admit_intention(actors[0], Action::Wait, IntentionOrigin::Human),
            Err(GameError::ActorBusy)
        );
        assert_eq!(
            game.admit_intention(
                actors[MAX_QUEUED_INTENTIONS],
                Action::Wait,
                IntentionOrigin::Human
            ),
            Err(GameError::QueueFull)
        );
        assert_eq!(game, full);
        game.cancel_intention(actors[0], game.pending_intention(actors[0]).unwrap().id)
            .unwrap();
        let id = game
            .admit_intention(
                actors[MAX_QUEUED_INTENTIONS],
                Action::Wait,
                IntentionOrigin::Human,
            )
            .unwrap();
        assert_eq!(id.0, MAX_QUEUED_INTENTIONS as u64 + 1);
    }

    #[test]
    fn a_dead_queued_actor_yields_a_failure_instead_of_stuck_work() {
        let (mut game, actor) = fixture();
        game.configure_combat(actor, crate::combat::CombatSpec::default())
            .unwrap();
        let id = game
            .admit_intention(actor, Action::Wait, IntentionOrigin::Human)
            .unwrap();
        game.apply_damage(
            actor,
            &BTreeMap::from([(crate::combat::DamageType::Vital, 100)]),
        );
        let before = game.tick();
        assert_eq!(game.next_intention_actor(), Some(actor));
        assert_eq!(game.tick(), before);
        let failed = game
            .execute_next_intention()
            .expect("dead queued actor must resolve");
        assert_eq!(failed.intention.id, id);
        assert_eq!(failed.outcome, Err(GameError::UnknownActor));
        assert_eq!(game.tick(), before);
        assert!(game.pending_intention(actor).is_none());
    }
}
