//! Frozen current-batch disclosure. Reads consume receipts and never infer history
//! from today's senses. Transient witnesses are discarded on checkpoint restore.
use crate::combat::{CombatEvent, DisclosedCombatEvent};
use crate::perception::ActorReferenceKind;
use crate::{ActorId, Game, GameError, Observation};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use tor_world::Shared;

type Reference = Option<ActorReferenceKind>;

fn invalid() -> GameError {
    GameError::Rule(crate::abilities::RuleError::InvalidPlan)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum EventKind {
    Resolved,
    Escape,
    Respec,
    Sequence,
    Item,
    Interrupted,
    Died,
}

impl<'de> Deserialize<'de> for EventKind {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match String::deserialize(deserializer)?.as_str() {
            "resolved" => Ok(Self::Resolved),
            "escape" => Ok(Self::Escape),
            "respec" => Ok(Self::Respec),
            "sequence" => Ok(Self::Sequence),
            "item" => Ok(Self::Item),
            "interrupted" => Ok(Self::Interrupted),
            "died" => Ok(Self::Died),
            _ => Err(serde::de::Error::custom("invalid event kind")),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct EventKey {
    kind: EventKind,
    actor: ActorId,
    target: ActorId,
    participated: bool,
}
impl EventKey {
    fn capture(event: &CombatEvent) -> Self {
        let (kind, actor, target, participated) = match event {
            CombatEvent::Resolved { outcome, .. } => (
                EventKind::Resolved,
                outcome.actor,
                outcome.target,
                outcome.target_participated(),
            ),
            CombatEvent::EscapeResolved { actor, .. } => (EventKind::Escape, *actor, *actor, true),
            CombatEvent::RespecCompleted { actor, .. } => (EventKind::Respec, *actor, *actor, true),
            CombatEvent::SequenceEnded { actor, .. } => (EventKind::Sequence, *actor, *actor, true),
            CombatEvent::ItemCompleted { actor, .. } => (EventKind::Item, *actor, *actor, true),
            CombatEvent::Interrupted { actor, .. } => {
                (EventKind::Interrupted, *actor, *actor, true)
            }
            CombatEvent::Died { actor } => (EventKind::Died, *actor, *actor, true),
        };
        Self {
            kind,
            actor,
            target,
            participated,
        }
    }
}

/// One final candidate's pre-receipt witness, reused for its existing observer
/// pass. It contains no observation and grants no authority after a mutation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EventObservationWitness {
    generation: u64,
    through: usize,
    actors: u64,
    world: tor_world::PerceptionSnapshot,
}

/// Opaque provenance of a particular ordinary projection. Public Observation
/// fields are not evidence: only this internally constructed token can extend
/// a candidate's historical receipts. Tokens never enter saves or equality.
#[derive(Clone, Debug, Default)]
pub struct EventObservation {
    observer: Option<ActorId>,
    witness: Option<EventObservationWitness>,
    entries: Vec<EventReceipt>,
}
impl PartialEq for EventObservation {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}
impl Eq for EventObservation {}

/// At most the two actual participants' reference ceilings, captured before the
/// first mutation. Contact plans only narrow the source's target reference.
#[derive(Clone, Debug)]
pub(crate) struct EventOrigin {
    event: Option<EventKey>,
    actor: ActorId,
    target: ActorId,
    source_target: Reference,
    target_source: Reference,
    witness: EventObservationWitness,
    frozen: [bool; 2],
}
impl EventOrigin {
    pub(crate) fn cap_contact_target(&mut self) {
        self.source_target = self.source_target.map(|_| ActorReferenceKind::Contact);
    }
    fn reference(&self, observer: ActorId, subject: ActorId) -> Reference {
        if observer == subject {
            Some(ActorReferenceKind::Creature)
        } else if observer == self.actor && subject == self.target {
            self.source_target
        } else if observer == self.target && subject == self.actor {
            self.target_source
        } else {
            None
        }
    }
    fn binds(&self, key: EventKey) -> bool {
        if self.event != Some(key) {
            return false;
        }
        match key.kind {
            EventKind::Resolved => self.actor == key.actor && self.target == key.target,
            EventKind::Died => key.actor == self.actor || key.actor == self.target,
            _ => key.actor == self.actor || key.actor == self.target,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct EventOrigins {
    entries: Vec<Option<EventOrigin>>,
    /// Engine extension exists only inside the unpublished host candidate.
    open: bool,
    extension_start: usize,
}
impl PartialEq for EventOrigins {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}
impl Eq for EventOrigins {}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
struct EventReceipt {
    index: usize,
    #[serde(deserialize_with = "crate::bounded::named_map")]
    key: EventKey,
    #[serde(deserialize_with = "crate::bounded::required_option")]
    source: Reference,
    #[serde(deserialize_with = "crate::bounded::required_option")]
    target: Reference,
    participant: bool,
}

impl<'de> Deserialize<'de> for EventReceipt {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            index: usize,
            #[serde(deserialize_with = "crate::bounded::named_map")]
            key: EventKey,
            #[serde(deserialize_with = "crate::bounded::required_option")]
            source: Reference,
            #[serde(deserialize_with = "crate::bounded::required_option")]
            target: Reference,
            participant: bool,
        }
        let f: Fields = crate::bounded::named_map(d)?;
        Ok(Self {
            index: f.index,
            key: f.key,
            source: f.source,
            target: f.target,
            participant: f.participant,
        })
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
struct ObserverReceipts {
    generation: u64,
    through: usize,
    entries: Vec<EventReceipt>,
}
impl<'de> Deserialize<'de> for ObserverReceipts {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            generation: u64,
            through: usize,
            entries: crate::bounded::Bounded<EventReceipt, { crate::limits::EVENT_RECEIPTS }>,
        }
        let f: Fields = crate::bounded::named_map(d)?;
        Ok(Self {
            generation: f.generation,
            through: f.through,
            entries: f.entries.0,
        })
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct EventReceipts {
    generation: u64,
    sealed: usize,
    bindings: usize,
    observers: Shared<BTreeMap<ActorId, Shared<ObserverReceipts>>>,
}
impl<'de> Deserialize<'de> for EventReceipts {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            generation: u64,
            sealed: usize,
            bindings: usize,
            #[serde(deserialize_with = "read_observers")]
            observers: BTreeMap<ActorId, Shared<ObserverReceipts>>,
        }
        let f: Fields = crate::bounded::named_map(d)?;
        Ok(Self {
            generation: f.generation,
            sealed: f.sealed,
            bindings: f.bindings,
            observers: Shared::new(f.observers),
        })
    }
}
fn read_observers<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<BTreeMap<ActorId, Shared<ObserverReceipts>>, D::Error> {
    struct Visitor;
    impl<'de> serde::de::Visitor<'de> for Visitor {
        type Value = BTreeMap<ActorId, Shared<ObserverReceipts>>;
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("bounded unique event observers")
        }
        fn visit_map<A: serde::de::MapAccess<'de>>(
            self,
            mut map: A,
        ) -> Result<Self::Value, A::Error> {
            use serde::de::Error;
            let mut result = BTreeMap::new();
            let mut count = 0usize;
            while let Some((id, value)) = map.next_entry::<ActorId, Shared<ObserverReceipts>>()? {
                count = count
                    .checked_add(value.entries.len().max(1))
                    .ok_or_else(|| A::Error::custom("event receipts exceed limit"))?;
                if count > crate::limits::EVENT_RECEIPTS || result.insert(id, value).is_some() {
                    return Err(A::Error::custom("duplicate or excessive event receipts"));
                }
            }
            Ok(result)
        }
    }
    d.deserialize_map(Visitor)
}

impl Game {
    pub fn event_observation_witness(&self) -> EventObservationWitness {
        EventObservationWitness {
            generation: self.event_receipts.generation,
            through: self.event_log.len(),
            actors: self.actors.visibility_epoch(),
            world: self.world.perception_snapshot(),
        }
    }

    /// A disclosed limb can place a body without identifying the source of a
    /// historical event. Identity additionally needs the authoritative reference
    /// cell in the same ordinary scene; anonymous contact remains body-based.
    fn event_reference_in_scene(
        &self,
        observer: ActorId,
        subject: ActorId,
        classified: Reference,
        contains: impl FnOnce(tor_world::Location) -> bool,
    ) -> Reference {
        if observer == subject {
            return Some(ActorReferenceKind::Creature);
        }
        match classified {
            Some(ActorReferenceKind::Contact) => Some(ActorReferenceKind::Contact),
            Some(ActorReferenceKind::Creature) => self
                .actors
                .get(&subject)
                .filter(|state| contains(state.location))
                .map(|_| ActorReferenceKind::Creature),
            None => None,
        }
    }

    pub(crate) fn capture_event_origin(&self, actor: ActorId, target: ActorId) -> EventOrigin {
        let reference = |observer, subject| {
            if observer == subject {
                Some(ActorReferenceKind::Creature)
            } else if self.actor_frozen(observer) {
                // Crediting a frozen causal owner cannot acquire new senses or
                // activate its floor. Its personal outcome still has a receipt.
                None
            } else {
                if self.actor_frozen(subject) {
                    let owner = self.actors.get(&observer)?;
                    let other = self.actors.get(&subject)?;
                    // An active recipient may see a frozen source across a
                    // physical link. Only an isolated observer region plus an
                    // absent subject body proves Hidden without a scene.
                    if self.world.linked_regions(owner.location.region).is_empty()
                        && self
                            .body_cells(other.location, other.orientation, &other.body)
                            .is_none_or(|body| {
                                body.iter()
                                    .all(|(at, _)| at.region != owner.location.region)
                            })
                    {
                        return None;
                    }
                }
                let (scene, sight) = self.scene_snapshot(observer).ok()?;
                let classified = self
                    .actor_disclosure_in_scene(observer, subject, &scene, &sight)
                    .ok()?
                    .reference();
                self.event_reference_in_scene(observer, subject, classified, |at| {
                    scene.iter().any(|cell| cell.location == at)
                })
            }
        };
        let mut source_target = reference(actor, target);
        if self
            .preparation(actor)
            .and_then(|p| p.plan.as_ability())
            .is_some_and(|p| {
                p.actor == actor
                    && p.target == target
                    && matches!(
                        p.invocation,
                        crate::abilities::AbilityInvocation::ContactTarget { .. }
                    )
            })
        {
            // A frozen anonymous invocation cannot become an identity license.
            source_target = source_target.map(|_| ActorReferenceKind::Contact);
        }
        EventOrigin {
            event: None,
            actor,
            target,
            source_target,
            target_source: reference(target, actor),
            witness: self.event_observation_witness(),
            frozen: [self.actor_frozen(actor), self.actor_frozen(target)],
        }
    }

    /// Reuse only an unpublished origin with identical sensory inputs. Health
    /// settlement alone does not invalidate these inputs, but a rider that
    /// changes placement, life, guise or senses must capture its own new origin.
    /// Appending a preceding event is allowed; every clone still binds to its
    /// exact event key independently when published.
    pub(crate) fn reuse_event_origin(
        &self,
        actor: ActorId,
        target: ActorId,
        previous: Option<&EventOrigin>,
    ) -> EventOrigin {
        previous
            .filter(|origin| {
                origin.event.is_none()
                    && origin.actor == actor
                    && origin.target == target
                    && origin.witness.generation == self.event_receipts.generation
                    && origin.witness.actors == self.actors.visibility_epoch()
                    && origin.witness.world == self.world.perception_snapshot()
                    && origin.frozen == [self.actor_frozen(actor), self.actor_frozen(target)]
            })
            .cloned()
            .unwrap_or_else(|| self.capture_event_origin(actor, target))
    }

    pub(crate) fn begin_event_batch(&mut self) -> Result<(), GameError> {
        let generation = self
            .event_receipts
            .generation
            .checked_add(1)
            .ok_or_else(invalid)?;
        self.event_log.clear();
        self.event_receipts = EventReceipts {
            generation,
            ..Default::default()
        };
        self.event_origins = EventOrigins {
            entries: Vec::new(),
            open: self.host_region_continuation,
            extension_start: 0,
        };
        Ok(())
    }

    pub(crate) fn push_event(
        &mut self,
        event: CombatEvent,
        mut origin: Option<EventOrigin>,
    ) -> Result<(), GameError> {
        let key = EventKey::capture(&event);
        if let Some(origin) = origin.as_mut() {
            if origin.event.is_some_and(|bound| bound != key) {
                return Err(invalid());
            }
            origin.event = Some(key);
        }
        if self.event_log.len() >= crate::limits::EVENTS_PER_BATCH {
            return Err(invalid());
        }
        if self.event_origins.entries.len() < self.event_log.len() {
            self.event_origins
                .entries
                .resize(self.event_log.len(), None);
        }
        if self.event_receipts.generation == 0 {
            self.event_receipts.generation = 1;
        }
        self.event_log.push(event);
        self.event_origins.entries.push(origin);
        Ok(())
    }

    pub(crate) fn take_event_suffix(
        &mut self,
        start: usize,
    ) -> Result<(Vec<CombatEvent>, Vec<Option<EventOrigin>>), GameError> {
        if start < self.event_receipts.sealed || start > self.event_log.len() {
            return Err(invalid());
        }
        self.event_origins
            .entries
            .resize(self.event_log.len(), None);
        Ok((
            self.event_log.split_off(start),
            self.event_origins.entries.split_off(start),
        ))
    }

    pub(crate) fn append_event_suffix(
        &mut self,
        events: Vec<CombatEvent>,
        origins: Vec<Option<EventOrigin>>,
    ) -> Result<(), GameError> {
        if events.len() != origins.len() {
            return Err(invalid());
        }
        for (event, origin) in events.into_iter().zip(origins) {
            self.push_event(event, origin)?;
        }
        Ok(())
    }

    fn add_event_receipts(
        &mut self,
        observer: ActorId,
        entries: Vec<EventReceipt>,
        through: usize,
    ) -> Result<(), GameError> {
        if entries.is_empty() {
            return Ok(());
        }
        if through > self.event_log.len() {
            return Err(invalid());
        }
        let old = self.event_receipts.observers.get(&observer);
        if old.is_some_and(|row| row.generation != self.event_receipts.generation) {
            return Err(invalid());
        }
        let floor = if self.event_origins.open {
            self.event_origins.extension_start
        } else {
            old.map_or(0, |row| row.through)
        };
        if entries.iter().any(|entry| {
            entry.index < floor
                || old.is_some_and(|row| {
                    row.entries
                        .binary_search_by_key(&entry.index, |old| {
                            crate::diagnostics::event_receipt_binding_inspections(1);
                            old.index
                        })
                        .is_ok()
                })
        }) {
            return Err(invalid());
        }
        let total = self
            .event_receipts
            .bindings
            .checked_add(entries.len())
            .ok_or_else(invalid)?;
        if total > crate::limits::EVENT_RECEIPTS {
            return Err(invalid());
        }
        if self.event_receipts.observers.is_shared() {
            crate::diagnostics::event_receipt_cow_bindings(self.event_receipts.observers.len());
        }
        let generation = self.event_receipts.generation;
        let row = self
            .event_receipts
            .observers
            .entry(observer)
            .or_insert_with(|| {
                Shared::new(ObserverReceipts {
                    generation,
                    through: 0,
                    entries: Vec::new(),
                })
            });
        if row.is_shared() {
            crate::diagnostics::event_receipt_cow_bindings(row.entries.len());
        }
        if row
            .entries
            .last()
            .zip(entries.first())
            .is_none_or(|(old, new)| old.index < new.index)
        {
            row.entries.extend(entries);
        } else {
            // Only a host's once-only missing-observer extension can interleave
            // bindings with a participant prefix. Ordinary suffixes just append.
            crate::diagnostics::event_receipt_binding_inspections(
                row.entries.len() + entries.len(),
            );
            let old = std::mem::take(&mut row.entries);
            let mut merged = Vec::with_capacity(old.len() + entries.len());
            let mut old = old.into_iter().peekable();
            let mut new = entries.into_iter().peekable();
            while let (Some(a), Some(b)) = (old.peek(), new.peek()) {
                merged.push(if a.index < b.index {
                    old.next().unwrap()
                } else {
                    new.next().unwrap()
                });
            }
            merged.extend(old);
            merged.extend(new);
            row.entries = merged;
        }
        row.through = through;
        self.event_receipts.bindings = total;
        Ok(())
    }

    pub(crate) fn bind_event_participation(&mut self, index: usize) -> Result<(), GameError> {
        if self.event_is_sealed(index) {
            return Err(invalid());
        }
        let key = self
            .event_log
            .get(index)
            .map(EventKey::capture)
            .ok_or_else(invalid)?;
        if let Some(origin) = self
            .event_origins
            .entries
            .get_mut(index)
            .and_then(Option::as_mut)
        {
            let previous = origin.event.ok_or_else(invalid)?;
            if previous.kind != key.kind
                || previous.actor != key.actor
                || previous.target != key.target
            {
                return Err(invalid());
            }
            origin.event = Some(key);
        }
        Ok(())
    }

    pub(crate) fn event_observations_open(&self) -> bool {
        self.event_origins.open
    }

    pub(crate) fn event_is_sealed(&self, index: usize) -> bool {
        index < self.event_receipts.sealed
    }

    /// Complete owner/actual-participant feedback after all rider participation
    /// and event reordering are final. Public simulation transactions call this.
    pub(crate) fn seal_participant_events(&mut self) -> Result<(), GameError> {
        let start = self.event_receipts.sealed;
        if start > self.event_log.len() || self.event_log.len() > crate::limits::EVENTS_PER_BATCH {
            return Err(invalid());
        }
        let mut additions: BTreeMap<ActorId, Vec<EventReceipt>> = BTreeMap::new();
        for (index, event) in self.event_log.iter().enumerate().skip(start) {
            crate::diagnostics::event_receipt_binding_inspections(1);
            let key = EventKey::capture(event);
            let origin = self
                .event_origins
                .entries
                .get(index)
                .and_then(Option::as_ref)
                .filter(|o| o.binds(key));
            let mut owners = vec![key.actor];
            if key.kind == EventKind::Resolved && key.participated && key.target != key.actor {
                owners.push(key.target);
            }
            // A causal participant may retain its original anonymous death reference.
            if key.kind == EventKind::Died {
                if let Some(origin) = origin {
                    owners.extend([origin.actor, origin.target]);
                }
            }
            owners.sort();
            owners.dedup();
            for owner in owners {
                let source = if owner == key.actor {
                    Some(ActorReferenceKind::Creature)
                } else {
                    origin.and_then(|o| o.reference(owner, key.actor))
                };
                let target = if owner == key.target {
                    Some(ActorReferenceKind::Creature)
                } else {
                    origin.and_then(|o| o.reference(owner, key.target))
                };
                if key.kind == EventKind::Died && source.is_none() {
                    continue;
                }
                additions.entry(owner).or_default().push(EventReceipt {
                    index,
                    key,
                    source,
                    target,
                    participant: owner == key.actor || (owner == key.target && key.participated),
                });
            }
        }
        for (owner, entries) in additions {
            self.add_event_receipts(owner, entries, self.event_log.len())?;
        }
        self.event_receipts.sealed = self.event_log.len();
        if !self.host_region_continuation && !self.event_origins.open {
            self.event_origins.entries.clear();
        }
        Ok(())
    }

    /// Compute the ordinary view once and bind its event-time proof internally.
    /// This is still a pure read; only record_event_observation persists anything.
    pub fn observe_event_scene(
        &self,
        observer: ActorId,
    ) -> Result<(Observation, Vec<tor_world::SightCell>, EventObservation), GameError> {
        let (observation, scene) = self.observe_scene(observer)?;
        let mut proof = EventObservation::default();
        if self.event_origins.open {
            let witness = self.event_observation_witness();
            let locations: BTreeSet<_> = scene.iter().map(|cell| cell.location).collect();
            let reference = |subject| {
                let classified = observation
                    .visible_actors
                    .binary_search_by_key(&subject, |actor| actor.id)
                    .ok()
                    .and_then(|index| observation.visible_actors[index].disclosure.reference());
                self.event_reference_in_scene(observer, subject, classified, |at| {
                    locations.contains(&at)
                })
            };
            for (index, event) in self
                .event_log
                .iter()
                .enumerate()
                .skip(self.event_origins.extension_start)
            {
                crate::diagnostics::event_receipt_binding_inspections(1);
                let key = EventKey::capture(event);
                if !matches!(key.kind, EventKind::Resolved | EventKind::Died) {
                    continue;
                }
                let Some(origin) = self
                    .event_origins
                    .entries
                    .get(index)
                    .and_then(Option::as_ref)
                    .filter(|origin| origin.binds(key))
                else {
                    continue;
                };
                if origin.witness.actors != witness.actors
                    || origin.witness.world != witness.world
                    || origin.witness.generation != witness.generation
                {
                    continue;
                }
                let source = reference(key.actor);
                let target = reference(key.target);
                if key.kind == EventKind::Resolved
                    && (source != Some(ActorReferenceKind::Creature)
                        || target != Some(ActorReferenceKind::Creature))
                {
                    continue;
                }
                if key.kind == EventKind::Died && source.is_none() {
                    continue;
                }
                proof.entries.push(EventReceipt {
                    index,
                    key,
                    source,
                    target,
                    participant: observer == key.actor
                        || (observer == key.target && key.participated),
                });
                if proof.entries.len() > crate::limits::EVENT_RECEIPTS {
                    return Err(invalid());
                }
            }
            proof.observer = Some(observer);
            proof.witness = Some(witness);
        }
        Ok((observation, scene, proof))
    }

    pub fn event_observation_pending(&self, proof: &EventObservation) -> bool {
        if !self.event_origins.open {
            return false;
        }
        let Some(observer) = proof.observer else {
            return false;
        };
        let row = self.event_receipts.observers.get(&observer);
        proof.entries.iter().any(|entry| {
            !row.is_some_and(|row| {
                row.entries
                    .binary_search_by_key(&entry.index, |old| {
                        crate::diagnostics::event_receipt_binding_inspections(1);
                        old.index
                    })
                    .is_ok()
            })
        })
    }

    /// Extend only an unpublished candidate with an internally bound ordinary
    /// projection. No caller-populated actor field licenses a reference.
    pub fn record_event_observation(
        &mut self,
        witness: &EventObservationWitness,
        proof: &EventObservation,
        observation: &mut Observation,
    ) -> Result<(), GameError> {
        if !self.event_origins.open {
            return Ok(());
        }
        if *witness != self.event_observation_witness()
            || proof.witness.as_ref() != Some(witness)
            || proof.observer != Some(observation.actor)
            || observation.tick != self.tick
            || !self.has_actor(observation.actor)
        {
            return Err(invalid());
        }
        let observer = observation.actor;
        let row = self.event_receipts.observers.get(&observer);
        let additions = proof
            .entries
            .iter()
            .copied()
            .filter(|entry| {
                !row.is_some_and(|row| {
                    row.entries
                        .binary_search_by_key(&entry.index, |old| {
                            crate::diagnostics::event_receipt_binding_inspections(1);
                            old.index
                        })
                        .is_ok()
                })
            })
            .collect();
        self.add_event_receipts(observer, additions, self.event_log.len())?;
        if let Some(combat) = observation.combat.as_mut() {
            combat.events = self.disclosed_event_history(observer);
        }
        Ok(())
    }

    /// End host-only extension before persistence/publication. Missing observers
    /// remain missing permanently; later reads cannot reconstruct an old event.
    pub fn seal_event_observations(&mut self) -> Result<(), GameError> {
        self.seal_participant_events()?;
        self.event_origins = EventOrigins::default();
        Ok(())
    }

    pub(crate) fn owns_event_receipt(&self, observer: ActorId, index: usize) -> bool {
        let Some(row) =
            self.event_receipts.observers.get(&observer).filter(|row| {
                row.generation == self.event_receipts.generation && index < row.through
            })
        else {
            return false;
        };
        let Ok(position) = row.entries.binary_search_by_key(&index, |entry| {
            crate::diagnostics::event_receipt_binding_inspections(1);
            entry.index
        }) else {
            return false;
        };
        let receipt = row.entries[position];
        receipt.participant
            && receipt.key.actor == observer
            && self.event_log.get(index).map(EventKey::capture) == Some(receipt.key)
    }

    /// Exact causal reference for the actual recipient of one sealed resolved
    /// event. A projected batch search cannot distinguish anonymous primaries
    /// or separate publications that legitimately reuse the same strike ordinal.
    pub(crate) fn event_target_source_reference(
        &self,
        observer: ActorId,
        index: usize,
    ) -> Reference {
        if index >= self.event_receipts.sealed {
            return None;
        }
        let row = self.event_receipts.observers.get(&observer).filter(|row| {
            row.generation == self.event_receipts.generation
                && index < row.through
                && row.through <= self.event_log.len()
        })?;
        let position = row
            .entries
            .binary_search_by_key(&index, |entry| {
                crate::diagnostics::event_receipt_binding_inspections(1);
                entry.index
            })
            .ok()?;
        let receipt = row.entries[position];
        (receipt.participant
            && receipt.key.kind == EventKind::Resolved
            && receipt.key.target == observer
            && receipt.key.participated
            && self.event_log.get(index).map(EventKey::capture) == Some(receipt.key))
        .then_some(receipt.source)
        .flatten()
    }

    pub(crate) fn disclosed_event_history(&self, observer: ActorId) -> Vec<DisclosedCombatEvent> {
        let Some(row) = self.event_receipts.observers.get(&observer).filter(|row| {
            row.generation == self.event_receipts.generation && row.through <= self.event_log.len()
        }) else {
            return Vec::new();
        };
        row.entries
            .iter()
            .filter_map(|receipt| {
                crate::diagnostics::event_receipt_binding_inspections(1);
                if receipt.index >= row.through {
                    return None;
                }
                let event = self.event_log.get(receipt.index)?;
                if receipt.key != EventKey::capture(event) {
                    return None;
                }
                match event {
                    CombatEvent::Resolved { outcome, .. }
                        if outcome.ability.is_none()
                            && outcome.actor == observer
                            && outcome.target == observer
                            && outcome.result == crate::abilities::OutcomeStatus::Applied =>
                    {
                        let crate::abilities::ResolvedEffect::ApplyCondition { id, .. } =
                            outcome.effects.first()?
                        else {
                            return None;
                        };
                        let definition = self.content.rules.condition(id)?;
                        Some(DisclosedCombatEvent::Condition {
                            id: id.clone(),
                            name: definition.name.clone(),
                        })
                    }
                    CombatEvent::Resolved { outcome, strike } => {
                        let ability = outcome.ability.as_ref()?;
                        if !receipt.participant
                            && (receipt.source != Some(ActorReferenceKind::Creature)
                                || receipt.target != Some(ActorReferenceKind::Creature))
                        {
                            return None;
                        }
                        if outcome.actor != observer
                            && receipt.source != Some(ActorReferenceKind::Creature)
                        {
                            return (outcome.target == observer && outcome.target_participated())
                                .then_some(DisclosedCombatEvent::UnseenAbility {
                                    caster: receipt.source.map(|_| outcome.actor),
                                    target: observer,
                                    outcome: outcome.result,
                                });
                        }
                        let definition = self.content.rules.ability(ability)?;
                        Some(DisclosedCombatEvent::Ability {
                            caster: receipt.source.map(|_| outcome.actor),
                            caster_reference: receipt.source,
                            target: receipt.target.map(|_| outcome.target),
                            target_reference: receipt.target,
                            id: ability.clone(),
                            name: definition.name.clone(),
                            outcome: outcome.result,
                            strike: *strike,
                        })
                    }
                    CombatEvent::EscapeResolved {
                        actor,
                        hold,
                        success,
                        ..
                    } if *actor == observer => Some(DisclosedCombatEvent::EscapeResolved {
                        hold: *hold,
                        success: *success,
                    }),
                    CombatEvent::Interrupted { actor, .. } if *actor == observer => {
                        Some(DisclosedCombatEvent::Interrupted { actor: *actor })
                    }
                    CombatEvent::Died { actor } => {
                        receipt.source.map(|reference| DisclosedCombatEvent::Died {
                            actor: *actor,
                            reference,
                        })
                    }
                    _ => None,
                }
            })
            .collect()
    }

    pub(crate) fn event_receipts_valid(&self) -> bool {
        if (!self.event_log.is_empty() && self.event_receipts.generation == 0)
            || self.event_receipts.sealed != self.event_log.len()
            || self.event_log.len() > crate::limits::EVENTS_PER_BATCH
        {
            return false;
        }
        let mut total = 0usize;
        for (owner, row) in self.event_receipts.observers.iter() {
            if owner.0 == 0
                || owner.0 >= self.next_actor_id
                || (!self.has_actor(*owner) && !self.detached_actor(*owner))
                || row.generation != self.event_receipts.generation
                || row.through > self.event_receipts.sealed
                || row.entries.is_empty()
            {
                return false;
            }
            total = match total.checked_add(row.entries.len()) {
                Some(n) if n <= crate::limits::EVENT_RECEIPTS => n,
                _ => return false,
            };
            let mut previous = None;
            for receipt in &row.entries {
                crate::diagnostics::event_receipt_binding_inspections(1);
                let key = receipt.key;
                if receipt.index >= row.through
                    || previous.is_some_and(|index| index >= receipt.index)
                    || self.event_log.get(receipt.index).map(EventKey::capture) != Some(key)
                    || key.actor.0 == 0
                    || key.actor.0 >= self.next_actor_id
                    || key.target.0 == 0
                    || key.target.0 >= self.next_actor_id
                {
                    return false;
                }
                previous = Some(receipt.index);
                if receipt.participant
                    != (*owner == key.actor || (*owner == key.target && key.participated))
                {
                    return false;
                }
                if (*owner == key.actor && receipt.source != Some(ActorReferenceKind::Creature))
                    || (*owner == key.target
                        && receipt.target != Some(ActorReferenceKind::Creature))
                {
                    return false;
                }
                if !receipt.participant
                    && match key.kind {
                        EventKind::Resolved => {
                            receipt.source != Some(ActorReferenceKind::Creature)
                                || receipt.target != Some(ActorReferenceKind::Creature)
                        }
                        EventKind::Died => receipt.source.is_none(),
                        _ => true,
                    }
                {
                    return false;
                }
                if key.actor == key.target && receipt.source != receipt.target {
                    return false;
                }
            }
        }
        total == self.event_receipts.bindings
    }
}

#[cfg(test)]
mod tests;
