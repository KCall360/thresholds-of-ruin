//! Species-owned acquisition policy and actor-owned, bounded personal engagement.
//! No controller state or public preparation may create knowledge of an attacker.
use crate::{ActorId, Game, GameError};
use serde::{Deserialize, Serialize};
use tor_world::Location;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Disposition {
    Passive,
    Mean,
}
impl<'de> Deserialize<'de> for Disposition {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        match String::deserialize(d)?.as_str() {
            "passive" => Ok(Self::Passive),
            "mean" => Ok(Self::Mean),
            _ => Err(serde::de::Error::custom("invalid Disposition")),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IdleBehavior {
    Wait,
    Explore,
}
impl<'de> Deserialize<'de> for IdleBehavior {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        match String::deserialize(d)?.as_str() {
            "wait" => Ok(Self::Wait),
            "explore" => Ok(Self::Explore),
            _ => Err(serde::de::Error::custom("invalid IdleBehavior")),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BehaviorPolicy {
    pub disposition: Disposition,
    pub idle: IdleBehavior,
    pub memory_ticks: u64,
    pub doors: bool,
    pub care: bool,
    pub equipment: bool,
    pub loot: bool,
    pub gold: bool,
    pub flee_percent: u32,
}
impl<'de> Deserialize<'de> for BehaviorPolicy {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            disposition: Disposition,
            idle: IdleBehavior,
            memory_ticks: u64,
            doors: bool,
            care: bool,
            equipment: bool,
            loot: bool,
            gold: bool,
            flee_percent: u32,
        }
        let fields: Fields = crate::bounded::named_map(d)?;
        let policy = Self {
            disposition: fields.disposition,
            idle: fields.idle,
            memory_ticks: fields.memory_ticks,
            doors: fields.doors,
            care: fields.care,
            equipment: fields.equipment,
            loot: fields.loot,
            gold: fields.gold,
            flee_percent: fields.flee_percent,
        };
        if !policy.valid() {
            return Err(serde::de::Error::custom("invalid behavior policy"));
        }
        Ok(policy)
    }
}
impl Default for BehaviorPolicy {
    fn default() -> Self {
        Self {
            disposition: Disposition::Passive,
            idle: IdleBehavior::Explore,
            memory_ticks: 1000,
            doors: false,
            care: false,
            equipment: false,
            loot: false,
            gold: false,
            flee_percent: 25,
        }
    }
}
impl BehaviorPolicy {
    pub fn valid(self) -> bool {
        self.memory_ticks <= crate::limits::NUMERIC as u64 && self.flee_percent <= 100
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Engagement {
    pub target: ActorId,
    pub location: Location,
    pub last_seen: u64,
}
impl<'de> Deserialize<'de> for Engagement {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            target: ActorId,
            #[serde(deserialize_with = "crate::public_plan::read_location")]
            location: Location,
            last_seen: u64,
        }
        let fields: Fields = crate::bounded::named_map(d)?;
        Ok(Self {
            target: fields.target,
            location: fields.location,
            last_seen: fields.last_seen,
        })
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Behavior {
    pub engagement: Option<Engagement>,
    /// SplitMix state: zero is valid and produces an ordinary first draw.
    pub rng: u64,
}
impl<'de> Deserialize<'de> for Behavior {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            #[serde(deserialize_with = "crate::bounded::required_option")]
            engagement: Option<Engagement>,
            rng: u64,
        }
        let fields: Fields = crate::bounded::named_map(d)?;
        Ok(Self {
            engagement: fields.engagement,
            rng: fields.rng,
        })
    }
}
impl Behavior {
    pub fn new(seed: u64, actor: ActorId) -> Self {
        Self {
            engagement: None,
            rng: seed.wrapping_add(actor.0.wrapping_sub(1)),
        }
    }
    pub(crate) fn validate_deadline(
        &self,
        policy: Option<BehaviorPolicy>,
    ) -> Result<(), GameError> {
        if self.engagement.is_some_and(|entry| {
            policy
                .and_then(|policy| entry.last_seen.checked_add(policy.memory_ticks))
                .is_none()
        }) {
            return Err(GameError::Schedule(crate::ScheduleError::TimeExhausted));
        }
        Ok(())
    }
    /// TOR adapts Rogue's 2/3 wake chance to one committed eligible AI decision.
    /// Candidate previews copy this state; only ordinary action publication saves it.
    pub fn wakes(&mut self) -> bool {
        !crate::dice::random_word(&mut self.rng).is_multiple_of(3)
    }
}
impl crate::Actor {
    pub(crate) fn behavior_policy(&self) -> Option<BehaviorPolicy> {
        self.combat
            .as_ref()
            .map(|combat| combat.creature().derived().behavior)
    }
    pub(crate) fn engagement_deadline(&self) -> Option<u64> {
        self.behavior
            .engagement?
            .last_seen
            .checked_add(self.behavior_policy()?.memory_ticks)
    }
    pub(crate) fn engagement_at(&self, clock: u64) -> Option<Engagement> {
        self.engagement_deadline()
            .filter(|deadline| clock < *deadline)?;
        self.behavior.engagement
    }
}
impl Game {
    pub(crate) fn install_behavior(
        &mut self,
        id: ActorId,
        behavior: Behavior,
    ) -> Result<(), GameError> {
        let mut actor = self.actors.get_mut(&id).ok_or(GameError::UnknownActor)?;
        behavior.validate_deadline(actor.behavior_policy())?;
        actor.behavior = behavior;
        Ok(())
    }
    pub(crate) fn behavior_valid(&self, id: ActorId, actor: &crate::Actor) -> bool {
        actor.behavior.engagement.is_none_or(|entry| {
            entry.target != id
                && (self.actors.contains_key(&entry.target) || self.detached_actor(entry.target))
                && self.world.knows(entry.location)
                && entry.last_seen <= self.actor_clock(id)
                && actor.engagement_deadline().is_some()
        })
    }

    /// A committed strike must name its caster through the existing event AND
    /// disclose a matching live body. A private plan, hidden anchor or partial
    /// body without a disclosed caster grants no identity or last-seen location.
    pub(crate) fn provoke_from_strike(
        &mut self,
        outcome: &crate::abilities::Outcome,
        strike: u16,
    ) -> Result<(), GameError> {
        if outcome.actor == outcome.target
            || !outcome.target_participated()
            || !self.alive(outcome.target)
        {
            return Ok(());
        }
        let Some(ability) = outcome.ability.as_ref() else {
            return Ok(());
        };
        if !self
            .content
            .rules
            .ability(ability)
            .is_some_and(|definition| {
                definition.interaction == crate::rules::Interaction::HostileToTarget
            })
        {
            return Ok(());
        }
        let view = self.observe(outcome.target)?;
        let licensed = view.combat.as_ref().is_some_and(|combat| combat.events.iter().any(|event| {
            matches!(event, crate::combat::DisclosedCombatEvent::Ability { caster: Some(caster), target: Some(target), id, strike: Some(ordinal), .. }
                if *caster == outcome.actor && *target == outcome.target && id == ability && *ordinal == strike)
        }));
        let witness = view
            .visible_actors
            .iter()
            .find(|actor| actor.id == outcome.actor);
        if let (true, Some(witness)) = (licensed, witness) {
            let memory = self.actors[&outcome.target]
                .behavior_policy()
                .ok_or(GameError::UnknownActor)?
                .memory_ticks;
            if memory == 0 {
                return Ok(());
            }
            self.actor_clock(outcome.target)
                .checked_add(memory)
                .ok_or(GameError::Schedule(crate::ScheduleError::TimeExhausted))?;
            let location = witness.location;
            let mut settled = self.settled_actor_outcome(outcome.target)?;
            settled.state.behavior.engagement = Some(Engagement {
                target: outcome.actor,
                location,
                last_seen: self.actor_clock(outcome.target),
            });
            self.apply_outcome(settled)?;
            self.reconcile_recovery_contexts()?;
        }
        Ok(())
    }
}
