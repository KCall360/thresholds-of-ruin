//! Frozen generic Escape work. Relationship endpoints remain private to simulation.
use crate::abilities::{CheckPhase, CheckSide, CheckThreshold, Plan, PlannedCheck, TokenClaim};
use crate::linked_hold::{HoldId, HoldPolicy};
use crate::{ActorId, Game, GameError, IntentionId};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EscapePlan {
    pub actor: ActorId,
    pub holder: ActorId,
    pub hold: HoldId,
    pub policy: HoldPolicy,
    pub check: PlannedCheck,
    pub tokens: Vec<TokenClaim>,
    pub preparation: u64,
    pub recovery: u64,
}
fn read_claims<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<TokenClaim>, D::Error> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Claim {
        reservation: crate::conditions::TokenReservation,
        #[serde(deserialize_with = "crate::bounded::named_map")]
        site: crate::abilities::CheckSite,
    }
    struct NamedClaim(Claim);
    impl<'de> Deserialize<'de> for NamedClaim {
        fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
            crate::bounded::named_map(d).map(Self)
        }
    }
    Ok(
        crate::bounded::Bounded::<NamedClaim, { 2 * crate::limits::CONDITIONS }>::deserialize(d)?
            .0
            .into_iter()
            .map(|claim| TokenClaim {
                reservation: claim.0.reservation,
                site: claim.0.site,
            })
            .collect(),
    )
}
impl<'de> Deserialize<'de> for EscapePlan {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            actor: ActorId,
            holder: ActorId,
            hold: HoldId,
            policy: HoldPolicy,
            check: PlannedCheck,
            #[serde(deserialize_with = "read_claims")]
            tokens: Vec<TokenClaim>,
            preparation: u64,
            recovery: u64,
        }
        let f: Fields = crate::bounded::named_map(d)?;
        Ok(Self {
            actor: f.actor,
            holder: f.holder,
            hold: f.hold,
            policy: f.policy,
            check: f.check,
            tokens: f.tokens,
            preparation: f.preparation,
            recovery: f.recovery,
        })
    }
}
impl EscapePlan {
    pub(crate) fn valid(&self, rules: &crate::rules::Ruleset) -> bool {
        let valid_actor = |id: ActorId| id.0 > 0 && id.0 < u64::MAX;
        let numeric = |n: i64| n.unsigned_abs() <= crate::limits::NUMERIC as u64;
        let CheckThreshold::Opposed { bonus, edge } = self.check.threshold else {
            return false;
        };
        valid_actor(self.actor)
            && valid_actor(self.holder)
            && self.actor != self.holder
            && self.hold.0 > 0
            && self.hold.0 < u64::MAX
            && self.policy.valid()
            && rules.flag(&self.policy.sustaining).is_some()
            && (1..=crate::limits::NUMERIC as u64).contains(&self.preparation)
            && (1..=crate::limits::NUMERIC as u64).contains(&self.recovery)
            && self
                .preparation
                .checked_add(self.recovery)
                .is_some_and(|v| v <= crate::limits::NUMERIC as u64)
            && self.check.actor == crate::rules::CheckActor::User
            && self.check.target_participates
            && numeric(self.check.bonus)
            && numeric(self.check.edge)
            && numeric(bonus)
            && numeric(edge)
            && self.tokens.len() <= 2 * crate::limits::CONDITIONS
            && self
                .tokens
                .iter()
                .map(|claim| &claim.reservation)
                .collect::<BTreeSet<_>>()
                .len()
                == self.tokens.len()
            && self.tokens.iter().all(|claim| {
                let token = &claim.reservation;
                if claim.site.strike != 0
                    || !valid_actor(token.source)
                    || token.qualification.is_some()
                {
                    return false;
                }
                let Some(condition) = rules.condition(&token.condition) else {
                    return false;
                };
                let (roller, opponent) = match claim.site.check {
                    CheckPhase::Primary {
                        side: CheckSide::Initiating,
                    } => (self.actor, self.holder),
                    CheckPhase::Primary {
                        side: CheckSide::Opposing,
                    } => (self.holder, self.actor),
                    CheckPhase::Rider { .. } => return false,
                };
                match condition.lifecycle {
                    crate::rules::ConditionLifecycle::NextCheck => token.holder == roller,
                    crate::rules::ConditionLifecycle::NextCheckAgainst => token.holder == opponent,
                    crate::rules::ConditionLifecycle::Timed => false,
                }
            })
    }
}
impl Game {
    pub(crate) fn escape_plan(&self, actor: ActorId, hold: HoldId) -> Result<Plan, GameError> {
        let link = self
            .linked_hold(actor, hold)
            .ok_or(GameError::Rule(crate::abilities::RuleError::OutOfReach))?;
        if !self.valid_linked_hold(link.holder, actor, hold)
            || self.actor_frozen(actor)
            || self.actor_frozen(link.holder)
        {
            return Err(GameError::Rule(crate::abilities::RuleError::OutOfReach));
        }
        let creature = self.creature(actor).ok_or(GameError::UnknownActor)?;
        let speed = Some(
            creature
                .derived()
                .attributes
                .get(crate::attributes::Attribute::Speed),
        );
        let rules = self.content.rules();
        let (check, tokens) = self.compile_escape_check(actor, link.holder)?;
        let plan = EscapePlan {
            actor,
            holder: link.holder,
            hold,
            policy: link.policy.clone(),
            check,
            tokens,
            preparation: creature
                .derived()
                .action_duration
                .apply_bounded(
                    crate::creatures::scaled_duration(link.policy.preparation, speed, rules)
                        .map_err(GameError::Build)?,
                )
                .map_err(GameError::Build)?,
            recovery: creature
                .derived()
                .action_duration
                .apply_bounded(
                    crate::creatures::scaled_duration(link.policy.recovery, speed, rules)
                        .map_err(GameError::Build)?,
                )
                .map_err(GameError::Build)?,
        };
        if !plan.valid(rules) || !self.escape_plan_source_valid(&plan) {
            return Err(GameError::Rule(crate::abilities::RuleError::InvalidPlan));
        }
        Ok(Plan::Escape(tor_world::Shared::new(plan)))
    }
    pub(crate) fn escape_plan_source_valid(&self, plan: &EscapePlan) -> bool {
        plan.hold.0 < self.next_hold_id
            && plan.actor.0 < self.next_actor_id
            && plan.holder.0 < self.next_actor_id
            && self
                .linked_hold(plan.actor, plan.hold)
                .is_none_or(|link| link.holder == plan.holder && link.policy == plan.policy)
            && plan
                .tokens
                .iter()
                .all(|claim| claim.reservation.source.0 < self.next_actor_id)
    }
    pub(crate) fn escape_plan_available(&self, plan: &EscapePlan) -> bool {
        !self.actor_frozen(plan.actor)
            && !self.actor_frozen(plan.holder)
            && self.valid_linked_hold(plan.holder, plan.actor, plan.hold)
            && self
                .linked_hold(plan.actor, plan.hold)
                .is_some_and(|link| link.policy == plan.policy)
    }
    pub(crate) fn complete_escape(
        &mut self,
        preparation: crate::combat::Preparation,
        due: u64,
    ) -> Result<(), GameError> {
        let Plan::Escape(plan) = &preparation.plan else {
            return Err(GameError::Rule(crate::abilities::RuleError::InvalidPlan));
        };
        // The scheduler has settled both endpoints. Exact identity and policy
        // validation occurs before the shared dice kernel or any completion work.
        if !self.escape_plan_available(plan) {
            let mut abandoned = self.actor_outcome(plan.actor)?;
            abandoned.interrupt(crate::lifecycle::Interruption::Discard, self.tick)?;
            self.apply_outcome(abandoned)?;
            return Ok(());
        }
        let mut rng = self.rules_rng;
        let success = crate::abilities::resolve_escape_check(&plan.check, &mut rng);
        if success && !self.escape_linked_hold(plan.actor, plan.hold)? {
            return Err(GameError::Rule(crate::abilities::RuleError::InvalidPlan));
        }
        // Build completion after releasing the exact link: a stale actor clone
        // must never restore its old incoming slot or another holder's mirror.
        let mut completion = self.actor_outcome(plan.actor)?;
        if completion.state.pending.as_ref() != Some(&preparation) {
            return Err(GameError::Rule(crate::abilities::RuleError::InvalidPlan));
        }
        completion.state.complete_preparation()?;
        completion.state.ready_at = due
            .checked_add(plan.recovery)
            .ok_or(GameError::Schedule(crate::ScheduleError::TimeExhausted))?;
        self.apply_outcome(completion)?;
        self.rules_rng = rng;
        self.push_event(
            crate::combat::CombatEvent::EscapeResolved {
                actor: plan.actor,
                hold: plan.hold,
                success,
                intention: preparation.intention,
            },
            None,
        )?;
        Ok(())
    }
}
pub(crate) fn read_action<'de, D: serde::Deserializer<'de>>(d: D) -> Result<HoldId, D::Error> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Fields {
        hold: HoldId,
    }
    let f: Fields = crate::bounded::named_map(d)?;
    Ok(f.hold)
}
pub(crate) fn read_completion<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<(ActorId, HoldId, bool, Option<IntentionId>), D::Error> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Fields {
        actor: ActorId,
        hold: HoldId,
        success: bool,
        #[serde(deserialize_with = "crate::bounded::required_option")]
        intention: Option<IntentionId>,
    }
    let f: Fields = crate::bounded::named_map(d)?;
    Ok((f.actor, f.hold, f.success, f.intention))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Action, CreatureIdentity, OutcomeKind};
    use std::num::NonZeroU64;
    use tor_world::{Extent, Location, Position, Region, RegionId, World};

    #[test]
    fn escape_action_requires_exact_named_inner_and_outer_objects() {
        let canonical = serde_json::json!({"type": "escape", "value": {"hold": 7}});
        let action = serde_json::from_value::<Action>(canonical.clone()).unwrap();
        assert_eq!(action, Action::Escape { hold: HoldId(7) });
        assert_eq!(serde_json::to_value(action).unwrap(), canonical);
        for malformed in [
            serde_json::json!(["escape", {"hold": 7}]),
            serde_json::json!({"type": "escape", "value": [7]}),
            serde_json::json!({"type": "escape", "value": {}}),
            serde_json::json!({"type": "escape", "value": {"hold": 7, "holder": 2}}),
            serde_json::json!({"type": "escape", "value": {"hold": 0}}),
            serde_json::json!({"type": "escape", "value": {"hold": u64::MAX}}),
        ] {
            assert!(
                serde_json::from_value::<Action>(malformed.clone()).is_err(),
                "{malformed}"
            );
        }
    }

    fn policy() -> HoldPolicy {
        HoldPolicy {
            sustaining: crate::content_ids::FlagId::new("magical").unwrap(),
            preparation: 100,
            recovery: 100,
            protection: 400,
        }
    }
    fn fixture() -> (Game, HoldId, HoldId) {
        fixture_with_rate(crate::action_duration::ActionDuration::Normal)
    }
    fn fixture_with_rate(rate: crate::action_duration::ActionDuration) -> (Game, HoldId, HoldId) {
        let mut species = crate::test_creatures::species();
        species.modifiers.push(crate::rules::Modifier {
            scope: crate::rules::Scope::Always,
            op: crate::rules::ModifierOp::ActionDuration { value: rate },
        });
        species.modifiers.push(crate::rules::Modifier {
            scope: crate::rules::Scope::Always,
            op: crate::rules::ModifierOp::Flag {
                flag: policy().sustaining,
            },
        });
        for (stat, value) in [
            (
                crate::rules::Stat::Attribute {
                    attribute: crate::attributes::Attribute::Strength,
                },
                3,
            ),
            (
                crate::rules::Stat::Attribute {
                    attribute: crate::attributes::Attribute::Speed,
                },
                5,
            ),
            (
                crate::rules::Stat::AbilityTag {
                    tag: crate::content_ids::AbilityTagId::new("attack").unwrap(),
                    stat: crate::rules::AbilityStat::CheckEdge,
                },
                9,
            ),
            (
                crate::rules::Stat::AbilityTag {
                    tag: crate::content_ids::AbilityTagId::new("attack").unwrap(),
                    stat: crate::rules::AbilityStat::CheckBonus,
                },
                80,
            ),
        ] {
            species.modifiers.push(crate::rules::Modifier {
                scope: crate::rules::Scope::Always,
                op: crate::rules::ModifierOp::Add { stat, value },
            });
        }
        let build = crate::test_creatures::build(species);
        let world = World::new(
            vec![Region {
                id: RegionId(1),
                name: "Escape fixture".into(),
                bounds: Extent::new(8, 4, 1).unwrap(),
            }],
            vec![],
        )
        .unwrap();
        let mut game = Game::new(world, 42, build.content().clone());
        game.set_gravity(RegionId(1), [0; 3]).unwrap();
        for x in [2, 1, 3] {
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
                CreatureIdentity {
                    name: format!("actor {x}"),
                    faction: "neutral".into(),
                },
                build.clone(),
            )
            .unwrap();
        }
        let first = game
            .apply_linked_hold(ActorId(2), ActorId(1), policy())
            .unwrap()
            .unwrap();
        let second = game
            .apply_linked_hold(ActorId(3), ActorId(1), policy())
            .unwrap()
            .unwrap();
        (game, first, second)
    }

    #[test]
    fn escape_scales_only_captured_action_phases_and_preserves_protection_clock() {
        let (normal, hold, _) = fixture();
        let baseline = normal.escape_plan(ActorId(1), hold).unwrap();
        for rate in [
            crate::action_duration::ActionDuration::Haste,
            crate::action_duration::ActionDuration::Slow,
        ] {
            let (game, hold, _) = fixture_with_rate(rate);
            let plan = game.escape_plan(ActorId(1), hold).unwrap();
            assert_eq!(
                plan.preparation(),
                rate.apply(baseline.preparation()).unwrap()
            );
            assert_eq!(plan.recovery(), rate.apply(baseline.recovery()).unwrap());
            let Plan::Escape(escape) = &plan else {
                unreachable!()
            };
            assert_eq!(escape.policy.protection, 400);
            assert_eq!(
                game.creature(ActorId(1)).unwrap().derived().attributes,
                normal.creature(ActorId(1)).unwrap().derived().attributes
            );
            let decoded: Plan =
                serde_json::from_value(serde_json::to_value(&plan).unwrap()).unwrap();
            assert_eq!(decoded, plan);
            assert!(decoded.valid(game.content.rules()));
        }
    }
    fn start(game: &mut Game, hold: HoldId, success: bool) -> EscapePlan {
        let Plan::Escape(plan) = game.escape_plan(ActorId(1), hold).unwrap() else {
            unreachable!()
        };
        let seed = (1..=1024)
            .find(|seed| {
                let mut rng = *seed;
                crate::abilities::resolve_escape_check(&plan.check, &mut rng) == success
            })
            .expect("both opposed outcomes are reachable");
        game.rules_rng = seed;
        let outcome = game.act(ActorId(1), Action::Escape { hold }).unwrap();
        assert_eq!(outcome.kind, OutcomeKind::EscapeStarted { hold });
        assert_eq!(
            game.preparation(ActorId(1)).unwrap().progress,
            crate::combat::PreparationProgress::Escape {}
        );
        (*plan).clone()
    }
    fn complete(game: &mut Game) {
        for _ in 0..20 {
            if game.preparation(ActorId(1)).is_none() {
                return;
            }
            game.act(game.next_actor().unwrap(), Action::Wait).unwrap();
        }
        panic!("Escape did not complete");
    }
    #[test]
    fn linked_hold_escape_exact_release_and_frozen_recovery() {
        let (mut game, first, second) = fixture();
        let plan = start(&mut game, first, true);
        game.apply_effects(
            ActorId(1),
            &[
                crate::rules::Effect::LoseAttribute {
                    attribute: crate::attributes::Attribute::Speed,
                    amount: crate::rules::AmountExpr::Fixed { amount: 3 },
                },
                crate::rules::Effect::LoseAttribute {
                    attribute: crate::attributes::Attribute::Strength,
                    amount: crate::rules::AmountExpr::Fixed { amount: 3 },
                },
            ],
        )
        .unwrap();
        assert_eq!(
            game.preparation(ActorId(1)).unwrap().plan,
            Plan::Escape(tor_world::Shared::new(plan.clone())),
            "current Speed/Strength cannot recompile paid operands"
        );
        assert!(game
            .preparation(ActorId(1))
            .unwrap()
            .plan
            .as_ability()
            .is_none());
        assert_eq!(
            game.preparation(ActorId(1)).unwrap().plan.work(),
            crate::Work::Escape { hold: first }
        );
        let mut shared = crate::checkpoint::SharedState::default();
        let saved = game.checkpoint(&mut shared);
        game = Game::restore_checkpoint(saved, &shared, game.content.clone()).unwrap();
        complete(&mut game);
        assert_eq!(
            game.linked_holds(ActorId(1)),
            vec![crate::linked_hold::LinkedHoldObservation {
                id: second,
                holder: ActorId(3)
            }]
        );
        assert_eq!(
            game.actors[&ActorId(1)].ready_at,
            plan.preparation + plan.recovery
        );
        assert!(game.condition_restricts(
            ActorId(1),
            crate::rules::ConditionRestriction::OrdinaryMovement
        ));
        assert!(game.actors[&ActorId(1)]
            .holds
            .incoming
            .iter()
            .any(|slot| matches!(
                slot,
                crate::linked_hold::HoldSlot::Protection {
                    holder: ActorId(2),
                    remaining
                } if *remaining + game.actors[&ActorId(1)].active_since == plan.preparation + 400
            )));
        assert!(game.event_log.iter().any(|event| matches!(event,
            crate::combat::CombatEvent::EscapeResolved { actor: ActorId(1), hold, success: true, .. } if *hold == first)));
        let own = game.observe(ActorId(1)).unwrap();
        assert!(own.combat.unwrap().events.iter().any(|event| matches!(event,
            crate::combat::DisclosedCombatEvent::EscapeResolved { hold, success: true } if *hold == first)));
        let other = game.observe(ActorId(2)).unwrap();
        assert!(!other.combat.unwrap().events.iter().any(|event| matches!(
            event,
            crate::combat::DisclosedCombatEvent::EscapeResolved { .. }
        )));
        let before = game.clone();
        game.resolve_preparations().unwrap();
        assert_eq!(game, before, "completion cannot resolve twice");
    }
    #[test]
    fn linked_hold_escape_failed_check_keeps_links_and_pays_recovery() {
        let (mut game, first, second) = fixture();
        let plan = start(&mut game, first, false);
        complete(&mut game);
        assert_eq!(
            game.linked_holds(ActorId(1))
                .iter()
                .map(|link| link.id)
                .collect::<Vec<_>>(),
            vec![first, second]
        );
        assert_eq!(
            game.actors[&ActorId(1)].ready_at,
            plan.preparation + plan.recovery
        );
        assert!(game.event_log.iter().any(|event| matches!(event,
            crate::combat::CombatEvent::EscapeResolved { hold, success: false, .. } if *hold == first)));
    }
    #[test]
    fn linked_hold_escape_stale_aba_abandons_without_rng_or_recovery_debt() {
        let (mut game, first, second) = fixture();
        start(&mut game, first, true);
        game.release_linked_hold(ActorId(1), first).unwrap();
        let replacement = game
            .apply_linked_hold(ActorId(2), ActorId(1), policy())
            .unwrap()
            .unwrap();
        assert_ne!(first, replacement);
        let rng = game.rules_rng;
        game.resolve_preparations().unwrap();
        assert_eq!(game.rules_rng, rng);
        assert!(game.preparation(ActorId(1)).is_none());
        assert_eq!(game.actors[&ActorId(1)].ready_at, game.tick);
        assert!(game.linked_hold(ActorId(1), replacement).is_some());
        assert!(game.linked_hold(ActorId(1), second).is_some());
        assert!(!game
            .event_log
            .iter()
            .any(|event| matches!(event, crate::combat::CombatEvent::EscapeResolved { .. })));
    }
    #[test]
    fn linked_hold_escape_generic_tokens_reserve_both_endpoints_and_retire_once() {
        let (mut game, first, _) = fixture();
        for actor in [ActorId(1), ActorId(2)] {
            for id in ["readied", "exposed"] {
                game.apply_condition(
                    actor,
                    &crate::content_ids::ConditionId::new(id).unwrap(),
                    ActorId(3),
                    1000,
                )
                .unwrap();
            }
        }
        let plan = start(&mut game, first, false);
        assert_eq!(plan.tokens.len(), 4);
        assert!(plan
            .tokens
            .iter()
            .all(|claim| claim.reservation.qualification.is_none()));
        assert!(game.token_reservations_valid(ActorId(1), false));
        assert!(game.token_reservations_valid(ActorId(2), false));
        complete(&mut game);
        for actor in [ActorId(1), ActorId(2)] {
            assert_eq!(
                game.creature(actor)
                    .unwrap()
                    .conditions()
                    .instances()
                    .count(),
                0
            );
        }
    }
    #[test]
    fn linked_hold_escape_strict_records_progress_and_issued_identity() {
        let (game, first, _) = fixture();
        let Plan::Escape(plan) = game.escape_plan(ActorId(1), first).unwrap() else {
            unreachable!()
        };
        let value = serde_json::to_value(&*plan).unwrap();
        let decode = |v: &serde_json::Value| {
            serde_json::from_slice::<EscapePlan>(&serde_json::to_vec(v).unwrap())
        };
        assert_eq!(decode(&value).unwrap(), *plan);
        for key in value.as_object().unwrap().keys() {
            let mut missing = value.clone();
            missing.as_object_mut().unwrap().remove(key);
            assert!(decode(&missing).is_err(), "missing {key}");
        }
        let mut unknown = value.clone();
        unknown["private_alias"] = serde_json::json!(1);
        assert!(decode(&unknown).is_err());
        let sequence =
            serde_json::Value::Array(value.as_object().unwrap().values().cloned().collect());
        assert!(decode(&sequence).is_err());
        for hold in [
            serde_json::json!(0),
            serde_json::json!(u64::MAX),
            serde_json::json!("1"),
            serde_json::json!({"id":1}),
        ] {
            let action = serde_json::json!({"type":"escape","value":{"hold":hold}});
            assert!(serde_json::from_value::<Action>(action).is_err());
        }
        assert!(serde_json::from_str::<Action>(r#"{"type":"escape","value":[1]}"#).is_err());
        assert!(serde_json::from_str::<crate::combat::PreparationProgress>(
            r#"{"kind":"escape","next_strike":0}"#
        )
        .is_err());
        let mut forged = (*plan).clone();
        forged.hold = HoldId(game.next_hold_id);
        assert!(!game.escape_plan_source_valid(&forged));
        let event = serde_json::json!({"EscapeResolved":{"actor":1,"hold":first.0,"success":true}});
        assert!(serde_json::from_value::<crate::combat::CombatEvent>(event).is_err());
    }
    #[test]
    fn linked_hold_escape_has_no_authored_ability_or_qualified_token_grants() {
        let (mut game, first, _) = fixture();
        let mut creature = game.creature(ActorId(1)).unwrap().into_owned();
        creature
            .apply_qualified_condition(
                &crate::content_ids::ConditionId::new("readied").unwrap(),
                ActorId(3),
                1000,
                Some(crate::conditions::TokenQualification {
                    opponent: ActorId(2),
                    ability_tag: crate::content_ids::AbilityTagId::new("attack").unwrap(),
                }),
            )
            .unwrap();
        let mut outcome = game.actor_outcome(ActorId(1)).unwrap();
        let identity = outcome.state.combat.as_ref().unwrap().identity.clone();
        outcome.state.combat = Some(crate::combat::CombatState::from_creature(
            identity, creature,
        ));
        game.apply_outcome(outcome).unwrap();
        let plan = start(&mut game, first, false);
        assert!(plan.tokens.is_empty());
        assert_eq!(plan.check.edge, 0);
        assert_eq!(
            plan.check.bonus,
            i64::from(
                game.creature(ActorId(1))
                    .unwrap()
                    .derived()
                    .attributes
                    .get(crate::attributes::Attribute::Strength)
            )
        );
        complete(&mut game);
        assert!(game
            .creature(ActorId(1))
            .unwrap()
            .conditions()
            .instances()
            .any(|condition| condition.qualification.is_some() && condition.reserved_by.is_none()));
    }
}
