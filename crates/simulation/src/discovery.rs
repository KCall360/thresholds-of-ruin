//! Actor-owned physical discovery. Queries never create attempts or consume RNG.
//!
//! Presence and family knowledge are separate from physical geometry. First-eye
//! receipts survive reentry, blindness, control transfer and save restoration.
use crate::{ActorId, Game, GameError};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use tor_world::{ConcealedFeatureId, Shared};

/// Allocation guard, not a forgetting policy: old failed attempts are never evicted.
const MAX_DISCOVERY_FACTS: usize = 262_144;

#[cfg(test)]
thread_local! {
    // Count the candidate scan even on a warm scene cache: LOS counts alone miss
    // the cloned sight vectors and per-cell feature lookup work.
    static DISCOVERY_WORK: std::cell::Cell<(usize, usize)> = const { std::cell::Cell::new((0, 0)) };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
struct DiscoveryFact {
    feature: ConcealedFeatureId,
    attempted: bool,
    presence: bool,
    family: bool,
}
impl<'de> Deserialize<'de> for DiscoveryFact {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            feature: ConcealedFeatureId,
            attempted: bool,
            presence: bool,
            family: bool,
        }
        let f: Fields = crate::bounded::named_map(d)?;
        let value = Self {
            feature: f.feature,
            attempted: f.attempted,
            presence: f.presence,
            family: f.family,
        };
        if !value.valid() {
            return Err(serde::de::Error::custom("invalid discovery fact"));
        }
        Ok(value)
    }
}
impl DiscoveryFact {
    fn valid(self) -> bool {
        let id = match self.feature {
            ConcealedFeatureId::Door(id) | ConcealedFeatureId::Trap(id) => id,
        };
        id != 0
            && id != u64::MAX
            && (self.attempted || self.presence)
            && (!self.family || self.presence)
            && (!self.family || matches!(self.feature, ConcealedFeatureId::Trap(_)))
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub(crate) struct DiscoveryKnowledge(Vec<DiscoveryFact>);
impl DiscoveryKnowledge {
    pub(crate) fn features(&self) -> impl Iterator<Item = ConcealedFeatureId> + '_ {
        self.0.iter().map(|fact| fact.feature)
    }
    fn index(&self, feature: ConcealedFeatureId) -> Result<usize, usize> {
        self.0.binary_search_by_key(&feature, |fact| fact.feature)
    }
    fn fact(&self, feature: ConcealedFeatureId) -> Option<&DiscoveryFact> {
        self.index(feature).ok().map(|index| &self.0[index])
    }
    fn update(
        &mut self,
        feature: ConcealedFeatureId,
        attempted: bool,
        presence: bool,
        family: bool,
    ) -> Result<(), GameError> {
        let index = match self.index(feature) {
            Ok(index) => index,
            Err(index) => {
                if self.0.len() >= MAX_DISCOVERY_FACTS {
                    return Err(GameError::Rule(crate::abilities::RuleError::NumericBudget));
                }
                self.0.insert(
                    index,
                    DiscoveryFact {
                        feature,
                        attempted: false,
                        presence: false,
                        family: false,
                    },
                );
                index
            }
        };
        let fact = &mut self.0[index];
        fact.attempted |= attempted;
        fact.presence |= presence;
        fact.family |= family;
        Ok(())
    }
    pub(crate) fn valid(&self) -> bool {
        self.0.len() <= MAX_DISCOVERY_FACTS
            && self.0.iter().all(|fact| fact.valid())
            && self
                .0
                .windows(2)
                .all(|pair| pair[0].feature < pair[1].feature)
    }
}
pub(crate) fn read_discovery<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Shared<DiscoveryKnowledge>, D::Error> {
    let value = DiscoveryKnowledge(
        crate::bounded::Bounded::<DiscoveryFact, MAX_DISCOVERY_FACTS>::deserialize(d)?.0,
    );
    if !value.valid() {
        return Err(serde::de::Error::custom("invalid discovery knowledge"));
    }
    Ok(Shared::new(value))
}

/// Actor operands captured when paid preparation starts. Candidate geometry and
/// DCs remain live at completion, and no undiscovered feature appears in a plan.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FrozenDiscoveryCheck {
    pub bonus: i64,
    pub edge: i64,
}
impl FrozenDiscoveryCheck {
    pub(crate) fn capture_for(
        creature: &crate::creatures::CreatureState,
        actor: ActorId,
    ) -> Result<Self, GameError> {
        let derived = creature.derived();
        Ok(Self {
            bonus: crate::attributes::SkillCheck {
                skill: crate::attributes::Skill::Perception,
                binding: derived.binding(),
                modifier: 0,
                threshold: 0,
            }
            .bonus(derived.attributes, derived.skills),
            edge: derived
                .discovery_edge(
                    creature.build().ruleset(),
                    actor,
                    None,
                    &[],
                    crate::rules::OutgoingEdgePolicy::Ordinary,
                )
                .map_err(GameError::Build)?,
        })
    }
    pub(crate) fn valid(self) -> bool {
        self.bonus.unsigned_abs() <= crate::limits::NUMERIC as u64
            && self.edge.unsigned_abs() <= crate::limits::NUMERIC as u64
    }
    pub(crate) fn roll(self, rng: &mut u64, passive: bool) -> i64 {
        let balance = self.edge - i64::from(passive);
        let edge = if balance >= 0 {
            crate::dice::Edge::from_counts(balance as u32, 0)
        } else {
            crate::dice::Edge::from_counts(0, balance.unsigned_abs() as u32)
        };
        i64::from(crate::dice::roll_check(rng, edge).kept) + self.bonus
    }
}
impl<'de> Deserialize<'de> for FrozenDiscoveryCheck {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            bonus: i64,
            edge: i64,
        }
        let f: Fields = crate::bounded::named_map(d)?;
        let value = Self {
            bonus: f.bonus,
            edge: f.edge,
        };
        if !value.valid() {
            return Err(serde::de::Error::custom("invalid discovery operands"));
        }
        Ok(value)
    }
}

#[derive(Clone, Debug)]
struct DiscoveryWitness {
    world: tor_world::PerceptionSnapshot,
    actors: u64,
    eligible: BTreeSet<ActorId>,
    observers: BTreeMap<ActorId, crate::observation::SceneWitness>,
}
#[derive(Clone, Debug, Default)]
pub(crate) struct DiscoveryContexts(Shared<Option<DiscoveryWitness>>);
impl PartialEq for DiscoveryContexts {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}
impl Eq for DiscoveryContexts {}

impl Game {
    pub fn feature_known(&self, actor: ActorId, feature: ConcealedFeatureId) -> bool {
        self.actors
            .get(&actor)
            .and_then(|actor| actor.discovery.fact(feature))
            .is_some_and(|fact| fact.presence)
    }
    pub fn feature_family_known(&self, actor: ActorId, feature: ConcealedFeatureId) -> bool {
        self.actors
            .get(&actor)
            .and_then(|actor| actor.discovery.fact(feature))
            .is_some_and(|fact| fact.family)
    }
    /// Trusted activation or an authorized adjacent examination grants the family.
    pub(crate) fn reveal_feature_family(
        &mut self,
        actor: ActorId,
        feature: ConcealedFeatureId,
    ) -> Result<(), GameError> {
        if !matches!(feature, ConcealedFeatureId::Trap(_)) || !self.knows_concealed_feature(feature)
        {
            return Err(GameError::InvalidLocation);
        }
        let owner = self.actors.get(&actor).ok_or(GameError::UnknownActor)?;
        let mut knowledge = owner.discovery.clone();
        knowledge.update(feature, false, true, true)?;
        self.actors
            .get_mut(&actor)
            .expect("validated observer")
            .discovery = knowledge;
        Ok(())
    }
    /// Automatic scans exclude one-use check tokens, including somebody else's
    /// reserved paid check. Timed, equipment and talent edges use the ordinary fold.
    pub(crate) fn capture_discovery_check(
        &self,
        actor: ActorId,
    ) -> Result<FrozenDiscoveryCheck, GameError> {
        let creature = self.creature(actor).ok_or(GameError::UnknownActor)?;
        FrozenDiscoveryCheck::capture_for(&creature, actor)
    }
    fn discovery_eligible(&self, actor: ActorId) -> bool {
        self.actors.get(&actor).is_some_and(|owner| {
            owner.alive() && owner.combat.is_some() && owner.perception().sight
        }) && !self.actor_frozen(actor)
            && self.condition_allows_voluntary_input(actor)
    }
    /// Eye sight only: awareness, contact and abstract stair landing projection do
    /// not license a concealed exterior cue. Alias/body cells deduplicate by ID.
    fn discovery_candidates(
        &self,
        actor: ActorId,
    ) -> Result<BTreeMap<ConcealedFeatureId, Option<i32>>, GameError> {
        let mut candidates = BTreeMap::new();
        if !self.discovery_eligible(actor) {
            return Ok(candidates);
        }
        let (_, sight) = self.scene_snapshot(actor)?;
        #[cfg(test)]
        DISCOVERY_WORK.with(|count| {
            let (scans, cells) = count.get();
            count.set((scans + 1, cells + sight.len()));
        });
        let cells: BTreeSet<_> = sight
            .iter()
            .filter(|cell| cell.physical)
            .map(|cell| cell.location)
            .collect();
        for at in cells {
            if let (Some(concealment), Some(door)) =
                (self.world.secret_door(at), self.world.door(at))
            {
                candidates.insert(
                    ConcealedFeatureId::Door(door.id),
                    (!door.open).then_some(concealment.dc),
                );
            }
            if let Some(trap) = self.world.trap(at) {
                candidates.insert(ConcealedFeatureId::Trap(trap.id), Some(trap.dc));
            }
        }
        Ok(candidates)
    }
    /// One authoritative actor boundary. Failed first observations persist too;
    /// none of the observation/snapshot/render paths call this mutating method.
    pub(crate) fn reconcile_passive_discovery_for(
        &mut self,
        actor: ActorId,
    ) -> Result<(), GameError> {
        let candidates = self.discovery_candidates(actor)?;
        if candidates.is_empty() {
            return Ok(());
        }
        let check = self.capture_discovery_check(actor)?;
        let mut knowledge = self.actors[&actor].discovery.clone();
        let mut rng = self.rules_rng;
        for (feature, dc) in candidates {
            let Some(dc) = dc else {
                // An exposed open entrance is no longer concealed. Its identity
                // remains learned when somebody closes the shared door later.
                knowledge.update(feature, false, true, false)?;
                continue;
            };
            if knowledge
                .fact(feature)
                .is_some_and(|fact| fact.attempted || fact.presence)
            {
                continue;
            }
            let success = check.roll(&mut rng, true) >= i64::from(dc);
            knowledge.update(feature, true, success, false)?;
        }
        self.actors
            .get_mut(&actor)
            .expect("validated observer")
            .discovery = knowledge;
        self.rules_rng = rng;
        Ok(())
    }
    /// Authoritative environment changes may affect every loaded active observer.
    pub(crate) fn reconcile_passive_discovery(&mut self) -> Result<(), GameError> {
        if !self.lifecycle.discovery_started || !self.world.has_concealed_features() {
            return Ok(());
        }
        let eligible: BTreeSet<_> = self
            .actors
            .keys()
            .copied()
            .filter(|actor| self.discovery_eligible(*actor))
            .collect();
        let world = self.world.perception_snapshot();
        let actors = self.actors.visibility_epoch();
        if self
            .lifecycle
            .discovery_contexts
            .0
            .as_ref()
            .is_some_and(|old| {
                old.world == world && old.actors == actors && old.eligible == eligible
            })
        {
            return Ok(());
        }
        // Actor movement does not occlude eye sight: concealed physical cues
        // depend on this observer's chart/body/sight and shared world geometry
        // and illumination. Reuse observation's complete witness so another
        // actor's visibility change cannot force warm sight-vector clones and
        // feature lookups for every unchanged observer.
        let old = self.lifecycle.discovery_contexts.0.as_ref();
        let mut observers = BTreeMap::new();
        let mut changed = Vec::new();
        for actor in &eligible {
            let witness = self.scene_witness(*actor)?;
            if old.is_none_or(|old| old.observers.get(actor) != Some(&witness)) {
                changed.push(*actor);
            }
            observers.insert(*actor, witness);
        }
        // Rebuilding from eligible loaded actors also drops sleep/freeze/death
        // and detached entries: waking/thawing must reconcile deferred cues.
        let witness = DiscoveryWitness {
            world,
            actors,
            eligible,
            observers,
        };
        if changed.is_empty() {
            self.lifecycle.discovery_contexts.0 = Shared::new(Some(witness));
            return Ok(());
        }
        let mut candidate = self.clone();
        for actor in changed {
            candidate.reconcile_passive_discovery_for(actor)?;
        }
        candidate.lifecycle.discovery_contexts.0 = Shared::new(Some(witness));
        *self = candidate;
        Ok(())
    }
    /// Initial scenario publication owns its first observation attempts. Hosts
    /// call once after complete construction, never during a query or restoration.
    pub fn initialize_discovery(&mut self) -> Result<(), GameError> {
        if self.lifecycle.discovery_started {
            return self.reconcile_passive_discovery();
        }
        let mut candidate = self.clone();
        candidate.lifecycle.discovery_started = true;
        candidate.reconcile_passive_discovery()?;
        *self = candidate;
        Ok(())
    }
    /// Apply an already resolved active Search total against live physical cues.
    /// The caller owns the ordinary action's RNG transaction and token lifecycle.
    pub(crate) fn apply_discovery_total(
        &mut self,
        actor: ActorId,
        total: i64,
    ) -> Result<(), GameError> {
        let candidates = self.discovery_candidates(actor)?;
        let mut knowledge = self
            .actors
            .get(&actor)
            .ok_or(GameError::UnknownActor)?
            .discovery
            .clone();
        for (feature, dc) in candidates {
            if dc.is_none_or(|dc| total >= i64::from(dc)) {
                knowledge.update(feature, false, true, false)?;
            }
        }
        self.actors
            .get_mut(&actor)
            .expect("validated observer")
            .discovery = knowledge;
        Ok(())
    }
    /// The periodic ring runs the active algorithm without an action or token.
    pub(crate) fn run_active_discovery(
        &mut self,
        actor: ActorId,
        check: &FrozenDiscoveryCheck,
    ) -> Result<(), GameError> {
        if !self.discovery_eligible(actor) {
            return Ok(());
        }
        let mut candidate = self.clone();
        let mut rng = candidate.rules_rng;
        let total = check.roll(&mut rng, false);
        candidate.reveal_adjacent_guises(actor)?;
        candidate.apply_discovery_total(actor, total)?;
        candidate.rules_rng = rng;
        *self = candidate;
        Ok(())
    }
}

#[cfg(test)]
#[path = "discovery_tests.rs"]
mod tests;
