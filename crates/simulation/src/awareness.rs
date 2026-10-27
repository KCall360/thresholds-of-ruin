//! Committed sensory events own awareness contests. Queries only read receipts.
//! Location alerts are snapshots, never actor references or identification.
use crate::{ActorId, Game, GameError};
use serde::{Deserialize, Serialize};
use tor_world::Location;

const MAX_CONTACTS: usize = 4096;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
struct Contact {
    subject: ActorId,
    observer_at: Location,
    source_at: Location,
    detected: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LocationAlert {
    pub location: Location,
    pub expires: u64,
}

impl<'de> Deserialize<'de> for Contact {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            subject: ActorId,
            #[serde(deserialize_with = "crate::public_plan::read_location")]
            observer_at: Location,
            #[serde(deserialize_with = "crate::public_plan::read_location")]
            source_at: Location,
            detected: bool,
        }
        let f: Fields = crate::bounded::named_map(d)?;
        if f.subject.0 == 0 || f.subject.0 == u64::MAX {
            return Err(serde::de::Error::custom("invalid awareness subject"));
        }
        Ok(Self {
            subject: f.subject,
            observer_at: f.observer_at,
            source_at: f.source_at,
            detected: f.detected,
        })
    }
}
impl<'de> Deserialize<'de> for LocationAlert {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            #[serde(deserialize_with = "crate::public_plan::read_location")]
            location: Location,
            expires: u64,
        }
        let f: Fields = crate::bounded::named_map(d)?;
        Ok(Self {
            location: f.location,
            expires: f.expires,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Awareness {
    contacts: Vec<Contact>,
    pub alert: Option<LocationAlert>,
    period: u64,
    elapsed: u64,
    pending: Option<u32>,
}
impl Default for Awareness {
    fn default() -> Self {
        Self {
            contacts: vec![],
            alert: None,
            period: 500,
            elapsed: 0,
            pending: None,
        }
    }
}
impl<'de> Deserialize<'de> for Awareness {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            contacts: crate::bounded::Bounded<Contact, MAX_CONTACTS>,
            #[serde(deserialize_with = "crate::bounded::required_option")]
            alert: Option<LocationAlert>,
            period: u64,
            elapsed: u64,
            #[serde(deserialize_with = "crate::bounded::required_option")]
            pending: Option<u32>,
        }
        let f: Fields = crate::bounded::named_map(d)?;
        if !(1..=crate::limits::NUMERIC as u64).contains(&f.period)
            || f.elapsed >= f.period
            || f.pending
                .is_some_and(|n| n == 0 || u64::from(n) > crate::limits::NUMERIC as u64)
            || !f.contacts.0.windows(2).all(|w| w[0].subject < w[1].subject)
            || f.contacts
                .0
                .iter()
                .any(|c| c.subject.0 == 0 || c.subject.0 == u64::MAX)
        {
            return Err(serde::de::Error::custom("invalid awareness state"));
        }
        Ok(Self {
            contacts: f.contacts.0,
            alert: f.alert,
            period: f.period,
            elapsed: f.elapsed,
            pending: f.pending,
        })
    }
}
impl Awareness {
    pub(crate) fn reconcile_alert(
        &mut self,
        memory: Option<u64>,
        clock: u64,
    ) -> Result<(), GameError> {
        let Some(alert) = &mut self.alert else {
            return Ok(());
        };
        let Some(memory) = memory.filter(|n| *n > 0) else {
            self.alert = None;
            return Ok(());
        };
        if alert.expires <= clock {
            self.alert = None;
            return Ok(());
        }
        let horizon = clock
            .checked_add(memory)
            .ok_or(GameError::Schedule(crate::ScheduleError::TimeExhausted))?;
        alert.expires = alert.expires.min(horizon);
        Ok(())
    }

    pub(crate) fn detected(&self, subject: ActorId) -> bool {
        self.contacts
            .binary_search_by_key(&subject, |c| c.subject)
            .ok()
            .is_some_and(|i| self.contacts[i].detected)
    }
    pub(crate) fn alert_at(&self, clock: u64) -> Option<Location> {
        self.alert.filter(|a| clock < a.expires).map(|a| a.location)
    }
    pub(crate) fn take_pending(&mut self) -> Option<u32> {
        self.pending.take()
    }
    pub(crate) fn pending(&self) -> bool {
        self.pending.is_some()
    }
    pub(crate) fn next_in(&self, actor: &crate::Actor) -> Option<u64> {
        (actor.alive() && actor.perception().aggravation.is_some())
            .then_some(self.period - self.elapsed)
    }
    pub(crate) fn rephase(&mut self, policy: Option<crate::perception::AggravationPolicy>) {
        if let Some(policy) = policy {
            self.elapsed = (u128::from(self.elapsed) * u128::from(policy.interval)
                / u128::from(self.period)) as u64;
            self.period = policy.interval;
        }
    }
    pub(crate) fn advance(&mut self, elapsed: u64, denominator: u32) -> Result<(), GameError> {
        if elapsed > self.period - self.elapsed || self.pending.is_some() {
            return Err(GameError::Schedule(crate::ScheduleError::TimeExhausted));
        }
        self.elapsed += elapsed;
        if self.elapsed == self.period {
            self.elapsed = 0;
            self.pending = Some(denominator);
        }
        Ok(())
    }
    pub(crate) fn shift(&mut self, delta: u64) -> Result<(), GameError> {
        if let Some(alert) = &mut self.alert {
            alert.expires = alert
                .expires
                .checked_add(delta)
                .ok_or(GameError::Schedule(crate::ScheduleError::TimeExhausted))?;
        }
        Ok(())
    }
    pub(crate) fn valid_for(&self, actor: &crate::Actor) -> bool {
        !self.pending()
            && actor
                .perception()
                .aggravation
                .is_none_or(|p| p.interval == self.period)
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct PersonalContactWitness {
    observer: ActorId,
    contact: Option<(ActorId, crate::perception::ActorReferenceKind)>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct AwarenessWitness {
    world: tor_world::PerceptionSnapshot,
    actors: u64,
    eligible: Vec<ActorId>,
    hostility: std::collections::BTreeMap<String, std::collections::BTreeSet<String>>,
    personal: Vec<PersonalContactWitness>,
}
#[derive(Clone, Debug, Default)]
pub(crate) struct AwarenessContexts(tor_world::Shared<Option<AwarenessWitness>>);
impl PartialEq for AwarenessContexts {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}
impl Eq for AwarenessContexts {}

// A move-only, pre-mutation witness. It is never serialized or reconstructed
// from current positions; the validated primary publication consumes it once.
#[derive(Debug)]
pub(crate) struct AttackAwarenessWitness {
    source: ActorId,
    target: ActorId,
    cues: Vec<AttackAwarenessCue>,
}
#[derive(Debug)]
struct AttackAwarenessCue {
    listener: ActorId,
    alert: Option<Location>,
    reference: Option<crate::perception::ActorReferenceKind>,
    others: Vec<BattleContact>,
}
#[derive(Debug)]
struct BattleContact {
    subject: ActorId,
    observer_at: Location,
    source_at: Location,
    reference: crate::perception::ActorReferenceKind,
}

impl Game {
    /// Called only after the caller has disclosed a matching live body. Ordinary
    /// bodies need no stealth receipt, including personal engagements established
    /// without a faction encounter. A continual-stealth source does require one.
    pub(crate) fn awareness_allows_target(&self, observer: ActorId, subject: ActorId) -> bool {
        self.actors.get(&subject).is_some_and(|source| {
            source.perception().continual_stealth.is_none()
                || self
                    .actors
                    .get(&observer)
                    .is_some_and(|actor| actor.awareness.detected(subject))
        })
    }

    fn awareness_eligible(&self, id: ActorId) -> bool {
        self.actors.get(&id).is_some_and(|a| {
            a.alive()
                && a.combat.is_some()
                && !(a.perception().readied_touch_ambush && a.guise.appearance().is_some())
        }) && !self.actor_frozen(id)
            && self.condition_allows_voluntary_input(id)
    }
    pub(crate) fn awareness_valid(&self, id: ActorId, actor: &crate::Actor) -> bool {
        actor.awareness.valid_for(actor)
            && actor.awareness.contacts.iter().all(|c| {
                c.subject != id
                    && (self.actors.contains_key(&c.subject) || self.detached_actor(c.subject))
                    && self.world.knows(c.source_at)
                    && self.world.knows(c.observer_at)
            })
            && actor.awareness.alert.is_none_or(|a| {
                self.world.knows(a.location)
                    && actor.behavior_policy().is_some_and(|p| {
                        p.memory_ticks > 0
                            && self
                                .actor_clock(id)
                                .checked_add(p.memory_ticks)
                                .is_some_and(|latest| a.expires <= latest)
                    })
            })
    }
    /// Initial encounter, movement and renewed contact are causal events. A
    /// detected body's later movement does not make it disappear behind a ring.
    pub(crate) fn reconcile_awareness(&mut self) -> Result<(), GameError> {
        if self.lifecycle.initial_awareness_deferred {
            return Ok(());
        }
        let observers: Vec<_> = self
            .combat
            .ai
            .keys()
            .copied()
            .filter(|id| self.awareness_eligible(*id))
            .collect();
        let world = self.world.perception_snapshot();
        let actors = self.actors.visibility_epoch();
        let personal: Vec<_> = observers
            .iter()
            .map(|id| PersonalContactWitness {
                observer: *id,
                contact: self.actors[id]
                    .engagement_at(self.actor_clock(*id))
                    .map(|contact| (contact.target, contact.reference)),
            })
            .collect();
        if self
            .lifecycle
            .awareness_contexts
            .0
            .as_ref()
            .is_some_and(|old| {
                old.world == world
                    && old.actors == actors
                    && old.eligible == observers
                    && old.personal == personal
                    && old.hostility == self.combat.hostility
            })
        {
            return Ok(());
        }
        let witness = AwarenessWitness {
            world,
            actors,
            eligible: observers.clone(),
            personal,
            hostility: self.combat.hostility.clone(),
        };
        for observer in observers {
            if !self.awareness_eligible(observer) {
                continue;
            }
            let view = self.observe(observer)?;
            let old = self.actors[&observer].awareness.contacts.clone();
            let mut contacts = Vec::new();
            let mut subjects = std::collections::BTreeSet::new();
            for visible in &view.visible_actors {
                if visible.id == observer
                    || !self.perceived_hostile(observer, visible.id, visible.disclosure)
                    || !subjects.insert(visible.id)
                {
                    continue;
                }
                if contacts.len() == MAX_CONTACTS {
                    return Err(GameError::InvalidLocation);
                }
                // Public projection rows are occupied body cells, not distinct
                // actors. A visible head still identifies one canonical anchor.
                let source_at = self.actors[&visible.id].location;
                let previous = old.iter().find(|c| c.subject == visible.id);
                let unchanged = previous
                    .is_some_and(|c| c.observer_at == view.location && c.source_at == source_at);
                let unhidden = self.actors[&visible.id]
                    .perception()
                    .continual_stealth
                    .is_none();
                let detected = if unhidden || previous.is_some_and(|c| c.detected) {
                    true
                } else if unchanged {
                    previous.expect("receipt").detected
                } else {
                    self.resolve_awareness_contest(observer, visible.id)?
                };
                contacts.push(Contact {
                    subject: visible.id,
                    observer_at: view.location,
                    source_at,
                    detected,
                });
            }
            contacts.sort_by_key(|c| c.subject);
            self.actors
                .get_mut(&observer)
                .expect("observer")
                .awareness
                .contacts = contacts;
        }
        self.lifecycle.awareness_contexts.0 = tor_world::Shared::new(Some(witness));
        Ok(())
    }
    fn resolve_awareness_contest(
        &mut self,
        observer: ActorId,
        subject: ActorId,
    ) -> Result<bool, GameError> {
        use crate::attributes::{Skill, SkillCheck};
        use crate::rules::{CheckActor, OutgoingEdgePolicy};
        let source = self.creature(subject).ok_or(GameError::UnknownActor)?;
        let recipient = self.creature(observer).ok_or(GameError::UnknownActor)?;
        let rules = self.content.rules();
        let mut operands = Vec::with_capacity(2);
        for (roller, opponent, actor, other, role, skill) in [
            (
                subject,
                observer,
                &source,
                &recipient,
                CheckActor::User,
                Skill::Stealth,
            ),
            (
                observer,
                subject,
                &recipient,
                &source,
                CheckActor::Target,
                Skill::Perception,
            ),
        ] {
            let edge = actor
                .derived()
                .check_edge_with_selection(
                    other.derived(),
                    rules,
                    crate::creatures::CheckContext {
                        ability: None,
                        roller_role: role,
                        roller,
                        opponent,
                        defense: None,
                    },
                    true,
                    OutgoingEdgePolicy::Ordinary,
                    &[],
                )
                .map_err(|_| GameError::Rule(crate::abilities::RuleError::InvalidDefinition))?
                + actor.derived().perception.skill_edge(skill);
            let bonus = SkillCheck {
                skill,
                binding: actor.derived().binding(),
                modifier: 0,
                threshold: 0,
            }
            .bonus(actor.derived().attributes, actor.derived().skills);
            operands.push((bonus, edge));
        }
        let mut rng = self.rules_rng;
        let totals: Vec<_> = operands
            .into_iter()
            .map(|(bonus, edge)| {
                let balance = edge.clamp(-3, 3);
                let edge = crate::dice::Edge::from_counts(
                    balance.max(0) as u32,
                    balance.saturating_neg().max(0) as u32,
                );
                bonus + i64::from(crate::dice::roll_check(&mut rng, edge).kept)
            })
            .collect();
        self.rules_rng = rng;
        // Automatic awareness is not a paid check. It neither borrows nor
        // consumes next-check tokens, matching passive discovery ownership.
        Ok(totals[1] >= totals[0])
    }
    /// A floor-wide supernatural cue conveys this point only. It cannot license
    /// an attack or pierce a guise, and never cancels magical sleep.
    pub(crate) fn alert_location_pulse(&mut self, source: ActorId) -> Result<(), GameError> {
        let location = self
            .actors
            .get(&source)
            .ok_or(GameError::UnknownActor)?
            .location;
        let listeners: Vec<_> = self
            .combat
            .ai
            .keys()
            .copied()
            .filter(|id| {
                *id != source
                    && self.awareness_eligible(*id)
                    && self.same_population_scope(self.actors[id].location, location)
            })
            .collect();
        for id in listeners {
            self.install_location_alert(id, location)?;
        }
        Ok(())
    }
    pub(crate) fn install_location_alert(
        &mut self,
        listener: ActorId,
        location: Location,
    ) -> Result<(), GameError> {
        let actor = &self.actors[&listener];
        let Some(policy) = actor.behavior_policy() else {
            return Ok(());
        };
        if policy.disposition != crate::behavior::Disposition::Mean || policy.memory_ticks == 0 {
            return Ok(());
        }
        let expires = self
            .actor_clock(listener)
            .checked_add(policy.memory_ticks)
            .ok_or(GameError::Schedule(crate::ScheduleError::TimeExhausted))?;
        self.actors
            .get_mut(&listener)
            .expect("listener")
            .awareness
            .alert = Some(LocationAlert { location, expires });
        Ok(())
    }
    pub(crate) fn settle_aggravation_for(&mut self, id: ActorId) -> Result<(), GameError> {
        let pending = self
            .actors
            .get_mut(&id)
            .and_then(|mut a| a.awareness.pending.take());
        if let Some(denominator) = pending {
            self.run_aggravation_trial(id, denominator)?;
        }
        Ok(())
    }
    pub(crate) fn run_aggravation_trial(
        &mut self,
        id: ActorId,
        denominator: u32,
    ) -> Result<(), GameError> {
        if crate::dice::random_word(&mut self.rules_rng).is_multiple_of(u64::from(denominator)) {
            self.alert_location_pulse(id)?;
        }
        Ok(())
    }
    pub(crate) fn settle_aggravation(&mut self) -> Result<(), GameError> {
        let pending: Vec<_> = self.actors.pending_aggravation().collect();
        for id in pending {
            self.settle_aggravation_for(id)?;
        }
        Ok(())
    }
    pub(crate) fn capture_attack_awareness(
        &self,
        source: ActorId,
        target: ActorId,
    ) -> Result<AttackAwarenessWitness, GameError> {
        self.capture_attack_awareness_with_impact(source, target, true)
    }
    pub(crate) fn capture_unavailable_attack_awareness(
        &self,
        source: ActorId,
        target: ActorId,
    ) -> Result<AttackAwarenessWitness, GameError> {
        self.capture_attack_awareness_with_impact(source, target, false)
    }
    fn capture_attack_awareness_with_impact(
        &self,
        source: ActorId,
        target: ActorId,
        impact: bool,
    ) -> Result<AttackAwarenessWitness, GameError> {
        let source_at = self
            .actors
            .get(&source)
            .ok_or(GameError::UnknownActor)?
            .location;
        let impact_at = impact
            .then(|| self.actors.get(&target).map(|actor| actor.location))
            .flatten();
        let mut cues = Vec::new();
        for listener in self.combat.ai.keys().copied() {
            if listener == source || !self.awareness_eligible(listener) {
                continue;
            }
            let actor = &self.actors[&listener];
            let scene = self.world.eye_scene(actor.location, actor.orientation, 6);
            let heard = |at| {
                scene
                    .iter()
                    .any(|cell| cell.physical && cell.location == at)
            };
            // Prefer the heard source point; an impact-only witness learns just
            // that point, never the remote attacker's current position.
            let alert = if heard(source_at) {
                Some(source_at)
            } else {
                impact_at.filter(|at| heard(*at))
            };
            let view = self.observe(listener)?;
            let reference = view
                .visible_actors
                .iter()
                .find(|actor| actor.id == source)
                .and_then(|actor| actor.disclosure.reference());
            if alert.is_none() && reference.is_none() {
                continue;
            }
            if cues.len() >= MAX_CONTACTS {
                return Err(GameError::InvalidLocation);
            }
            let others: std::collections::BTreeMap<_, _> = view
                .visible_actors
                .iter()
                .filter(|other| {
                    other.id != source
                        && other.id != listener
                        && self.actors[&other.id]
                            .perception()
                            .continual_stealth
                            .is_some()
                        && !self.actors[&listener].awareness.detected(other.id)
                        && self.perceived_hostile(listener, other.id, other.disclosure)
                })
                .filter_map(|other| {
                    other.disclosure.reference().map(|reference| {
                        (
                            other.id,
                            BattleContact {
                                subject: other.id,
                                observer_at: view.location,
                                source_at: self.actors[&other.id].location,
                                reference,
                            },
                        )
                    })
                })
                .collect();
            // Match the existing per-observer contact bound; do not impose a
            // new global crowd limit on unrelated ordinary visible creatures.
            if others.len() > MAX_CONTACTS {
                return Err(GameError::InvalidLocation);
            }
            cues.push(AttackAwarenessCue {
                listener,
                alert,
                reference,
                others: others.into_values().collect(),
            });
        }
        Ok(AttackAwarenessWitness {
            source,
            target,
            cues,
        })
    }

    pub(crate) fn awareness_on_attack(
        &mut self,
        outcome: &crate::abilities::Outcome,
        witness: AttackAwarenessWitness,
    ) -> Result<(), GameError> {
        if witness.source != outcome.actor || witness.target != outcome.target {
            return Err(GameError::Rule(crate::abilities::RuleError::InvalidPlan));
        }
        if outcome.actor == outcome.target
            || !outcome
                .ability
                .as_ref()
                .and_then(|id| self.content.rules().ability(id))
                .is_some_and(|a| a.interaction == crate::rules::Interaction::HostileToTarget)
        {
            return Ok(());
        }
        self.apply_attack_awareness(witness)
    }

    fn awareness_contest_battle_contact(
        &mut self,
        observer: ActorId,
        contact: BattleContact,
    ) -> Result<(), GameError> {
        if !self.awareness_eligible(observer)
            || self.actors[&observer].location != contact.observer_at
            || self
                .actors
                .get(&contact.subject)
                .is_none_or(|subject| subject.location != contact.source_at)
            || self.actors[&observer].awareness.detected(contact.subject)
        {
            return Ok(());
        }
        let view = self.observe(observer)?;
        if !view.visible_actors.iter().any(|other| {
            other.id == contact.subject
                && other.disclosure.reference() == Some(contact.reference)
                && self.perceived_hostile(observer, other.id, other.disclosure)
        }) {
            return Ok(());
        }
        let detected = if self.actors[&contact.subject]
            .perception()
            .continual_stealth
            .is_none()
        {
            true
        } else {
            self.resolve_awareness_contest(observer, contact.subject)?
        };
        let mut actor = self.actors.get_mut(&observer).expect("eligible observer");
        let contacts = &mut actor.awareness.contacts;
        let receipt = Contact {
            subject: contact.subject,
            observer_at: contact.observer_at,
            source_at: contact.source_at,
            detected,
        };
        match contacts.binary_search_by_key(&contact.subject, |entry| entry.subject) {
            Ok(index) => contacts[index] = receipt,
            Err(index) => {
                if contacts.len() >= MAX_CONTACTS {
                    return Err(GameError::InvalidLocation);
                }
                contacts.insert(index, receipt);
            }
        }
        Ok(())
    }

    fn apply_attack_awareness(&mut self, witness: AttackAwarenessWitness) -> Result<(), GameError> {
        for cue in witness.cues {
            if !self.awareness_eligible(cue.listener) {
                continue;
            }
            if let Some(location) = cue.alert {
                self.install_location_alert(cue.listener, location)?;
            }
            for other in cue.others {
                self.awareness_contest_battle_contact(cue.listener, other)?;
            }
            let Some(reference) = cue.reference else {
                continue;
            };
            let view = self.observe(cue.listener)?;
            if !view.visible_actors.iter().any(|actor| {
                actor.id == witness.source && actor.disclosure.reference() == Some(reference)
            }) {
                continue;
            }
            let source_at = self.actors[&witness.source].location;
            let mut actor = self
                .actors
                .get_mut(&cue.listener)
                .expect("eligible listener");
            let contacts = &mut actor.awareness.contacts;
            match contacts.binary_search_by_key(&witness.source, |contact| contact.subject) {
                Ok(index) => contacts[index].detected = true,
                Err(index) => {
                    if contacts.len() >= MAX_CONTACTS {
                        return Err(GameError::InvalidLocation);
                    }
                    contacts.insert(
                        index,
                        Contact {
                            subject: witness.source,
                            observer_at: view.location,
                            source_at,
                            detected: true,
                        },
                    );
                }
            }
        }
        self.lifecycle.awareness_contexts = Default::default();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::perception::contract_fixtures::{at, OWNER, SUBJECT};
    use crate::perception::PerceptionValue;

    fn encounter(stealth: bool) -> Game {
        encounter_with_height(stealth, 1)
    }

    fn encounter_with_height(stealth: bool, height: i32) -> Game {
        let modifiers = if stealth {
            vec![PerceptionValue::ContinualStealth { edge: 1 }]
        } else {
            vec![]
        };
        let mut game = crate::perception::contract_fixtures::fixture_with_height(
            &[],
            &modifiers,
            height,
            |_, owner, _| {
                owner.behavior.disposition = crate::behavior::Disposition::Mean;
                owner.behavior.idle = crate::behavior::IdleBehavior::Wait;
            },
        );
        game.actors
            .get_mut(&OWNER)
            .unwrap()
            .combat
            .as_mut()
            .unwrap()
            .identity
            .faction = "observer".into();
        game.combat
            .hostility
            .insert("observer".into(), ["private affiliation".into()].into());
        game.combat
            .hostility
            .insert("private affiliation".into(), ["observer".into()].into());
        game.configure_ai(
            OWNER,
            crate::ai::AiProfile {
                idle: crate::behavior::IdleBehavior::Wait,
                ..Default::default()
            },
        )
        .unwrap();
        game.refresh_navigation();
        game.initialize_discovery().unwrap();
        game
    }

    #[test]
    fn ordinary_approach_detects_without_lottery_or_query_mutation() {
        let mut game = encounter(false);
        assert!(game.actors[&OWNER].awareness.detected(SUBJECT));
        let before = game.clone();
        for _ in 0..8 {
            game.observe(OWNER).unwrap();
            game.choose_ai(OWNER).unwrap();
        }
        assert_eq!(game, before);
        game.reconcile_awareness().unwrap();
        assert_eq!(game.rules_rng, before.rules_rng);
    }

    #[test]
    fn awareness_cache_tracks_hostility_and_personal_contact_expiry() {
        let mut game = encounter(false);
        game.combat.hostility.clear();
        game.reconcile_awareness().unwrap();
        assert!(!game.actors[&OWNER].awareness.detected(SUBJECT));
        game.actors.get_mut(&OWNER).unwrap().behavior.engagement =
            Some(crate::behavior::Engagement {
                reference: crate::perception::ActorReferenceKind::Creature,
                target: SUBJECT,
                location: game.actors[&SUBJECT].location,
                last_seen: game.tick,
            });
        game.reconcile_awareness().unwrap();
        assert!(game.actors[&OWNER].awareness.detected(SUBJECT));
        // No geometry or visibility input changes when personal memory expires.
        game.tick = game.actors[&OWNER].engagement_deadline().unwrap();
        game.reconcile_awareness().unwrap();
        assert!(!game.actors[&OWNER].awareness.detected(SUBJECT));
    }

    #[test]
    fn tall_visible_subject_owns_one_contest_and_strict_checkpoint_receipt() {
        let mut game = encounter_with_height(true, 2);
        game.set_body(
            SUBJECT,
            crate::BodySpec {
                cells: vec![[0, 0, 0], [0, 0, 1]],
                eye: [0, 0, 1],
                mass: 80,
            },
        )
        .unwrap();
        assert!(
            game.observe(OWNER)
                .unwrap()
                .visible_actors
                .iter()
                .filter(|row| row.id == SUBJECT)
                .count()
                >= 2
        );
        game.actors
            .get_mut(&OWNER)
            .unwrap()
            .awareness
            .contacts
            .clear();
        game.lifecycle.awareness_contexts = Default::default();
        let mut expected = game.clone();
        let detected = expected.resolve_awareness_contest(OWNER, SUBJECT).unwrap();
        game.reconcile_awareness().unwrap();
        assert_eq!(game.rules_rng, expected.rules_rng, "one actor, one contest");
        let contacts = &game.actors[&OWNER].awareness.contacts;
        assert_eq!(contacts.len(), 1);
        assert_eq!(contacts[0].source_at, game.actors[&SUBJECT].location);
        assert_eq!(contacts[0].detected, detected);
        let valid = serde_json::to_value(&game.actors[&OWNER].awareness).unwrap();
        let decoded: Awareness = serde_json::from_value(valid.clone()).unwrap();
        assert_eq!(decoded, game.actors[&OWNER].awareness);
        let mut duplicate = valid.clone();
        duplicate["contacts"]
            .as_array_mut()
            .unwrap()
            .push(valid["contacts"][0].clone());
        assert!(serde_json::from_value::<Awareness>(duplicate).is_err());
        let mut unordered = valid.clone();
        let mut earlier = valid["contacts"][0].clone();
        earlier["subject"] = serde_json::json!(OWNER.0);
        unordered["contacts"].as_array_mut().unwrap().push(earlier);
        assert!(serde_json::from_value::<Awareness>(unordered).is_err());
        let mut shared = crate::checkpoint::SharedState::default();
        let snapshot = game.checkpoint(&mut shared);
        let bytes = serde_json::to_vec(&(snapshot, shared)).unwrap();
        let (snapshot, shared) = serde_json::from_slice(&bytes).unwrap();
        let restored = Game::restore_checkpoint(snapshot, &shared, game.content().clone()).unwrap();
        assert_eq!(restored, game);
    }

    #[test]
    fn visible_upper_body_contact_uses_occluded_actor_anchor() {
        let mut game = encounter_with_height(true, 2);
        for actor in [OWNER, SUBJECT] {
            game.set_body(
                actor,
                crate::BodySpec {
                    cells: vec![[0, 0, 0], [0, 0, 1]],
                    eye: [0, 0, 1],
                    mass: 80,
                },
            )
            .unwrap();
        }
        game.teleport(SUBJECT, at(1, 5, 2)).unwrap();
        game.set_wall(at(1, 4, 2), true).unwrap();
        let view = game.observe(OWNER).unwrap();
        let cells: Vec<_> = view
            .visible_actors
            .iter()
            .filter(|row| row.id == SUBJECT)
            .map(|row| row.location)
            .collect();
        assert!(!cells.is_empty(), "upper body remains physically visible");
        assert!(
            !cells.contains(&game.actors[&SUBJECT].location),
            "anchor is occluded"
        );
        game.actors
            .get_mut(&OWNER)
            .unwrap()
            .awareness
            .contacts
            .clear();
        game.lifecycle.awareness_contexts = Default::default();
        game.reconcile_awareness().unwrap();
        let contacts = &game.actors[&OWNER].awareness.contacts;
        assert_eq!(contacts.len(), 1);
        assert_eq!(contacts[0].source_at, game.actors[&SUBJECT].location);
        let rng = game.rules_rng;
        game.lifecycle.awareness_contexts = Default::default();
        game.reconcile_awareness().unwrap();
        assert_eq!(
            game.rules_rng, rng,
            "head-only receipt is still the same contact"
        );
    }

    #[test]
    fn attempted_stealth_receipt_survives_queries_and_checkpoint() {
        let mut game = encounter(true);
        let before = game.rules_rng;
        let receipt = game.actors[&OWNER].awareness.clone();
        for _ in 0..8 {
            game.observe(OWNER).unwrap();
            game.choose_ai(OWNER).unwrap();
            game.reconcile_awareness().unwrap();
        }
        assert_eq!(game.rules_rng, before);
        assert_eq!(game.actors[&OWNER].awareness, receipt);
        let mut shared = crate::checkpoint::SharedState::default();
        let snapshot =
            serde_json::from_slice(&serde_json::to_vec(&game.checkpoint(&mut shared)).unwrap())
                .unwrap();
        let shared = serde_json::from_slice(&serde_json::to_vec(&shared).unwrap()).unwrap();
        let mut restored =
            Game::restore_checkpoint(snapshot, &shared, game.content().clone()).unwrap();
        restored.reconcile_awareness().unwrap();
        assert_eq!(restored.rules_rng, before);
        assert_eq!(restored.actors[&OWNER].awareness, receipt);
    }

    #[test]
    fn location_pulse_has_no_identity_and_does_not_track_source() {
        let mut game = encounter(false);
        game.alert_location_pulse(SUBJECT).unwrap();
        assert!(game.actors[&OWNER].behavior.engagement.is_none());
        assert_eq!(
            game.actors[&OWNER].awareness.alert_at(game.tick),
            Some(at(1, 3, 2))
        );
        game.teleport(SUBJECT, at(2, 4, 2)).unwrap();
        assert_eq!(
            game.actors[&OWNER].awareness.alert_at(game.tick),
            Some(at(1, 3, 2))
        );
        let expires = game.actors[&OWNER].awareness.alert.unwrap().expires;
        assert_eq!(game.actors[&OWNER].awareness.alert_at(expires), None);
    }

    #[test]
    fn cadence_preserves_fraction_and_serialization_rejects_malformed_state() {
        let mut clock = Awareness::default();
        clock.advance(499, 3).unwrap();
        clock.rephase(None);
        clock.rephase(Some(crate::perception::AggravationPolicy {
            interval: 500,
            chance_denominator: 3,
            stealth_disadvantage: 1,
        }));
        assert_eq!(clock.elapsed, 499);
        let value = serde_json::to_value(&clock).unwrap();
        assert_eq!(
            serde_json::from_value::<Awareness>(value.clone()).unwrap(),
            clock
        );
        for key in ["contacts", "alert", "period", "elapsed", "pending"] {
            let mut invalid = value.clone();
            invalid.as_object_mut().unwrap().remove(key);
            assert!(
                serde_json::from_value::<Awareness>(invalid).is_err(),
                "missing {key}"
            );
        }
        clock.advance(1, 3).unwrap();
        assert_eq!(clock.take_pending(), Some(3));
        assert_eq!(clock.take_pending(), None);
        assert_eq!(clock.elapsed, 0);
    }

    #[test]
    fn frozen_alert_shifts_exactly_without_live_tracking() {
        let mut state = Awareness {
            alert: Some(LocationAlert {
                location: at(1, 3, 2),
                expires: 1000,
            }),
            ..Default::default()
        };
        state.shift(3000).unwrap();
        assert_eq!(state.alert_at(3999), Some(at(1, 3, 2)));
        assert_eq!(state.alert_at(4000), None);
    }
}

#[cfg(test)]
mod event_tests {
    use super::*;
    use crate::perception::contract_fixtures::{at, fixture_with, OWNER, SUBJECT};
    use crate::perception::PerceptionValue;

    fn pair() -> Game {
        let mut game = fixture_with(
            &[],
            &[PerceptionValue::ContinualStealth { edge: 1 }],
            |_, owner, _| {
                owner.behavior.disposition = crate::behavior::Disposition::Mean;
            },
        );
        game.actors
            .get_mut(&OWNER)
            .unwrap()
            .combat
            .as_mut()
            .unwrap()
            .identity
            .faction = "observer".into();
        game.combat
            .hostility
            .insert("observer".into(), ["private affiliation".into()].into());
        game.configure_ai(OWNER, crate::ai::AiProfile::default())
            .unwrap();
        game
    }

    #[test]
    fn opposed_roll_has_success_and_failure_and_unchanged_events_do_not_retry() {
        let base = pair();
        let mut outcomes = std::collections::BTreeSet::new();
        for seed in 0..64 {
            let mut game = base.clone();
            game.rules_rng = seed;
            game.actors
                .get_mut(&OWNER)
                .unwrap()
                .awareness
                .contacts
                .clear();
            game.lifecycle.awareness_contexts = Default::default();
            game.reconcile_awareness().unwrap();
            outcomes.insert(game.actors[&OWNER].awareness.detected(SUBJECT));
            let rng = game.rules_rng;
            game.lifecycle.awareness_contexts = Default::default();
            game.reconcile_awareness().unwrap();
            assert_eq!(
                game.rules_rng, rng,
                "cache eviction must not reroll persisted event"
            );
        }
        assert_eq!(outcomes, [false, true].into());
    }

    #[test]
    fn direct_detection_persists_until_contact_break_then_can_be_contested_again() {
        let mut game = pair();
        // A directly detected body remains detected on later movement.
        game.actors.get_mut(&OWNER).unwrap().awareness.contacts[0].detected = true;
        game.teleport(SUBJECT, at(1, 4, 2)).unwrap();
        assert!(game.actors[&OWNER].awareness.detected(SUBJECT));
        game.teleport(SUBJECT, at(2, 4, 2)).unwrap();
        assert!(!game.actors[&OWNER].awareness.detected(SUBJECT));
    }
}

#[cfg(test)]
mod cadence_tests {
    use super::*;
    use crate::perception::{
        contract_fixtures::{fixture_with, OWNER, SUBJECT},
        PerceptionValue,
    };

    #[test]
    fn analytic_advance_stops_at_single_nonstacking_pulse_without_nutrition_trigger() {
        let pulse = PerceptionValue::Aggravation {
            interval: 500,
            chance_denominator: 1,
            stealth_disadvantage: 1,
        };
        let mut game = fixture_with(&[pulse.clone(), pulse], &[], |document, _, listener| {
            assert!(!document.nutrition_condition_trigger.enabled());
            listener.behavior.disposition = crate::behavior::Disposition::Mean;
        });
        game.configure_ai(SUBJECT, crate::ai::AiProfile::default())
            .unwrap();
        let before = game.rules_rng;
        let mut expected_rng = before;
        crate::dice::random_word(&mut expected_rng);
        assert_eq!(game.advance_physics(2000).unwrap(), 500);
        assert_eq!(
            game.rules_rng, expected_rng,
            "one wearer stream, not one trial per source"
        );
        let source_at = game.actors[&OWNER].location;
        assert_eq!(
            game.actors[&SUBJECT].awareness.alert_at(500),
            Some(source_at)
        );
        let after = game.clone();
        for _ in 0..5 {
            game.observe(SUBJECT).unwrap();
            game.next_creature_event_tick();
        }
        assert_eq!(game, after);
        assert_eq!(game.next_creature_event_tick(), Some(1000));
    }

    #[test]
    fn malformed_location_records_and_unbounded_alerts_are_rejected() {
        let location = crate::perception::contract_fixtures::at(1, 2, 2);
        assert!(
            serde_json::from_value::<LocationAlert>(serde_json::json!([location, 1000])).is_err()
        );
        assert!(serde_json::from_value::<Contact>(serde_json::json!([
            2, location, location, true
        ]))
        .is_err());
        let mut game = fixture_with(&[], &[], |_, owner, _| owner.behavior.memory_ticks = 1000);
        game.actors.get_mut(&OWNER).unwrap().awareness.alert = Some(LocationAlert {
            location,
            expires: u64::MAX,
        });
        assert!(!game.awareness_valid(OWNER, &game.actors[&OWNER]));
    }
}

#[cfg(test)]
mod operand_tests {
    use crate::content_ids::ConditionId;
    use crate::perception::{
        contract_fixtures::{fixture_with, OWNER, SUBJECT},
        PerceptionValue,
    };

    #[test]
    fn opposed_awareness_uses_actual_opponent_for_source_scoped_edges() {
        let base = fixture_with(
            &[],
            &[PerceptionValue::ContinualStealth { edge: 1 }],
            |document, _, _| {
                let mut condition = document
                    .conditions
                    .iter()
                    .find(|c| c.id.as_str() == "readied")
                    .unwrap()
                    .clone();
                condition.id = ConditionId::new("awareness_scoped").unwrap();
                condition.family =
                    crate::content_ids::ConditionFamilyId::new("awareness_scoped").unwrap();
                condition.lifecycle = crate::rules::ConditionLifecycle::Timed;
                condition.source_scoped = true;
                condition.modifiers = vec![crate::rules::Modifier {
                    cancellable: false,
                    scope: crate::rules::Scope::VersusSource,
                    op: crate::rules::ModifierOp::Add {
                        stat: crate::rules::Stat::CheckEdge,
                        value: -3,
                    },
                }];
                document.conditions.push(condition);
            },
        );
        let id = ConditionId::new("awareness_scoped").unwrap();
        let mut scoped = base.clone();
        scoped.apply_condition(SUBJECT, &id, OWNER, 1000).unwrap();
        let mut unrelated = base.clone();
        unrelated
            .apply_condition(SUBJECT, &id, SUBJECT, 1000)
            .unwrap();
        let mut changed = false;
        for seed in 0..64 {
            let mut plain = base.clone();
            let mut exact = scoped.clone();
            let mut other = unrelated.clone();
            plain.rules_rng = seed;
            exact.rules_rng = seed;
            other.rules_rng = seed;
            let baseline = plain.resolve_awareness_contest(OWNER, SUBJECT).unwrap();
            assert_eq!(
                other.resolve_awareness_contest(OWNER, SUBJECT).unwrap(),
                baseline
            );
            changed |= exact.resolve_awareness_contest(OWNER, SUBJECT).unwrap() != baseline;
        }
        assert!(
            changed,
            "source-scoped penalty must affect the actual opposing observer"
        );
    }

    #[test]
    fn automatic_awareness_neither_borrows_nor_consumes_next_check_tokens() {
        let mut game = fixture_with(
            &[],
            &[PerceptionValue::ContinualStealth { edge: 1 }],
            |_, _, _| {},
        );
        let distracted = ConditionId::new("distracted").unwrap();
        for actor in [OWNER, SUBJECT] {
            game.apply_condition(actor, &distracted, actor, 1000)
                .unwrap();
            assert!(game
                .creature(actor)
                .unwrap()
                .conditions()
                .has_active(&distracted));
        }
        let plain = fixture_with(
            &[],
            &[PerceptionValue::ContinualStealth { edge: 1 }],
            |_, _, _| {},
        );
        for seed in 0..32 {
            let mut baseline = plain.clone();
            let mut prepared = game.clone();
            baseline.rules_rng = seed;
            prepared.rules_rng = seed;
            assert_eq!(
                prepared.resolve_awareness_contest(OWNER, SUBJECT).unwrap(),
                baseline.resolve_awareness_contest(OWNER, SUBJECT).unwrap()
            );
            assert_eq!(prepared.rules_rng, baseline.rules_rng);
        }
        game.resolve_awareness_contest(OWNER, SUBJECT).unwrap();
        for actor in [OWNER, SUBJECT] {
            assert!(game
                .creature(actor)
                .unwrap()
                .conditions()
                .has_active(&distracted));
        }
    }
}

#[cfg(test)]
mod attack_witness_tests {
    use super::*;
    use crate::perception::{
        contract_fixtures::{at, spawn, OWNER, SUBJECT},
        PerceptionValue,
    };

    fn encounter() -> Game {
        encounter_with_height(1)
    }

    fn encounter_with_height(height: i32) -> Game {
        let mut game = crate::perception::contract_fixtures::fixture_with_height(
            &[],
            &[PerceptionValue::ContinualStealth { edge: 1 }],
            height,
            |_, owner, _| {
                owner.behavior.disposition = crate::behavior::Disposition::Mean;
            },
        );
        game.configure_ai(OWNER, crate::ai::AiProfile::default())
            .unwrap();
        game
    }

    #[test]
    fn impact_only_listener_gets_impact_snapshot_not_remote_source_identity() {
        let mut game = encounter();
        game.teleport(SUBJECT, at(2, 3, 2)).unwrap();
        let witness = game.capture_attack_awareness(SUBJECT, OWNER).unwrap();
        let cue = witness
            .cues
            .iter()
            .find(|cue| cue.listener == OWNER)
            .unwrap();
        assert_eq!(cue.alert, Some(at(1, 2, 2)));
        assert_eq!(cue.reference, None);
        game.apply_attack_awareness(witness).unwrap();
        assert_eq!(
            game.actors[&OWNER].awareness.alert_at(game.tick),
            Some(at(1, 2, 2))
        );
        assert!(!game.actors[&OWNER].awareness.detected(SUBJECT));
    }

    #[test]
    fn displaced_source_does_not_redirect_already_heard_battle() {
        let mut game = encounter();
        let before = game.actors[&SUBJECT].location;
        let witness = game.capture_attack_awareness(SUBJECT, OWNER).unwrap();
        game.teleport(SUBJECT, at(2, 3, 2)).unwrap();
        game.apply_attack_awareness(witness).unwrap();
        assert_eq!(
            game.actors[&OWNER].awareness.alert_at(game.tick),
            Some(before)
        );
        assert!(!game.actors[&OWNER].awareness.detected(SUBJECT));
    }

    #[test]
    fn newly_visible_source_does_not_acquire_event_time_identity() {
        let mut game = encounter();
        game.teleport(SUBJECT, at(2, 3, 2)).unwrap();
        let witness = game.capture_attack_awareness(SUBJECT, OWNER).unwrap();
        game.teleport(SUBJECT, at(1, 3, 2)).unwrap();
        // This fixture clears ordinary approach receipts to isolate the attack
        // witness. The attack must not manufacture a new visual grant.
        game.actors
            .get_mut(&OWNER)
            .unwrap()
            .awareness
            .contacts
            .clear();
        game.apply_attack_awareness(witness).unwrap();
        assert!(!game.actors[&OWNER].awareness.detected(SUBJECT));
        assert_eq!(
            game.actors[&OWNER].awareness.alert_at(game.tick),
            Some(at(1, 2, 2))
        );
    }

    #[test]
    fn visual_offense_beyond_sound_radius_still_breaks_stealth() {
        let mut game = encounter();
        game.teleport(OWNER, at(1, 0, 2)).unwrap();
        game.teleport(SUBJECT, at(1, 8, 2)).unwrap();
        let remote_target = spawn(&mut game, "fixture_perception_subject", at(2, 2, 2));
        let witness = game
            .capture_attack_awareness(SUBJECT, remote_target)
            .unwrap();
        let cue = witness
            .cues
            .iter()
            .find(|cue| cue.listener == OWNER)
            .unwrap();
        assert_eq!(cue.alert, None);
        assert!(cue.reference.is_some());
        game.actors
            .get_mut(&OWNER)
            .unwrap()
            .awareness
            .contacts
            .clear();
        game.apply_attack_awareness(witness).unwrap();
        assert!(game.actors[&OWNER].awareness.detected(SUBJECT));
    }

    #[test]
    fn future_listener_does_not_receive_reconstructed_noise() {
        let mut game = encounter();
        let witness = game.capture_attack_awareness(SUBJECT, OWNER).unwrap();
        let future = spawn(&mut game, "fixture_perception_owner", at(1, 4, 2));
        game.configure_ai(future, crate::ai::AiProfile::default())
            .unwrap();
        game.apply_attack_awareness(witness).unwrap();
        assert!(game.actors[&future].awareness.alert.is_none());
    }

    #[test]
    fn unrelated_stealth_battle_contact_is_contested_not_automatically_revealed() {
        let mut successes = std::collections::BTreeSet::new();
        for seed in 0..64 {
            let mut game = encounter_with_height(2);
            game.set_body(
                SUBJECT,
                crate::BodySpec {
                    cells: vec![[0, 0, 0], [0, 0, 1]],
                    eye: [0, 0, 1],
                    mass: 80,
                },
            )
            .unwrap();
            game.actors
                .get_mut(&OWNER)
                .unwrap()
                .combat
                .as_mut()
                .unwrap()
                .identity
                .faction = "observer".into();
            game.combat
                .hostility
                .insert("observer".into(), ["private affiliation".into()].into());
            let source = spawn(&mut game, "fixture_perception_owner", at(1, 4, 2));
            game.reconcile_awareness().unwrap();
            // Establish the failed encounter receipt BEFORE freezing the battle.
            // A successful encounter is intentionally excluded from the witness;
            // erasing it after capture would create a new unrelated encounter.
            game.actors
                .get_mut(&OWNER)
                .unwrap()
                .awareness
                .contacts
                .iter_mut()
                .find(|contact| contact.subject == SUBJECT)
                .expect("visible hostile has an encounter receipt")
                .detected = false;
            game.rules_rng = seed;
            let witness = game.capture_attack_awareness(source, OWNER).unwrap();
            let contacts = &witness
                .cues
                .iter()
                .find(|cue| cue.listener == OWNER)
                .unwrap()
                .others;
            assert_eq!(
                contacts.len(),
                1,
                "one battle contest per actor, not body cell"
            );
            assert_eq!(contacts[0].subject, SUBJECT);
            assert_eq!(contacts[0].source_at, game.actors[&SUBJECT].location);
            let mut expected = game.clone();
            expected.resolve_awareness_contest(OWNER, SUBJECT).unwrap();
            game.apply_attack_awareness(witness).unwrap();
            assert_eq!(game.rules_rng, expected.rules_rng);
            assert_ne!(
                game.rules_rng, seed,
                "the battle must actually contest the failed contact"
            );
            successes.insert(game.actors[&OWNER].awareness.detected(SUBJECT));
            let before = game.rules_rng;
            game.reconcile_awareness().unwrap();
            assert_eq!(
                game.rules_rng, before,
                "query/reconciliation must not repeat battle contest"
            );
        }
        assert_eq!(successes, [false, true].into());
    }
}

#[cfg(test)]
mod protected_battle_listener_tests {
    use crate::perception::contract_fixtures::{ambush, fixture_with, guise, OWNER, SUBJECT};

    #[test]
    fn battle_witness_neither_wakes_magical_sleep_nor_intact_ambush() {
        for sleeping in [false, true] {
            let modifiers = if sleeping {
                vec![]
            } else {
                vec![
                    guise(),
                    ambush(),
                    crate::perception::PerceptionValue::ReadiedTouchAmbush { enabled: true },
                ]
            };
            let mut game = fixture_with(&modifiers, &[], |document, owner, _| {
                owner.behavior.disposition = crate::behavior::Disposition::Mean;
                document.conditions.push(serde_json::from_value(serde_json::json!({
                    "id":"fixture_sleep", "family":"fixture_sleep", "name":"Sleep",
                    "lifecycle":"timed", "timing":{"kind":"actor_active","on_expiry":{"kind":"remove"}},
                    "restrictions":["voluntary_actions"], "preparation":"pause",
                    "modifiers":[], "stacking":{"kind":"keep"}, "source_scoped":false,
                    "descriptors":[], "capacity":"reject"
                })).unwrap());
            });
            game.configure_ai(OWNER, crate::ai::AiProfile::default())
                .unwrap();
            if sleeping {
                game.apply_condition(
                    OWNER,
                    &crate::content_ids::ConditionId::new("fixture_sleep").unwrap(),
                    SUBJECT,
                    1000,
                )
                .unwrap();
            }
            let witness = game.capture_attack_awareness(SUBJECT, OWNER).unwrap();
            assert!(witness.cues.iter().all(|cue| cue.listener != OWNER));
            game.apply_attack_awareness(witness).unwrap();
            assert!(game.actors[&OWNER].awareness.alert.is_none());
            if sleeping {
                assert!(!game.condition_allows_voluntary_input(OWNER));
            } else {
                assert!(game.actors[&OWNER].guise.appearance().is_some());
            }
        }
    }
}

#[cfg(test)]
mod unavailable_attack_tests {
    use crate::perception::{
        contract_fixtures::{at, fixture_with, spawn, OWNER, SUBJECT},
        PerceptionValue,
    };

    #[test]
    fn paid_unavailable_primary_breaks_source_stealth_without_target_application_or_impact() {
        let mut game = fixture_with(
            &[],
            &[PerceptionValue::ContinualStealth { edge: 1 }],
            |_, owner, _| {
                owner.behavior.disposition = crate::behavior::Disposition::Mean;
            },
        );
        game.configure_ai(OWNER, crate::ai::AiProfile::default())
            .unwrap();
        let target = spawn(&mut game, "fixture_perception_subject", at(2, 3, 2));
        let outcome = crate::abilities::Outcome {
            actor: SUBJECT,
            target,
            ability: Some(crate::content_ids::AbilityId::new("basic_melee").unwrap()),
            intention: None,
            result: crate::abilities::OutcomeStatus::Miss,
            check: None,
            resistance: None,
            participation: crate::abilities::AttemptedParticipation::blocked(
                None,
                crate::abilities::UnselectedReason::UnavailableTarget,
            ),
            target_rider_participated: false,
            effects: vec![],
        };
        let target_before = game.actors[&target].clone();
        game.actors
            .get_mut(&OWNER)
            .unwrap()
            .awareness
            .contacts
            .clear();
        let witness = game
            .capture_unavailable_attack_awareness(SUBJECT, target)
            .unwrap();
        let origin = game.capture_event_origin(SUBJECT, target);
        let count = game.event_log.len();
        let receipt = game
            .publish_unavailable_primary(outcome.clone(), 0, origin)
            .unwrap();
        game.seal_participant_events().unwrap();
        assert_eq!(game.event_log.len(), count + 1);
        let mut forged = outcome.clone();
        forged.target = OWNER;
        let before = game.clone();
        let wrong_witness = game
            .capture_unavailable_attack_awareness(SUBJECT, OWNER)
            .unwrap();
        assert!(game
            .provoke_from_strike(&forged, receipt, wrong_witness)
            .is_err());
        assert_eq!(
            game, before,
            "publication mismatch must precede all side effects"
        );
        game.provoke_from_strike(&outcome, receipt, witness)
            .unwrap();
        assert!(game.actors[&OWNER].awareness.detected(SUBJECT));
        assert_eq!(game.actors[&target], target_before);
        assert!(!outcome.target_participated());
        assert!(outcome.effects.is_empty() && outcome.check.is_none());

        // A listener only at the unavailable remote target hears no fictional hit.
        game.configure_ai(target, crate::ai::AiProfile::default())
            .unwrap();
        let unavailable = game
            .capture_unavailable_attack_awareness(SUBJECT, target)
            .unwrap();
        assert!(unavailable.cues.iter().all(|cue| cue.listener != target));
    }

    #[test]
    fn ordinary_visible_crowd_does_not_expand_other_stealth_witnesses() {
        let mut game = fixture_with(&[], &[], |_, owner, _| {
            owner.behavior.disposition = crate::behavior::Disposition::Mean;
        });
        game.actors
            .get_mut(&OWNER)
            .unwrap()
            .combat
            .as_mut()
            .unwrap()
            .identity
            .faction = "observer".into();
        game.combat
            .hostility
            .insert("observer".into(), ["private affiliation".into()].into());
        game.configure_ai(OWNER, crate::ai::AiProfile::default())
            .unwrap();
        for x in 0..9 {
            spawn(&mut game, "fixture_perception_subject", at(1, x, 5));
        }
        let witness = game.capture_attack_awareness(SUBJECT, OWNER).unwrap();
        assert!(witness.cues.iter().all(|cue| cue.others.is_empty()));
    }
}

#[cfg(test)]
mod unavailable_execution_tests {
    use crate::perception::{
        contract_fixtures::{at, fixture_with, spawn, OWNER, SUBJECT},
        PerceptionValue,
    };

    #[test]
    fn actual_committed_primary_with_departed_target_emits_only_source_cue() {
        let mut game = fixture_with(
            &[],
            &[PerceptionValue::ContinualStealth { edge: 1 }],
            |_, owner, _| {
                owner.behavior.disposition = crate::behavior::Disposition::Mean;
            },
        );
        game.actors
            .get_mut(&OWNER)
            .unwrap()
            .combat
            .as_mut()
            .unwrap()
            .identity
            .faction = "observer".into();
        game.combat
            .hostility
            .insert("private affiliation".into(), ["observer".into()].into());
        let plan = game
            .ability_plan(
                SUBJECT,
                &crate::content_ids::AbilityId::new("basic_melee").unwrap(),
                OWNER,
            )
            .unwrap();
        let listener = spawn(&mut game, "fixture_perception_owner", at(1, 4, 2));
        game.configure_ai(listener, crate::ai::AiProfile::default())
            .unwrap();
        game.teleport(OWNER, at(2, 2, 2)).unwrap();
        game.actors
            .get_mut(&listener)
            .unwrap()
            .awareness
            .contacts
            .clear();
        let health = game.health(OWNER);
        let result = game
            .resolve_strike(
                plan.as_ability().unwrap(),
                0,
                None,
                &mut crate::combat::PreparationProgress::Ability {
                    next_strike: 1,
                    successful_primaries: 0,
                    attempted_riders: [0; 2],
                    linked_hold: None,
                },
            )
            .unwrap();
        assert_eq!(result.result, crate::abilities::OutcomeStatus::Miss);
        assert!(result.check.is_none() && result.effects.is_empty());
        assert!(!result.target_participated());
        assert_eq!(game.health(OWNER), health);
        assert!(game.actors[&listener].awareness.detected(SUBJECT));
        assert_eq!(
            game.actors[&listener].awareness.alert_at(game.tick),
            Some(at(1, 3, 2))
        );
    }
}
