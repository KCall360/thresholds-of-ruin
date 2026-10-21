//! A one-lifetime, source-owned completed preparation. It contains no opponent;
//! physical touch binds the live recipient and spends the entitlement exactly once.
use crate::abilities::Placement;
use crate::content_ids::AbilityId;
use crate::creatures::{record::CreatureRecord, CreatureState};
use crate::{Actor, ActorId, Game, GameError};
use serde::{Deserialize, Serialize};
use tor_world::Shared;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) enum ReadiedAmbush {
    #[default]
    Unacquired,
    Ready {
        source: Shared<CreatureState>,
        at: Placement,
        body: Shared<crate::BodySpec>,
        ability: AbilityId,
    },
    Provoked {
        toucher: ActorId,
    },
    Spent,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum ReadiedRecord {
    Unacquired,
    Ready {
        source: CreatureRecord,
        at: Placement,
        body: Shared<crate::BodySpec>,
        ability: AbilityId,
    },
    Provoked {
        toucher: ActorId,
    },
    Spent,
}
impl<'de> Deserialize<'de> for ReadiedRecord {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            Unacquired {},
            Ready {
                source: CreatureRecord,
                at: Placement,
                body: Shared<crate::BodySpec>,
                ability: AbilityId,
            },
            Provoked {
                toucher: ActorId,
            },
            Spent {},
        }
        Ok(match crate::bounded::named_map(d)? {
            Fields::Unacquired {} => Self::Unacquired,
            Fields::Ready {
                source,
                at,
                body,
                ability,
            } => Self::Ready {
                source,
                at,
                body,
                ability,
            },
            Fields::Provoked { toucher } => {
                if toucher.0 == 0 || toucher.0 == u64::MAX {
                    return Err(serde::de::Error::custom("invalid toucher"));
                }
                Self::Provoked { toucher }
            }
            Fields::Spent {} => Self::Spent,
        })
    }
}
impl ReadiedRecord {
    pub(crate) fn capture(state: &ReadiedAmbush) -> Self {
        match state {
            ReadiedAmbush::Unacquired => Self::Unacquired,
            ReadiedAmbush::Spent => Self::Spent,
            ReadiedAmbush::Provoked { toucher } => Self::Provoked { toucher: *toucher },
            ReadiedAmbush::Ready {
                source,
                at,
                body,
                ability,
            } => Self::Ready {
                source: CreatureRecord::capture(source),
                at: *at,
                body: body.clone(),
                ability: ability.clone(),
            },
        }
    }
    pub(crate) fn restore(
        &self,
        content: std::sync::Arc<crate::creatures::Content>,
    ) -> Result<ReadiedAmbush, crate::creatures::CreatureStateError> {
        Ok(match self {
            Self::Unacquired => ReadiedAmbush::Unacquired,
            Self::Spent => ReadiedAmbush::Spent,
            Self::Provoked { toucher } => ReadiedAmbush::Provoked { toucher: *toucher },
            Self::Ready {
                source,
                at,
                body,
                ability,
            } => ReadiedAmbush::Ready {
                source: Shared::new(source.restore(content, vec![])?),
                at: *at,
                body: body.clone(),
                ability: ability.clone(),
            },
        })
    }
}

pub(crate) fn eligible_definition(definition: &crate::rules::AbilityDef) -> bool {
    definition.basic_attack
        && definition.invocation == crate::abilities::InvocationKind::ActorTarget
        && definition.interaction == crate::rules::Interaction::HostileToTarget
        && matches!(definition.reach, crate::rules::Reach::Melee)
        && matches!(
            definition.strikes,
            crate::rules::StrikeSource::SelectedMelee
                | crate::rules::StrikeSource::NaturalMelee { .. }
        )
        && definition.cost.is_none()
        && definition.cooldown.is_none()
        && definition.supply_costs.is_empty()
}
pub(crate) fn eligible_ability(creature: &CreatureState) -> Option<AbilityId> {
    creature
        .derived()
        .abilities
        .iter()
        .find(|id| {
            creature
                .build()
                .ruleset()
                .ability(id)
                .is_some_and(eligible_definition)
        })
        .cloned()
}
impl ReadiedAmbush {
    pub(crate) fn toucher(&self) -> Option<ActorId> {
        match self {
            Self::Provoked { toucher } => Some(*toucher),
            _ => None,
        }
    }
    pub(crate) fn retire(&mut self) {
        if !matches!(self, Self::Unacquired) {
            *self = Self::Spent;
        }
    }
    pub(crate) fn valid_for(&self, actor: &Actor) -> bool {
        match self {
            Self::Unacquired => {
                !actor.perception().readied_touch_ambush || actor.guise.appearance().is_none()
            }
            Self::Spent => true,
            Self::Provoked { toucher } => {
                toucher.0 != 0
                    && toucher.0 != u64::MAX
                    && actor.alive()
                    && actor.guise.appearance().is_some()
                    && actor.perception().readied_touch_ambush
            }
            Self::Ready {
                source,
                at,
                body,
                ability,
            } => {
                actor.alive()
                    && actor.pending.is_none()
                    && actor.equipment.is_empty()
                    && actor.perception().readied_touch_ambush
                    && actor.guise.appearance().is_some()
                    && actor.body == *body
                    && actor.location == at.location
                    && actor.orientation == at.orientation
                    && eligible_ability(source).as_ref() == Some(ability)
                    && !source.health().dead()
                    && actor.combat.as_ref().is_some_and(|current| {
                        current.creature().build() == source.build()
                            && current.creature().build_epoch() == source.build_epoch()
                            && current.creature().derived() == source.derived()
                            && current.creature().conditions() == source.conditions()
                    })
            }
        }
    }
}
impl Game {
    pub(crate) fn reconcile_readied_ambush(
        &self,
        outcome: &mut crate::lifecycle::ActorOutcome,
    ) -> Result<(), GameError> {
        let actor = &mut outcome.state;
        if matches!(actor.readied_ambush, ReadiedAmbush::Unacquired)
            && actor.perception().readied_touch_ambush
            && actor.guise.appearance().is_some()
        {
            let creature = actor
                .combat
                .as_ref()
                .ok_or(GameError::UnknownActor)?
                .creature();
            // An equipment-bearing or already-busy acquisition does not receive
            // an unearned preparation. The one lifetime opportunity is retired.
            actor.readied_ambush =
                if actor.alive() && actor.pending.is_none() && actor.equipment.is_empty() {
                    let ability = eligible_ability(creature).ok_or(GameError::Rule(
                        crate::abilities::RuleError::InvalidDefinition,
                    ))?;
                    ReadiedAmbush::Ready {
                        source: Shared::new(creature.clone()),
                        at: Placement {
                            location: actor.location,
                            orientation: actor.orientation,
                        },
                        body: actor.body.clone(),
                        ability,
                    }
                } else {
                    ReadiedAmbush::Spent
                };
        } else if !actor.readied_ambush.valid_for(actor) {
            actor.readied_ambush.retire();
        }
        Ok(())
    }

    /// Called only by an already-authorized physical touch, never a private
    /// discovery receipt. The next ordinary completion pass releases at this tick.
    pub(crate) fn touch_readied_ambush(
        &mut self,
        subject: ActorId,
        toucher: ActorId,
    ) -> Result<(), GameError> {
        self.settle_actor_time(subject)?;
        let Some(actor) = self.actors.get(&subject) else {
            return Ok(());
        };
        if actor.perception().readied_touch_ambush
            && actor.guise.appearance().is_some()
            && (!actor.readied_ambush.valid_for(actor)
                || !matches!(actor.readied_ambush, ReadiedAmbush::Ready { .. }))
        {
            // Real contact supplies this identity, not private recognition.
            // Remember the provocation without exposing the guise or granting
            // new preparation credit; ordinary scheduling owns any later attack.
            let mut outcome = self.actor_outcome(subject)?;
            if outcome.state.readied_ambush.toucher().is_none() {
                outcome.state.readied_ambush = ReadiedAmbush::Provoked { toucher };
            }
            return self.apply_outcome(outcome);
        }
        if !actor.readied_ambush.valid_for(actor)
            || self.actor_frozen(subject)
            || !self.condition_allows_voluntary_input(subject)
            || !self.attack_available(subject, toucher)
        {
            return Ok(());
        }
        let ReadiedAmbush::Ready { ability, .. } = &actor.readied_ambush else {
            return Ok(());
        };
        let ability = ability.clone();
        let plan = self.compile_touch_strike_plan(subject, &ability, toucher)?;
        if plan.cost().is_some()
            || !plan
                .as_ability()
                .is_some_and(|plan| plan.supply_costs.is_empty() && plan.cooldown.is_none())
        {
            return Err(GameError::Rule(crate::abilities::RuleError::InvalidPlan));
        }
        let intention = self.issue_reaction_intention()?;
        self.start_readied_work(plan, intention)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::perception::{contract_fixtures::*, PerceptionValue};
    use crate::{combat::CombatEvent, Action};

    fn ready_fixture() -> Game {
        fixture(
            &[],
            &[
                guise(),
                ambush(),
                PerceptionValue::ReadiedTouchAmbush { enabled: true },
            ],
        )
    }
    fn released(game: &Game) -> usize {
        game.combat_events()
            .iter()
            .filter(|event| {
                matches!(event,
            CombatEvent::SequenceEnded { actor, .. } if *actor == SUBJECT)
            })
            .count()
    }
    #[test]
    fn contact_releases_one_completed_preparation_and_ordinary_recovery() {
        let mut game = ready_fixture();
        assert!(matches!(
            game.actors[&SUBJECT].readied_ambush,
            ReadiedAmbush::Ready { .. }
        ));
        let plan = game
            .compile_touch_strike_plan(SUBJECT, &id("basic_melee"), OWNER)
            .unwrap();
        let recovery = plan.recovery();
        let now = game.tick();
        assert_eq!(restore(&game), game, "valid independent initial boundary");
        game.begin_effect_publication().unwrap();
        // Waiting has its own next ordinary decision; the already-completed
        // preparation is an independent entitlement, not a new free wind-up.
        game.actors.get_mut(&SUBJECT).unwrap().ready_at = now + 100;
        game.touch_readied_ambush(SUBJECT, OWNER).unwrap();
        assert_eq!(game.actors[&SUBJECT].ready_at, now);
        assert!(game.actors[&SUBJECT].guise.appearance().is_none());
        game.resolve_preparations().unwrap();
        assert_eq!(game.tick(), now);
        assert_eq!(game.actors[&SUBJECT].ready_at, now + recovery);
        assert_eq!(released(&game), 1);
        game.touch_readied_ambush(SUBJECT, OWNER).unwrap();
        game.resolve_preparations().unwrap();
        assert_eq!(released(&game), 1);
        game.finish_effect_publication().unwrap();
        assert!(game.event_receipts_valid());
        assert!(game.combat_valid());
        assert_eq!(restore(&game), game);
    }
    #[test]
    fn paid_pickup_releases_at_contact_instead_of_waiting_a_new_preparation() {
        let mut game = ready_fixture();
        let outcome = game
            .act(
                OWNER,
                Action::TouchApparentObject {
                    subject: SUBJECT,
                    quantity: None,
                },
            )
            .unwrap();
        assert_eq!(outcome.at_tick, 0);
        assert_eq!(released(&game), 1);
        assert!(matches!(
            game.actors[&SUBJECT].readied_ambush,
            ReadiedAmbush::Spent
        ));
        assert!(game.preparation(SUBJECT).is_none());
    }
    #[test]
    fn attempted_physical_bump_also_releases_without_moving_or_transferring_items() {
        let mut game = ready_fixture();
        let items = game.items.clone();
        game.act(OWNER, Action::Move(tor_world::Direction::East))
            .unwrap();
        assert_eq!(game.actors[&OWNER].location, at(1, 2, 2));
        assert_eq!(game.actors[&SUBJECT].location, at(1, 3, 2));
        assert_eq!(released(&game), 1);
        assert_eq!(game.items, items);
    }

    #[test]
    fn mere_adjacent_observation_preserves_waiting_source_and_rng() {
        let mut game = ready_fixture();
        game.configure_ai(SUBJECT, crate::ai::AiProfile::default())
            .unwrap();
        let before = game.clone();
        assert_eq!(game.choose_ai(SUBJECT).unwrap().action, Action::Wait);
        assert_eq!(game, before);
        assert_eq!(restore(&game), game);
    }
    #[test]
    fn incoming_attack_abandons_guise_and_retires_the_touch_entitlement() {
        let mut game = ready_fixture();
        let plan = game
            .compile_touch_strike_plan(OWNER, &id("basic_melee"), SUBJECT)
            .unwrap();
        let preparation = plan.preparation();
        let intention = game.issue_reaction_intention().unwrap();
        game.start_work(plan, Some(intention)).unwrap();
        assert!(matches!(
            game.actors[&SUBJECT].readied_ambush,
            ReadiedAmbush::Ready { .. }
        ));
        game.tick += preparation;
        game.resolve_preparations().unwrap();
        assert!(game.actors[&SUBJECT].guise.appearance().is_none());
        assert!(matches!(
            game.actors[&SUBJECT].readied_ambush,
            ReadiedAmbush::Spent
        ));
        game.touch_readied_ambush(SUBJECT, OWNER).unwrap();
        assert_eq!(released(&game), 0);
    }

    #[test]
    fn displacement_retires_preparation_and_never_rearms_on_return() {
        let mut game = ready_fixture();
        game.teleport(SUBJECT, at(1, 5, 2)).unwrap();
        game.teleport(SUBJECT, at(1, 3, 2)).unwrap();
        assert!(matches!(
            game.actors[&SUBJECT].readied_ambush,
            ReadiedAmbush::Spent
        ));
        game.touch_readied_ambush(SUBJECT, OWNER).unwrap();
        assert_eq!(released(&game), 0);
        assert_eq!(restore(&game), game);
    }
    #[test]
    fn authored_nondefault_body_is_captured_before_initial_preparation() {
        let mut game = ready_fixture();
        let build = game.creature(SUBJECT).unwrap().build().clone();
        let body = crate::BodySpec {
            cells: vec![[0, 0, 0], [0, 1, 0]],
            eye: [0, 1, 0],
            mass: 120,
        };
        let definition = crate::prepared_spawn::PreparedActor {
            build: Some(crate::creatures::record::BuildRecord::capture(&build)),
            identity: Some(crate::CreatureIdentity {
                name: "body witness".into(),
                faction: "neutral".into(),
            }),
            nutrition: crate::nutrition::NutritionRequirement::Exempt,
            anatomy: None,
            body: Some(body.clone()),
            ai: None,
            known_identities: vec![],
            velocity: None,
            asset: None,
        };
        assert!(definition.valid(&game));
        let actor = game
            .spawn_actor(at(1, 6, 3), std::num::NonZeroU64::new(100).unwrap())
            .unwrap();
        game.configure_prepared_actor_build(actor, &definition, Some(build))
            .unwrap();
        assert!(
            matches!(&game.actors[&actor].readied_ambush, ReadiedAmbush::Ready {body: captured, ..} if **captured == body)
        );
        assert_eq!(restore(&game), game);
        game.set_body(actor, crate::BodySpec::default()).unwrap();
        assert!(matches!(
            game.actors[&actor].readied_ambush,
            ReadiedAmbush::Spent
        ));
        assert_eq!(restore(&game), game);
    }

    #[test]
    fn touching_an_invalidated_ready_source_remembers_provocation_without_reveal() {
        let mut game = ready_fixture();
        let mut body = crate::BodySpec::default();
        body.mass += 1;
        game.set_body(SUBJECT, body).unwrap();
        assert!(game.actors[&SUBJECT].guise.appearance().is_some());
        assert!(matches!(
            game.actors[&SUBJECT].readied_ambush,
            ReadiedAmbush::Spent
        ));
        let next_turn = game.actors[&SUBJECT].ready_at;
        game.touch_readied_ambush(SUBJECT, OWNER).unwrap();
        assert!(game.actors[&SUBJECT].guise.appearance().is_some());
        assert_eq!(game.actors[&SUBJECT].readied_ambush.toucher(), Some(OWNER));
        assert!(game.preparation(SUBJECT).is_none());
        assert_eq!(game.actors[&SUBJECT].ready_at, next_turn);
        assert_eq!(released(&game), 0);
        assert_eq!(restore(&game), game);
    }

    #[test]
    fn incapable_touch_keeps_guise_and_saved_provocation_until_a_legal_turn() {
        let mut game = fixture_with(
            &[],
            &[
                guise(),
                ambush(),
                PerceptionValue::ReadiedTouchAmbush { enabled: true },
            ],
            |document, _, _| {
                document.conditions.push(serde_json::from_value(serde_json::json!({
                    "id":"fixture_incapable", "family":"fixture_incapable", "name":"Incapable fixture",
                    "lifecycle":"timed", "timing":{"kind":"actor_active","on_expiry":{"kind":"remove"}},
                    "restrictions":["voluntary_actions"], "preparation":"preserve", "modifiers":[],
                    "stacking":{"kind":"keep"}, "source_scoped":false, "descriptors":[], "capacity":"reject"
                })).unwrap());
            },
        );
        game.configure_ai(SUBJECT, crate::ai::AiProfile::default())
            .unwrap();
        assert_eq!(restore(&game), game, "valid independent capable baseline");
        apply(&mut game, SUBJECT, "fixture_incapable", 1000);
        game.touch_readied_ambush(SUBJECT, OWNER).unwrap();
        assert!(game.actors[&SUBJECT].guise.appearance().is_some());
        assert_eq!(game.choose_ai(SUBJECT).unwrap().action, Action::Wait);
        assert_eq!(restore(&game), game);
        // Expiry must pass through the ordinary decision/timer boundary. A
        // direct tick jump leaves the observer's due decision in the past and
        // is correctly rejected by strict save validation.
        for _ in 0..10 {
            assert_eq!(game.next_actor(), Some(OWNER));
            assert_eq!(game.actors[&OWNER].ready_at, game.tick());
            game.act(OWNER, Action::Wait).unwrap();
        }
        assert_eq!(game.tick(), 1000);
        let next = game.next_actor().expect("expiry leaves a legal decision");
        assert_eq!(game.actors[&next].ready_at, game.tick());
        assert!(game.condition_allows_voluntary_input(SUBJECT));
        assert_eq!(restore(&game), game, "valid ordinary expiry boundary");
        let plan = game
            .choose_ai(SUBJECT)
            .unwrap()
            .plan
            .expect("real contact permits ordinary attack preparation after waking");
        assert!(plan.preparation() > 0);
        assert!(game.actors[&SUBJECT].guise.appearance().is_some());
        game.teleport(OWNER, at(1, 7, 2)).unwrap();
        assert_eq!(game.choose_ai(SUBJECT).unwrap().action, Action::Wait);
        assert_eq!(restore(&game), game);
        game.teleport(OWNER, at(1, 2, 2)).unwrap();
        assert!(game.choose_ai(SUBJECT).unwrap().plan.is_some());
    }

    #[test]
    fn authored_ready_policy_rejects_a_costly_basic_attack() {
        let game = ready_fixture();
        let old = game.creature(SUBJECT).unwrap().build().clone();
        let mut document = old.ruleset().document().clone();
        document
            .abilities
            .iter_mut()
            .find(|ability| ability.id == id("basic_melee"))
            .unwrap()
            .cost = crate::test_creatures::paid_melee().cost;
        let content = crate::creatures::Content::new(
            crate::rules::Ruleset::validate(document).unwrap(),
            old.content().catalog().clone(),
        )
        .unwrap();
        let changed = crate::creatures::CreatureBuild::new(
            content,
            old.species_id().clone(),
            old.ledger().clone(),
            old.binding(),
            [
                crate::attributes::Skill::Deception,
                crate::attributes::Skill::Persuasion,
            ],
        );
        assert!(changed.and_then(|build| build.derive()).is_err());
    }

    #[test]
    fn source_ready_record_rejects_a_forged_placement_witness() {
        let game = ready_fixture();
        let mut shared = crate::checkpoint::SharedState::default();
        let snapshot = game.checkpoint(&mut shared);
        let pool = serde_json::to_value(&snapshot).unwrap()["actors"]
            .as_u64()
            .unwrap() as usize;
        let mut json = serde_json::to_value(&shared).unwrap();
        json["actors"][pool][SUBJECT.0.to_string()]["readied_ambush"]["at"]["location"]
            ["position"]["x"] = 6.into();
        let shared = serde_json::from_value(json).unwrap();
        assert!(Game::restore_checkpoint(snapshot, &shared, game.content().clone()).is_err());
    }
}
