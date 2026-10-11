//! Actor-local lifecycle outcomes and authoritative state application.
//!
//! Planners edit owned actor snapshots. This boundary alone publishes their
//! creature state, reconciles preparations, performs death cleanup, and emits
//! combat events. It runs inside the caller's existing Game transaction.
use crate::abilities::{Outcome, ResolvedEffect, RuleError};
use crate::combat::CombatEvent;
use crate::{Actor, ActorId, Game, GameError, Item, ItemId, ItemLocation, ItemSpec, Work};

pub(crate) enum ApplicationOutcome {
    Resolved(Outcome),
    Actor(ActorOutcome),
    Item(ItemOutcome),
}
impl From<Outcome> for ApplicationOutcome {
    fn from(value: Outcome) -> Self {
        Self::Resolved(value)
    }
}
impl From<ActorOutcome> for ApplicationOutcome {
    fn from(value: ActorOutcome) -> Self {
        Self::Actor(value)
    }
}
impl From<ItemOutcome> for ApplicationOutcome {
    fn from(value: ItemOutcome) -> Self {
        Self::Item(value)
    }
}

pub(crate) struct ActorOutcome {
    pub id: ActorId,
    pub state: Actor,
    events: Vec<CombatEvent>,
    input: InputBoundary,
}
#[derive(Default)]
enum InputBoundary {
    #[default]
    Preserve,
    Human,
    Require,
}
pub(crate) enum Interruption {
    Suspend,
    Discard,
}
impl ActorOutcome {
    pub(crate) fn interrupt(&mut self, mode: Interruption, clock: u64) {
        let Some(preparation) = self.state.pending.as_ref() else {
            return;
        };
        if matches!(mode, Interruption::Suspend) && !preparation.active {
            return;
        }
        let intention = preparation.intention;
        match mode {
            Interruption::Suspend => {
                self.pause(clock);
            }
            Interruption::Discard => {
                self.state.cancel_preparation();
                self.state.ready_at = clock;
            }
        }
        if !self.events.iter().any(|event| matches!(event,
            CombatEvent::Interrupted { actor, intention: prior } if *actor == self.id && *prior == intention)) {
            self.events.push(CombatEvent::Interrupted { actor: self.id, intention });
        }
        self.input = InputBoundary::Human;
    }
    pub(crate) fn pause(&mut self, clock: u64) -> Option<Work> {
        let preparation = self.state.pending.as_mut().filter(|p| p.active)?;
        preparation.remaining = preparation
            .remaining
            .saturating_sub(clock.saturating_sub(preparation.started));
        preparation.active = false;
        self.state.ready_at = clock;
        self.input = InputBoundary::Require;
        Some(preparation.plan.work())
    }
    pub(crate) fn require_human_input(&mut self) {
        self.input = InputBoundary::Human;
    }
}

pub(crate) struct ItemOutcome {
    pub actor: ActorId,
    pub work: Work,
    pub intention: Option<crate::IntentionId>,
    pub change: ItemChange,
}
pub(crate) enum ItemChange {
    Equipment(ActorOutcome),
    Consume { actor: ActorOutcome, item: ItemId },
}

struct DeathOutcome {
    corpse: ItemId,
    next_item_id: u64,
    body: Item,
    inventory: Vec<ItemId>,
}

impl Game {
    pub(crate) fn actor_outcome(&self, id: ActorId) -> Result<ActorOutcome, GameError> {
        let state = self.actors.get(&id).ok_or(GameError::UnknownActor)?.clone();
        Ok(ActorOutcome {
            id,
            state,
            events: vec![],
            input: InputBoundary::Preserve,
        })
    }

    /// Read a settled actor snapshot without committing timers on a later error.
    pub(crate) fn settled_actor_outcome(&self, id: ActorId) -> Result<ActorOutcome, GameError> {
        let mut outcome = self.actor_outcome(id)?;
        let state = &mut outcome.state;
        let clock = self.actor_clock(id);
        let elapsed = clock.saturating_sub(state.active_since);
        if elapsed > 0 {
            if let Some(combat) = state.combat.as_mut() {
                combat.advance_active(elapsed);
            }
            state.active_since = clock;
            #[cfg(test)]
            crate::creature_time::count_settlement();
        }
        Ok(outcome)
    }

    /// External effect application owns one transaction; internal lifecycle callers
    /// use the already existing action/physics transaction and actor-local snapshots.
    pub fn apply(&mut self, outcome: Outcome) -> Result<(), GameError> {
        let mut candidate = self.clone();
        candidate.apply_outcome(outcome)?;
        *self = candidate;
        Ok(())
    }

    pub(crate) fn apply_outcome(
        &mut self,
        outcome: impl Into<ApplicationOutcome>,
    ) -> Result<(), GameError> {
        match outcome.into() {
            ApplicationOutcome::Resolved(outcome) => {
                let mut transition = self.settled_actor_outcome(outcome.target)?;
                for effect in &outcome.effects {
                    if matches!(effect, ResolvedEffect::ApplyCondition { .. })
                        && !self.actors.contains_key(&outcome.actor)
                        && !self.detached_actor(outcome.actor)
                    {
                        return Err(GameError::UnknownActor);
                    }
                    let applied = transition
                        .state
                        .combat
                        .as_mut()
                        .ok_or(GameError::Rule(RuleError::InvalidDefinition))?
                        .apply_effect(outcome.actor, effect)?;
                    if matches!(applied, Some(ResolvedEffect::Damage { amount }) if amount > 0) {
                        transition.interrupt(Interruption::Suspend, self.tick);
                    }
                }
                // A resolution event precedes its interruption/death consequences.
                transition
                    .events
                    .insert(0, CombatEvent::Resolved { outcome });
                self.commit_actor_outcome(transition)?;
            }
            ApplicationOutcome::Actor(outcome) => self.commit_actor_outcome(outcome)?,
            ApplicationOutcome::Item(outcome) => {
                self.event_log.push(CombatEvent::ItemCompleted {
                    actor: outcome.actor,
                    work: outcome.work,
                    intention: outcome.intention,
                });
                match outcome.change {
                    ItemChange::Equipment(actor) => self.commit_actor_outcome(actor)?,
                    ItemChange::Consume { actor, item } => {
                        let consumed = self.items.get(&item).ok_or(GameError::ItemUnavailable)?;
                        if consumed.quantity == 1 {
                            self.items.remove(&item);
                        } else {
                            self.items
                                .edit(item, |item| item.quantity -= 1)
                                .ok_or(GameError::ItemUnavailable)?;
                        }
                        self.commit_actor_outcome(actor)?;
                    }
                }
            }
        }
        Ok(())
    }

    fn commit_actor_outcome(&mut self, mut outcome: ActorOutcome) -> Result<(), GameError> {
        let before = self
            .actors
            .get(&outcome.id)
            .ok_or(GameError::UnknownActor)?;
        let died = before.alive() && !outcome.state.alive();
        let invalid_preparation = outcome.state.pending.as_ref().is_some_and(|pending| {
            outcome.state.combat.as_ref().is_some_and(|combat| {
                pending
                    .hold
                    .is_some_and(|owner| combat.creature().costs().reservation(owner).is_none())
                    || pending.plan.as_ability().is_some_and(|plan| {
                        !combat
                            .creature()
                            .derived()
                            .abilities
                            .contains(&plan.ability)
                    })
            })
        });
        if invalid_preparation {
            outcome.interrupt(Interruption::Discard, self.actor_clock(outcome.id));
        }
        // Validate every fallible death operation before publishing the actor.
        let death = if died {
            Some(self.death_outcome(&mut outcome)?)
        } else {
            None
        };
        let id = outcome.id;
        self.actors.insert(id, outcome.state);
        match outcome.input {
            InputBoundary::Preserve => {}
            InputBoundary::Human => {
                if !self.is_ai(id) {
                    self.combat.input_boundaries.insert(id);
                }
            }
            InputBoundary::Require => {
                self.combat.input_boundaries.insert(id);
            }
        }
        self.event_log.extend(outcome.events);
        if let Some(death) = death {
            self.combat.input_boundaries.remove(&id);
            for item in death.inventory {
                self.items
                    .edit(item, |item| {
                        item.location = death.body.location;
                        item.motion = death.body.motion.clone();
                        item.orientation = death.body.orientation;
                    })
                    .expect("carried item from current inventory index");
            }
            self.next_item_id = death.next_item_id;
            self.items.insert(death.corpse, death.body);
            self.on_actor_death(id)?;
        }
        Ok(())
    }

    fn death_outcome(&self, outcome: &mut ActorOutcome) -> Result<DeathOutcome, GameError> {
        let actor = &mut outcome.state;
        let combat = actor.combat.as_mut().ok_or(GameError::UnknownActor)?;
        combat
            .set_equipment(Vec::new())
            .map_err(GameError::Creature)?;
        let next_item_id = self
            .next_item_id
            .checked_add(1)
            .ok_or(GameError::IdentityExhausted)?;
        let mut spec = ItemSpec::ordinary(format!("{} corpse", combat.identity.name));
        spec.class = crate::ItemClass::Corpse;
        spec.properties
            .insert("actor".into(), outcome.id.0.to_string());
        spec.properties
            .insert("death_tick".into(), self.tick.to_string());
        actor.cancel_preparation();
        actor.equipment.clear();
        let body = Item {
            spec: tor_world::Shared::new(spec),
            quantity: 1,
            location: ItemLocation::Ground(actor.location),
            motion: std::mem::take(&mut actor.motion),
            orientation: actor.orientation,
        };
        outcome.events.push(CombatEvent::Died { actor: outcome.id });
        Ok(DeathOutcome {
            corpse: ItemId(self.next_item_id),
            next_item_id,
            body,
            inventory: self.items.at(ItemLocation::Carried(outcome.id)).collect(),
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::combat::CombatEvent;
    use crate::content_ids::{AbilityId, TemplateId};
    use crate::rules::{Modifier, ModifierOp, Scope};
    use crate::{Action, ActorId, CreatureIdentity, Game};

    fn game() -> Game {
        let mut document = crate::test_support::test_ruleset().document().clone();
        let mut ability = document
            .abilities
            .iter()
            .find(|ability| ability.basic_attack)
            .unwrap()
            .clone();
        ability.id = AbilityId::new("borrowed_strike").unwrap();
        ability.name = "Borrowed strike".into();
        ability.basic_attack = false;
        ability.cost = None;
        document.abilities.push(ability.clone());
        document.conditions.push(crate::rules::ConditionDef {
            id: crate::content_ids::ConditionId::new("borrowed_grant").unwrap(),
            name: "Borrowed grant".into(),
            modifiers: vec![Modifier {
                scope: Scope::Always,
                op: ModifierOp::GrantAbility {
                    ability: ability.id.clone(),
                },
            }],
            stacking: crate::rules::Stacking::Longest,
            source_scoped: true,
            descriptors: vec![],
            capacity: crate::rules::Eviction::Oldest,
        });
        let mut template =
            crate::creatures::Template::new(TemplateId::new("borrowed_grant").unwrap(), 0);
        template.modifiers.push(Modifier {
            scope: Scope::Always,
            op: ModifierOp::GrantAbility {
                ability: ability.id,
            },
        });
        template.modifiers.push(Modifier {
            scope: Scope::Always,
            op: ModifierOp::GrantAbility {
                ability: AbilityId::new("power_strike").unwrap(),
            },
        });
        let species = crate::test_creatures::species();
        let species_id = species.id.clone();
        let content = crate::creatures::Content::new(
            crate::rules::Ruleset::validate(document).unwrap(),
            crate::creatures::ScenarioCatalog {
                species: [(species_id.clone(), species)].into(),
                templates: [(template.id.clone(), template)].into(),
            },
        )
        .unwrap();
        let mut build = crate::creatures::CreatureBuild::new(
            content.clone(),
            species_id,
            crate::progression::HdLedger::seeded(vec![crate::progression::HdSource::Racial], 7)
                .unwrap(),
            crate::attributes::ManaBinding::Intellect,
        )
        .unwrap();
        build
            .set_templates(vec![TemplateId::new("borrowed_grant").unwrap()])
            .unwrap();
        let mut game = Game::two_room_in_stone(42, content);
        for x in [1, 2] {
            let id = game
                .spawn_actor(
                    tor_world::Location {
                        region: tor_world::RegionId(1),
                        position: tor_world::Position { x, y: 1, z: 0 },
                    },
                    std::num::NonZeroU64::new(100).unwrap(),
                )
                .unwrap();
            game.configure_creature(
                id,
                CreatureIdentity {
                    name: format!("subject {x}"),
                    faction: "neutral".into(),
                },
                build.clone(),
            )
            .unwrap();
        }
        game
    }

    #[test]
    fn rebuild_losing_a_free_ability_cancels_its_captured_preparation() {
        let mut game = game();
        let actor = ActorId(1);
        game.act(
            actor,
            Action::UseAbility {
                id: AbilityId::new("borrowed_strike").unwrap(),
                target: ActorId(2),
            },
        )
        .unwrap();
        assert!(game.preparation(actor).unwrap().hold.is_none());
        let mut build = game.creature(actor).unwrap().build().clone();
        build.set_templates(vec![]).unwrap();
        game.rebuild_creature(actor, build).unwrap();
        assert!(game.preparation(actor).is_none());
        assert!(matches!(
            game.combat_events().last(),
            Some(crate::combat::CombatEvent::Interrupted {
                actor: ActorId(1),
                ..
            })
        ));
    }

    #[test]
    fn expired_free_grant_interrupts_before_preparation_completion() {
        let mut game = game();
        let actor = ActorId(1);
        let mut build = game.creature(actor).unwrap().build().clone();
        build.set_templates(vec![]).unwrap();
        game.rebuild_creature(actor, build).unwrap();
        game.apply_condition(
            actor,
            &crate::content_ids::ConditionId::new("borrowed_grant").unwrap(),
            ActorId(2),
            10,
        )
        .unwrap();
        game.act(
            actor,
            Action::UseAbility {
                id: AbilityId::new("borrowed_strike").unwrap(),
                target: ActorId(2),
            },
        )
        .unwrap();
        assert!(game.preparation(actor).is_some());
        game.tick = 10;
        game.settle_creature_time().unwrap();
        assert!(game.preparation(actor).is_none());
        assert_eq!(
            game.combat_events()
                .iter()
                .filter(|event| matches!(
                    event,
                    CombatEvent::Interrupted {
                        actor: ActorId(1),
                        ..
                    }
                ))
                .count(),
            1
        );
        assert!(!game
            .combat_events()
            .iter()
            .any(|event| matches!(event, CombatEvent::Resolved { .. })));
    }

    #[test]
    fn lethal_paid_work_emits_one_interruption_and_one_death() {
        let mut game = game();
        let actor = ActorId(1);
        let intention = game
            .admit_intention(
                actor,
                Action::UseAbility {
                    id: AbilityId::new("power_strike").unwrap(),
                    target: ActorId(2),
                },
                crate::IntentionOrigin::Human,
            )
            .unwrap();
        game.execute_next_intention().unwrap().outcome.unwrap();
        assert_eq!(game.preparation(actor).unwrap().hold, Some(intention));
        crate::test_creatures::injure(&mut game, actor, 100).unwrap();
        assert!(!game.alive(actor));
        assert!(game.preparation(actor).is_none());
        assert!(game
            .creature(actor)
            .unwrap()
            .costs()
            .reservations()
            .is_empty());
        assert_eq!(
            game.combat_events()
                .iter()
                .filter(|event| matches!(
                    event,
                    CombatEvent::Interrupted {
                        actor: ActorId(1),
                        ..
                    }
                ))
                .count(),
            1
        );
        assert_eq!(
            game.combat_events()
                .iter()
                .filter(|event| matches!(event, CombatEvent::Died { actor: ActorId(1) }))
                .count(),
            1
        );
    }

    #[test]
    fn rejected_condition_does_not_commit_elapsed_timers_or_events() {
        let mut game = game();
        let actor = ActorId(1);
        let fear = crate::content_ids::ConditionId::new("fear").unwrap();
        game.apply_condition(actor, &fear, ActorId(2), 100).unwrap();
        game.tick = 10;
        let before = game.clone();
        assert_eq!(
            game.apply_condition(actor, &fear, ActorId(999), 20),
            Err(crate::GameError::UnknownActor)
        );
        assert!(
            game == before,
            "rejected edit must preserve the complete game"
        );
    }
}
