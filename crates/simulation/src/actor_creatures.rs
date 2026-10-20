//! Creature-owned actor lifecycle and strict checkpoint adapter.
use crate::creatures::{CreatureBuild, CreatureState, RebuildOutcome};
use crate::{ActorId, Game, GameError};
use serde::{Deserialize, Serialize};
use tor_world::Shared;

/// Trusted backend inspection of one loaded creature. This is deliberately not
/// serializable: callers authorize recipients and map independent wire DTOs.
/// Borrowed definitions, choices and derived sources all belong to this state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreatureInspection<'a> {
    pub identity: CreatureIdentity,
    pub creature: std::borrow::Cow<'a, CreatureState>,
    pub stats: crate::creatures::OwnStats,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreatureIdentity {
    #[serde(deserialize_with = "read_name")]
    pub name: String,
    #[serde(deserialize_with = "read_faction")]
    pub faction: String,
}

fn read_text<'de, D: serde::Deserializer<'de>, const MAXIMUM: usize>(
    deserializer: D,
) -> Result<String, D::Error> {
    struct Text<const MAXIMUM: usize>;
    impl<const MAXIMUM: usize> serde::de::Visitor<'_> for Text<MAXIMUM> {
        type Value = String;
        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            write!(
                formatter,
                "1..{MAXIMUM} bytes of text without control characters"
            )
        }
        fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<String, E> {
            if value.is_empty() || value.len() > MAXIMUM || value.chars().any(char::is_control) {
                return Err(E::custom("invalid creature identity text"));
            }
            Ok(value.to_owned())
        }
    }
    deserializer.deserialize_str(Text::<MAXIMUM>)
}
fn read_name<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<String, D::Error> {
    read_text::<D, { CreatureIdentity::MAX_NAME_BYTES }>(deserializer)
}
fn read_faction<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<String, D::Error> {
    read_text::<D, { CreatureIdentity::MAX_FACTION_BYTES }>(deserializer)
}
impl CreatureIdentity {
    /// Maximum UTF-8 byte length accepted by authoring, spawning and checkpoints.
    pub const MAX_NAME_BYTES: usize = 60;
    /// Maximum UTF-8 byte length of a faction label.
    pub const MAX_FACTION_BYTES: usize = 80;

    pub(crate) fn valid(&self) -> bool {
        valid_identity(&self.name, &self.faction)
    }
}
fn valid_identity(name: &str, faction: &str) -> bool {
    !name.is_empty()
        && name.len() <= CreatureIdentity::MAX_NAME_BYTES
        && !name.chars().any(char::is_control)
        && !faction.is_empty()
        && faction.len() <= CreatureIdentity::MAX_FACTION_BYTES
        && !faction.chars().any(char::is_control)
}

impl crate::Actor {
    /// Discard progress and release its unpaid reservation. Start payments
    /// remain spent; paused/resumed work retains its original hold owner.
    pub(crate) fn cancel_preparation(&mut self) -> Option<crate::combat::Preparation> {
        let preparation = self.pending.take()?;
        if let Some(owner) = preparation.hold {
            if let Some(combat) = self.combat.as_mut() {
                combat.creature.cancel_cost(owner);
            }
        }
        Some(preparation)
    }
    pub(crate) fn complete_preparation(
        &mut self,
    ) -> Result<Option<crate::combat::Preparation>, GameError> {
        let Some(preparation) = self.pending.as_ref() else {
            return Ok(None);
        };
        if let Some(owner) = preparation.hold {
            self.combat
                .as_mut()
                .ok_or(GameError::UnknownActor)?
                .creature
                .finish_cost(owner)
                .map_err(|_| GameError::Rule(crate::abilities::RuleError::Unfunded))?;
        }
        Ok(self.pending.take())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CombatState {
    pub identity: CreatureIdentity,
    creature: Shared<CreatureState>,
}

impl CombatState {
    pub(crate) fn from_creature(identity: CreatureIdentity, creature: CreatureState) -> Self {
        Self {
            identity,
            creature: Shared::new(creature),
        }
    }
    pub fn creature(&self) -> &CreatureState {
        &self.creature
    }
    pub fn advance_active(&mut self, ticks: u64) -> crate::creatures::ActiveTimeOutcome {
        self.creature.advance_active(ticks)
    }
    pub(crate) fn advance_active_with_endpoint_context(
        &mut self,
        ticks: u64,
        context: Option<bool>,
    ) -> crate::creatures::ActiveTimeOutcome {
        self.creature
            .advance_active_with_endpoint_context(ticks, context)
    }
    pub(crate) fn start_preparation_cost(
        &mut self,
        owner: crate::IntentionId,
        cost: crate::costs::CostBundle,
    ) -> Result<crate::costs::StartCost, GameError> {
        self.creature
            .start_cost(owner, cost)
            .map_err(|_| GameError::Rule(crate::abilities::RuleError::Unfunded))
    }
    pub(crate) fn start_preparation_cooldown(
        &mut self,
        ability: &crate::content_ids::AbilityId,
        duration: u64,
    ) -> Result<(), GameError> {
        self.creature
            .start_cooldown(ability, duration)
            .map_err(|_| GameError::Rule(crate::abilities::RuleError::CoolingDown))
    }
    pub(crate) fn with_recovery_context<R, E>(
        &mut self,
        context: Option<crate::health_recovery::RecoveryContext>,
        apply: impl FnOnce(&mut CreatureState) -> Result<R, E>,
    ) -> Result<R, E> {
        match context {
            Some(sensed) => self.creature.with_recovery_context(sensed, apply),
            None => apply(&mut self.creature),
        }
    }
    pub fn apply_condition(
        &mut self,
        condition: &crate::content_ids::ConditionId,
        source: ActorId,
        duration: u64,
        context: Option<crate::health_recovery::RecoveryContext>,
    ) -> Result<crate::conditions::ConditionUpdate, GameError> {
        self.with_recovery_context(context, |creature| {
            creature.apply_condition(condition, source, duration)
        })
        .map_err(|error| {
            GameError::Rule(match error {
                crate::conditions::ConditionError::Capacity => {
                    crate::abilities::RuleError::ConditionCapacity
                }
                _ => crate::abilities::RuleError::InvalidDefinition,
            })
        })
    }
    pub fn apply_persistent_condition(
        &mut self,
        condition: &crate::content_ids::ConditionId,
        source: ActorId,
        context: Option<crate::health_recovery::RecoveryContext>,
    ) -> Result<crate::conditions::ConditionUpdate, GameError> {
        self.with_recovery_context(context, |creature| {
            creature.apply_persistent_condition(condition, source)
        })
        .map_err(|error| {
            GameError::Rule(match error {
                crate::conditions::ConditionError::Capacity => {
                    crate::abilities::RuleError::ConditionCapacity
                }
                _ => crate::abilities::RuleError::InvalidDefinition,
            })
        })
    }
    pub(crate) fn reserve_tokens(
        &mut self,
        tokens: &[crate::conditions::TokenReservation],
        owner: ActorId,
    ) -> Result<(), GameError> {
        self.creature
            .reserve_tokens(tokens, owner)
            .map_err(|_| GameError::Rule(crate::abilities::RuleError::InvalidPlan))
    }
    pub(crate) fn consume_tokens(
        &mut self,
        tokens: &[crate::conditions::TokenReservation],
        owner: ActorId,
    ) {
        self.creature.consume_tokens(tokens, owner);
    }
    pub(crate) fn apply_effect(
        &mut self,
        source: ActorId,
        effect: &crate::abilities::ResolvedEffect,
        observed_plan: &mut Option<tor_world::Shared<crate::public_plan::PublicPlanObservation>>,
        placement: &mut crate::abilities::Placement,
        context: Option<crate::health_recovery::RecoveryContext>,
    ) -> Result<Option<crate::abilities::ResolvedEffect>, GameError> {
        effect
            .apply_to(
                &mut crate::abilities::ActorEffectState {
                    creature: &mut self.creature,
                    observed_plan: Some(observed_plan),
                    placement: Some(placement),
                    recovery_context: context,
                },
                source,
            )
            .map_err(GameError::from)
    }
    pub(crate) fn damage_activity(&mut self) {
        self.creature.damage_activity();
    }
    pub(crate) fn set_recovery_context(&mut self, sensed_hostile: bool) {
        self.creature.set_recovery_context(sensed_hostile);
    }
    pub(crate) fn retry_growth_after_fold(
        &mut self,
        before: &Self,
        context: impl FnMut(
            u64,
        ) -> Result<
            crate::health_recovery::RecoveryContext,
            crate::creatures::CreatureStateError,
        >,
    ) -> Result<RebuildOutcome, crate::creatures::CreatureStateError> {
        self.creature
            .retry_growth_after_fold(&before.creature, context)
    }
    pub(crate) fn stage_xp_award(
        &mut self,
        amount: u32,
    ) -> Result<(), crate::creatures::CreatureStateError> {
        self.creature.stage_xp_award(amount)
    }
    pub(crate) fn configure_nutrition(
        &mut self,
        requirement: crate::nutrition::NutritionRequirement,
    ) -> Result<(), crate::creatures::CreatureStateError> {
        self.creature.configure_nutrition(requirement)
    }
    pub(crate) fn set_recovery_nutrition(
        &mut self,
        nutrition: crate::health_recovery::RecoveryNutrition,
    ) -> Result<(), crate::health::HealthError> {
        self.creature.set_recovery_nutrition(nutrition)
    }
    pub fn hp(&self) -> u64 {
        self.creature.health().current()
    }
    pub fn valid(&self) -> bool {
        self.identity.valid()
    }
    pub(crate) fn identity(&self) -> CreatureIdentity {
        self.identity.clone()
    }
    pub(crate) fn set_equipment_in_context(
        &mut self,
        equipment: Vec<crate::creatures::EquipmentSource>,
        context: Option<crate::health_recovery::RecoveryContext>,
    ) -> Result<RebuildOutcome, crate::creatures::CreatureStateError> {
        self.with_recovery_context(context, |creature| creature.stage_equipment(equipment))
    }
    pub(crate) fn set_equipment(
        &mut self,
        equipment: Vec<crate::creatures::EquipmentSource>,
    ) -> Result<RebuildOutcome, crate::creatures::CreatureStateError> {
        self.creature.stage_equipment(equipment)
    }
    /// A discarded AI projection has no later actor publication boundary.
    pub(crate) fn project_equipment_immediately_in_context(
        &mut self,
        equipment: Vec<crate::creatures::EquipmentSource>,
        context: Option<crate::health_recovery::RecoveryContext>,
    ) -> Result<RebuildOutcome, crate::creatures::CreatureStateError> {
        self.with_recovery_context(context, |creature| creature.set_equipment(equipment))
    }
    pub fn restore_resource(&mut self, resource: crate::resources::Resource, amount: u32) -> u32 {
        self.creature.restore_resource(resource, amount)
    }
}

impl Game {
    /// Configure once, before play. Subsequent changes use injury-preserving rebuilds.
    pub fn configure_creature(
        &mut self,
        id: ActorId,
        identity: CreatureIdentity,
        build: CreatureBuild,
    ) -> Result<(), GameError> {
        let outcome = self.prepare_creature_configuration(id, identity, build)?;
        let prepared = self.prepare_actor_outcome(outcome)?;
        self.publish_actor_effect(prepared, false)
    }

    /// An enclosing birth/entry transaction owns rollback and final receipts.
    pub(crate) fn configure_creature_in_transaction(
        &mut self,
        id: ActorId,
        identity: CreatureIdentity,
        build: CreatureBuild,
    ) -> Result<(), GameError> {
        let outcome = self.prepare_creature_configuration(id, identity, build)?;
        self.apply_outcome(outcome)?;
        self.retire_settled_falls();
        self.reconcile_recovery_contexts()
    }

    fn prepare_creature_configuration(
        &self,
        id: ActorId,
        identity: CreatureIdentity,
        build: CreatureBuild,
    ) -> Result<crate::lifecycle::ActorOutcome, GameError> {
        if build.content().as_ref() != self.content.as_ref() {
            return Err(GameError::Rule(
                crate::abilities::RuleError::InvalidDefinition,
            ));
        }
        if !identity.valid() {
            return Err(GameError::Build(
                crate::creatures::BuildError::InvalidDefinition,
            ));
        }
        let creature = CreatureState::new(build).map_err(GameError::Creature)?;
        if creature.health().dead() {
            return Err(GameError::Build(
                crate::creatures::BuildError::InvalidDefinition,
            ));
        }
        let active_since = self.actor_clock(id);
        let mut outcome = self.actor_outcome(id)?;
        let actor = &mut outcome.state;
        if actor.combat.is_some() || !actor.equipment.is_empty() {
            return Err(GameError::ActorBusy);
        }
        actor.active_since = active_since;
        actor.anatomy = Shared::new(creature.derived().anatomy.clone());
        actor.combat = Some(CombatState::from_creature(identity, creature));
        actor.cancel_preparation();
        Ok(outcome)
    }
    /// Project one loaded creature without committing time, preparation or RNG.
    /// Expired contextual memory may consult current perception and warm derived
    /// caches. Authority belongs to the backend.
    pub fn inspect_creature(&self, id: ActorId) -> Option<CreatureInspection<'_>> {
        let identity = self.actors.get(&id)?.combat.as_ref()?.identity();
        let creature = self.creature(id)?;
        let stats = creature.own_stats();
        Some(CreatureInspection {
            identity,
            creature,
            stats,
        })
    }

    pub fn creature(&self, id: ActorId) -> Option<std::borrow::Cow<'_, CreatureState>> {
        let actor = self.actors.get(&id)?;
        let creature = actor.combat.as_ref()?.creature();
        let elapsed = self.actor_clock(id).saturating_sub(actor.active_since);
        if elapsed == 0 || !creature.needs_active_time() {
            Some(std::borrow::Cow::Borrowed(creature))
        } else {
            let outcome = self.projected_actor_outcome(id).ok()?;
            Some(std::borrow::Cow::Owned(
                outcome.state.combat?.creature().clone(),
            ))
        }
    }
    /// Body-changing transformations are outside this milestone. All supported
    /// transformations preserve sockets, inventory and injury, and run ordinary
    /// death cleanup if their derived maximum falls to the retained injury.
    pub fn rebuild_creature(
        &mut self,
        id: ActorId,
        build: CreatureBuild,
    ) -> Result<RebuildOutcome, GameError> {
        let (application, mut outcome) = self.prepare_creature_rebuild(id, build)?;
        let prepared = self.prepare_actor_outcome(application)?;
        outcome.merge(prepared.rebuild_outcome().clone());
        self.publish_actor_effect(prepared, false)?;
        Ok(outcome)
    }

    /// Build only an owned settled actor candidate. Preview and ordinary action
    /// execution share this validation without nesting a Game transaction.
    pub(crate) fn prepare_creature_rebuild(
        &self,
        id: ActorId,
        build: CreatureBuild,
    ) -> Result<(crate::lifecycle::ActorOutcome, RebuildOutcome), GameError> {
        if build.content().as_ref() != self.content.as_ref() {
            return Err(GameError::Rule(
                crate::abilities::RuleError::InvalidDefinition,
            ));
        }
        let mut application = self.settled_actor_outcome(id)?;
        let actor = &mut application.state;
        let combat = actor.combat.as_ref().ok_or(GameError::Rule(
            crate::abilities::RuleError::InvalidDefinition,
        ))?;
        let mut candidate = combat.creature().clone();
        let mut context_error = None;
        let outcome = candidate
            .stage_rebuild_with_recovery_context(build, || {
                self.recovery_source_context(id).map_err(|error| {
                    context_error = Some(error);
                    crate::creatures::CreatureStateError::Inconsistent
                })
            })
            .map_err(|error| context_error.unwrap_or(GameError::Creature(error)))?;
        // Judge paid grants only after the common finalizer's growth fold.
        if candidate.derived().anatomy != *actor.anatomy {
            return Err(GameError::Build(
                crate::creatures::BuildError::InvalidDefinition,
            ));
        }
        actor.combat = Some(CombatState::from_creature(combat.identity(), candidate));
        Ok((application, outcome))
    }
}

#[cfg(test)]
mod preparation_cost_tests {
    use super::*;
    use crate::attributes::ManaBinding;
    use crate::content_ids::{AbilityId, ClassId};
    use crate::progression::{HdLedger, HdSource};
    use crate::resources::Resource;
    use crate::{Action, IntentionOrigin};
    use std::num::NonZeroU64;
    use tor_world::{Location, Position, RegionId};

    fn location(x: i32) -> Location {
        Location {
            region: RegionId(1),
            position: Position { x, y: 1, z: 0 },
        }
    }

    // Costs are authored on the selected ability and retained by preparation.
    fn charged_game() -> (Game, ActorId, ActorId, crate::IntentionId) {
        charged_game_with_seed(42)
    }

    fn charged_game_with_seed(seed: u64) -> (Game, ActorId, ActorId, crate::IntentionId) {
        let species = crate::test_creatures::unmodified_species("cost_subject");
        let mut document = crate::test_support::test_ruleset().document().clone();
        let index = document
            .abilities
            .iter()
            .position(|ability| ability.id.as_str() == "basic_melee")
            .unwrap();
        let mut free = document.abilities[index].clone();
        free.id = AbilityId::new("free_strike").unwrap();
        free.basic_attack = false;
        free.name = "Free strike".into();
        document.constants.default_abilities.push(free.id.clone());
        document.abilities.push(free);
        document.abilities[index].cost = Some(crate::rules::Cost {
            resource: Resource::Stamina,
            start: 1,
            resolution: 1,
        });
        let rules = crate::rules::Ruleset::validate(document).unwrap();
        let id = species.id.clone();
        let content = crate::creatures::Content::new(
            rules,
            crate::creatures::ScenarioCatalog {
                species: std::collections::BTreeMap::from([(id.clone(), species)]),
                templates: Default::default(),
            },
        )
        .unwrap();
        let build = CreatureBuild::new(
            content,
            id,
            HdLedger::seeded(vec![HdSource::Class(ClassId::new("warrior").unwrap())], 7).unwrap(),
            ManaBinding::Intellect,
            [
                tor_simulation::attributes::Skill::Deception,
                tor_simulation::attributes::Skill::Persuasion,
            ],
        )
        .unwrap();
        let mut game = Game::two_room_in_stone(seed, build.content().clone());
        let actor = game
            .spawn_actor(location(1), NonZeroU64::new(100).unwrap())
            .unwrap();
        let target = game
            .spawn_actor(location(2), NonZeroU64::new(10).unwrap())
            .unwrap();
        for id in [actor, target] {
            game.configure_creature(
                id,
                CreatureIdentity {
                    name: "subject".into(),
                    faction: "neutral".into(),
                },
                build.clone(),
            )
            .unwrap();
        }
        let intention = game
            .admit_intention(
                actor,
                Action::UseAbility {
                    id: AbilityId::new("basic_melee").unwrap(),
                    invocation: crate::abilities::AbilityInvocation::actor_target(target),
                    supplies: Default::default(),
                },
                IntentionOrigin::Human,
            )
            .unwrap();
        game.execute_next_intention().unwrap().outcome.unwrap();
        (game, actor, target, intention)
    }

    #[test]
    fn cancellation_releases_only_the_unpaid_preparation_cost() {
        for teleport in [false, true] {
            let (mut game, actor, _, intention) = charged_game();
            let before = game.clone();
            if teleport {
                game.teleport(actor, location(3)).unwrap();
            } else {
                game.cancel_intention(actor, intention).unwrap();
            }
            let creature = game.creature(actor).unwrap();
            let costs = creature.costs();
            assert!(costs.reservations().is_empty());
            assert_eq!(costs.resources().balance(Resource::Stamina), 1);
            assert_eq!(costs.available(Resource::Stamina), 1);
            assert!(game.preparation(actor).is_none());
            assert_eq!(game.tick(), before.tick());
            assert_eq!(
                before
                    .creature(actor)
                    .unwrap()
                    .costs()
                    .available(Resource::Stamina),
                0,
                "retained snapshots preserve the hold"
            );
        }
    }

    #[test]
    fn completion_pays_the_original_owner_once_after_a_new_resume_admission() {
        let (mut game, actor, target, original) = charged_game();
        game.pause_preparation(actor).unwrap();
        let resumed = game
            .admit_intention(
                actor,
                Action::UseAbility {
                    id: AbilityId::new("basic_melee").unwrap(),
                    invocation: crate::abilities::AbilityInvocation::actor_target(target),
                    supplies: Default::default(),
                },
                IntentionOrigin::Human,
            )
            .unwrap();
        assert_ne!(resumed, original);
        game.execute_next_intention().unwrap().outcome.unwrap();
        assert_eq!(game.preparation(actor).unwrap().hold, Some(original));
        assert_eq!(
            game.creature(actor)
                .unwrap()
                .costs()
                .resources()
                .balance(Resource::Stamina),
            1
        );
        for _ in 0..6 {
            game.act(target, Action::Wait).unwrap();
        }
        assert_eq!(game.tick(), 60);
        let creature = game.creature(actor).unwrap();
        let costs = creature.costs();
        assert!(costs.reservations().is_empty());
        assert_eq!(costs.resources().balance(Resource::Stamina), 0);
        assert!(game.preparation(actor).is_none());
    }

    #[test]
    fn a_missed_attack_still_pays_the_resolution_cost() {
        let seed = (0u64..32)
            .find(|&seed| {
                let mut rng = seed;
                crate::dice::roll_check(&mut rng, crate::dice::Edge::default()).kept < 10
            })
            .unwrap();
        let (mut game, actor, target, _) = charged_game_with_seed(seed);
        let target_health = game.health(target);
        for _ in 0..6 {
            game.act(target, Action::Wait).unwrap();
        }
        assert_eq!(game.health(target), target_health);
        assert!(game.event_log.iter().any(|event| matches!(event,
            crate::combat::CombatEvent::Resolved { outcome, .. } if outcome.result == crate::abilities::OutcomeStatus::Miss)));
        let creature = game.creature(actor).unwrap();
        let costs = creature.costs();
        assert!(costs.reservations().is_empty());
        assert_eq!(costs.resources().balance(Resource::Stamina), 0);
    }

    #[test]
    fn replacing_preparation_with_new_work_releases_its_original_hold() {
        for moving in [false, true] {
            let (mut game, actor, _, original) = charged_game();
            game.pause_preparation(actor).unwrap();
            if moving {
                game.act(actor, Action::Move(tor_world::Direction::South))
                    .unwrap();
                assert!(game.preparation(actor).is_none());
            } else {
                let mut at = location(1);
                at.position.y = 2;
                let target = game.spawn_actor(at, NonZeroU64::new(100).unwrap()).unwrap();
                game.configure_creature(
                    target,
                    CreatureIdentity {
                        name: "other".into(),
                        faction: "neutral".into(),
                    },
                    game.creature(actor).unwrap().build().clone(),
                )
                .unwrap();
                let replacement = game
                    .admit_intention(
                        actor,
                        Action::UseAbility {
                            id: AbilityId::new("free_strike").unwrap(),
                            invocation: crate::abilities::AbilityInvocation::actor_target(target),
                            supplies: Default::default(),
                        },
                        IntentionOrigin::Human,
                    )
                    .unwrap();
                assert_ne!(replacement, original);
                assert!(
                    game.creature(actor)
                        .unwrap()
                        .costs()
                        .reservation(original)
                        .is_some(),
                    "queueing does not change payment"
                );
                game.execute_next_intention().unwrap().outcome.unwrap();
                assert_eq!(game.preparation(actor).unwrap().hold, None);
            }
            let creature = game.creature(actor).unwrap();
            let costs = creature.costs();
            assert!(costs.reservations().is_empty());
            assert_eq!(costs.available(Resource::Stamina), 1);
        }
    }

    #[test]
    fn losing_target_reach_releases_the_preparation_hold() {
        let (mut game, actor, target, _) = charged_game();
        game.teleport(target, location(4)).unwrap();
        game.act(target, Action::Wait).unwrap();
        assert!(game.preparation(actor).is_none());
        let creature = game.creature(actor).unwrap();
        let costs = creature.costs();
        assert!(costs.reservations().is_empty());
        assert_eq!(costs.available(Resource::Stamina), 1);
    }
}
