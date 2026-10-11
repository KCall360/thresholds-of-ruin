//! Deterministic combat values. These are authoritative rules, never wire views.
use crate::{ActorId, Game, GameError, Item, ItemId, ItemLocation, ItemSpec};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

// Nullable context is explicit for trusted direct rule calls. Requiring the
// field prevents silently decoding progress from a different saved format.
fn deserialize_intention_context<'de, D>(
    deserializer: D,
) -> Result<Option<crate::IntentionId>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Option::<crate::IntentionId>::deserialize(deserializer)
}

pub(crate) use crate::actor_creatures::CombatState;

fn deserialize_charge<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<crate::costs::ResourceCost>, D::Error> {
    Option::deserialize(deserializer)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Preparation {
    /// Backend admission identity; assigned before wind-up can advance.
    #[serde(deserialize_with = "deserialize_intention_context")]
    pub intention: Option<crate::IntentionId>,
    /// Original admission owns reserved costs even when a new receipt resumes
    /// the same work. Trusted direct calls explicitly have no admission owner.
    #[serde(deserialize_with = "deserialize_intention_context")]
    pub(crate) origin_intention: Option<crate::IntentionId>,
    pub work: crate::Work,
    pub threats: BTreeSet<ActorId>,
    /// Timing captured when this work first starts; resume never retimes it.
    pub duration: u64,
    pub recovery: u64,
    #[serde(deserialize_with = "deserialize_charge")]
    pub charge: Option<crate::costs::ResourceCost>,
    pub remaining: u64,
    pub started: u64,
    pub active: bool,
}

impl Preparation {
    /// Identity of the admission that first started this preparation.
    pub fn origin_intention(&self) -> Option<crate::IntentionId> {
        self.origin_intention
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CombatWorld {
    #[serde(deserialize_with = "Option::deserialize")]
    pub arena: Option<crate::arena::ArenaRun>,
    run_mode: RunMode,
    rng: u64,
    pub events: Vec<CombatEvent>,
    pub selected: Option<ActorId>,
    pub characters: BTreeSet<ActorId>,
    pub objective: Option<Objective>,
    pub outcome: RunOutcome,
    pub hostility: BTreeMap<String, BTreeSet<String>>,
    pub input_boundaries: BTreeSet<ActorId>,
    pub ai: BTreeMap<ActorId, crate::ai::Ai>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum RunMode {
    Adventure,
    Arena,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Objective {
    pub anchor: tor_world::Location,
    pub item: Option<ItemId>,
    pub disclosed: bool,
    pub continue_play: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunOutcome {
    pub victor: Option<ActorId>,
    pub deceased: Option<ActorId>,
    pub terminal: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CombatView {
    /// Exact creature information belongs only to the observer's own actor.
    pub own_stats: Option<crate::creatures::OwnStats>,
    pub hp: u32,
    pub max_hp: u32,
    pub preparation_remaining: Option<u64>,
    pub preparation_active: bool,
    pub recovery_remaining: u64,
    pub actors: Vec<(ActorId, bool, Injury)>,
    /// What the action this view follows did, as far as the observer knows.
    pub events: Vec<DisclosedCombatEvent>,
    pub objective: Option<ObjectiveKind>,
    /// Where the objective is met, when the objective is disclosed.
    pub exit: Option<tor_world::Location>,
    pub victory: bool,
    pub dead: bool,
    pub terminal: bool,
}

/// How hurt a visible actor looks; never its numbers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Injury {
    Healthy,
    Wounded,
    BadlyWounded,
    NearDeath,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObjectiveKind {
    /// Bring the objective item back to the exit.
    RetrieveAndReturn,
    ReachExit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AttackOutcome {
    Miss,
    /// Struck, but every damage component was resisted.
    NoInjury,
    Hit,
}

/// A combat event as one observer may know it. `None` is a participant it
/// couldn't see.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DisclosedCombatEvent {
    Ability {
        caster: Option<ActorId>,
        target: Option<ActorId>,
        ability: crate::grants::Ability,
        outcome: AbilityOutcome,
    },
    Attack {
        attacker: Option<ActorId>,
        target: Option<ActorId>,
        outcome: AttackOutcome,
    },
    Interrupted {
        actor: ActorId,
    },
    Died {
        actor: ActorId,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AbilityOutcome {
    Applied,
    Unaffected,
    Miss,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CombatEvent {
    AbilityResolved {
        #[serde(deserialize_with = "deserialize_intention_context")]
        intention: Option<crate::IntentionId>,
        actor: ActorId,
        target: ActorId,
        ability: crate::grants::Ability,
        applied: bool,
        damage: u32,
    },
    ItemCompleted {
        actor: ActorId,
        work: crate::Work,
        intention: Option<crate::IntentionId>,
    },
    Resolved {
        #[serde(deserialize_with = "deserialize_intention_context")]
        intention: Option<crate::IntentionId>,
        actor: ActorId,
        target: ActorId,
        hit: bool,
        damage: u32,
    },
    Interrupted {
        #[serde(deserialize_with = "deserialize_intention_context")]
        intention: Option<crate::IntentionId>,
        actor: ActorId,
    },
    Died {
        actor: ActorId,
    },
}

impl CombatWorld {
    pub(crate) fn resolve_creature_fear(
        &mut self,
        caster: &crate::creatures::CreatureState,
        defender: &crate::creatures::CreatureState,
        protection: &crate::damage::Protection,
        edge: crate::dice::Edge,
        observer: Option<&mut crate::resolution_diagnostics::ResolutionTrace>,
    ) -> crate::abilities::FearResolution {
        match observer {
            Some(observer) => crate::abilities::resolve_fear_with_diagnostics(
                caster,
                defender,
                protection,
                &mut self.rng,
                edge,
                observer,
            ),
            None => {
                crate::abilities::resolve_fear(caster, defender, protection, &mut self.rng, edge)
            }
        }
        .expect("validated fear resolution")
    }
    pub(crate) fn resolve_creature_attack(
        &mut self,
        check: crate::damage::AttackCheck,
        edge: crate::dice::Edge,
        damage: &crate::damage::DamageSpec,
        protection: &crate::damage::Protection,
        observer: Option<&mut crate::resolution_diagnostics::ResolutionTrace>,
    ) -> crate::damage::AttackOutcome {
        match observer {
            Some(observer) => {
                check.resolve_with_diagnostics(&mut self.rng, edge, damage, protection, observer)
            }
            None => check.resolve(&mut self.rng, edge, damage, protection),
        }
    }
    pub fn new(seed: u64) -> Self {
        Self {
            run_mode: RunMode::Adventure,
            arena: None,
            rng: seed,
            events: Vec::new(),
            selected: None,
            characters: BTreeSet::new(),
            objective: None,
            outcome: RunOutcome::default(),
            hostility: BTreeMap::new(),
            input_boundaries: BTreeSet::new(),
            ai: BTreeMap::new(),
        }
    }
}

impl Game {
    pub(crate) fn combat_valid(&self) -> bool {
        // Characters may be detached, or not built yet, like anyone else;
        // reference points and pins, not these checks, decide what's loaded.
        // (A pin keeps every actor awaiting input active once a transition
        // has run.)
        let known = |id: &ActorId| self.actors.contains_key(id) || self.detached_actor(*id);
        self.combat.input_boundaries.iter().all(known)
            && self
                .combat
                .selected
                .is_none_or(|id| known(&id) && self.combat.characters.contains(&id))
            && self.combat.characters.iter().all(known)
            && self.combat.objective.as_ref().is_none_or(|o| {
                self.world.knows(o.anchor)
                    // An objective item can have been consumed. Keep its issued
                    // identity so the objective remains unmet, without requiring
                    // a live stack or permitting a future/unissued identity.
                    && o.item.is_none_or(|id| id.0 > 0 && id.0 < self.next_item_id)
            })
            && self
                .combat
                .outcome
                .victor
                .is_none_or(|id| self.combat.characters.contains(&id))
            && self
                .combat
                .outcome
                .deceased
                .is_none_or(|id| self.combat.selected == Some(id) && !self.alive(id))
            && match self.combat.run_mode {
                RunMode::Adventure => {
                    self.combat.arena.is_none()
                        && (self.combat.outcome.deceased.is_none() || self.combat.outcome.terminal)
                }
                RunMode::Arena => {
                    self.combat.objective.is_none()
                        && self.combat.outcome.victor.is_none()
                        && self.arena_state_valid()
                }
            }
            && self.actors.iter().all(|(id, actor)| {
                self.work_state_valid(*id) && actor.combat.as_ref().is_none_or(|c| c.valid())
            })
            && self.combat.ai.iter().all(|(id, ai)| {
                self.health(*id).is_some()
                    && ai.profile.valid()
                    // Memory may refer to detached actors and places.
                    && ai.target.is_none_or(|(target, at, tick)| {
                        (self.actors.contains_key(&target) || self.detached_actor(target))
                            && self.world.knows(at)
                            && tick <= self.actor_clock(*id)
                    })
                    && ai.visits.keys().all(|at| self.world.knows(*at))
            })
    }
    pub(crate) fn combat_view(
        &self,
        id: ActorId,
        visible: &impl Fn(tor_world::Location) -> bool,
        perceived: &[(ActorId, crate::actor_store::BodyCells)],
    ) -> Option<CombatView> {
        let a = self.actors.get(&id)?;
        let c = a.combat.as_ref()?;
        let actors = perceived
            .iter()
            .filter(|(other, body)| {
                *other != id && self.actors[other].alive() && body.iter().any(|(p, _)| visible(*p))
            })
            .filter_map(|(other, _)| {
                let a = &self.actors[other];
                let c = a.combat.as_ref()?;
                let injury = if c.hp() == c.spec.max_hp {
                    Injury::Healthy
                } else if u64::from(c.hp()) * 4 <= u64::from(c.spec.max_hp) {
                    Injury::NearDeath
                } else if u64::from(c.hp()) * 2 <= u64::from(c.spec.max_hp) {
                    Injury::BadlyWounded
                } else {
                    Injury::Wounded
                };
                Some((*other, self.hostile(id, *other), injury))
            })
            .collect();
        // Personal sensations are always safe; third-party events require both participants visible.
        let disclosed =
            |actor| actor == id || self.actors.get(&actor).is_some_and(|a| visible(a.location));
        let seen = |actor| disclosed(actor).then_some(actor);
        let events = self
            .combat
            .events
            .iter()
            .filter_map(|event| match event {
                CombatEvent::AbilityResolved {
                    actor,
                    target,
                    ability,
                    applied,
                    damage,
                    ..
                } if *ability != crate::grants::Ability::BasicMelee
                    && ((*actor == id || *target == id)
                        || (disclosed(*actor) && disclosed(*target))) =>
                {
                    Some(DisclosedCombatEvent::Ability {
                        caster: seen(*actor),
                        target: seen(*target),
                        ability: *ability,
                        outcome: if *ability == crate::grants::Ability::Fear {
                            if *applied {
                                AbilityOutcome::Applied
                            } else {
                                AbilityOutcome::Unaffected
                            }
                        } else if !applied {
                            AbilityOutcome::Miss
                        } else if *damage == 0 {
                            AbilityOutcome::Unaffected
                        } else {
                            AbilityOutcome::Applied
                        },
                    })
                }
                CombatEvent::Resolved {
                    actor,
                    target,
                    hit,
                    damage,
                    ..
                } if (*actor == id || *target == id)
                    || (disclosed(*actor) && disclosed(*target)) =>
                {
                    Some(DisclosedCombatEvent::Attack {
                        attacker: seen(*actor),
                        target: seen(*target),
                        outcome: if !hit {
                            AttackOutcome::Miss
                        } else if *damage == 0 {
                            AttackOutcome::NoInjury
                        } else {
                            AttackOutcome::Hit
                        },
                    })
                }
                CombatEvent::Interrupted { actor, .. } if *actor == id => {
                    Some(DisclosedCombatEvent::Interrupted { actor: *actor })
                }
                CombatEvent::Died { actor } if disclosed(*actor) => {
                    Some(DisclosedCombatEvent::Died { actor: *actor })
                }
                _ => None,
            })
            .collect();
        Some(CombatView {
            own_stats: Some(c.creature().own_stats()),
            hp: c.hp(),
            max_hp: c.spec.max_hp,
            preparation_remaining: a.pending.as_ref().map(|p| {
                if p.active {
                    p.remaining
                        .saturating_sub(self.tick.saturating_sub(p.started))
                } else {
                    p.remaining
                }
            }),
            preparation_active: a.pending.as_ref().is_some_and(|p| p.active),
            recovery_remaining: if a.pending.is_none() {
                a.ready_at.saturating_sub(self.tick)
            } else {
                0
            },
            actors,
            events,
            objective: self
                .combat
                .objective
                .as_ref()
                .filter(|o| o.disclosed)
                .map(|o| {
                    if o.item.is_some() {
                        ObjectiveKind::RetrieveAndReturn
                    } else {
                        ObjectiveKind::ReachExit
                    }
                }),
            exit: self
                .combat
                .objective
                .as_ref()
                .filter(|o| o.disclosed)
                .map(|o| o.anchor),
            victory: self.combat.outcome.victor.is_some(),
            dead: c.hp() == 0,
            terminal: self.combat.outcome.terminal,
        })
    }
    pub fn configure_run(
        &mut self,
        selected: ActorId,
        characters: BTreeSet<ActorId>,
        objective: Option<Objective>,
        hostility: BTreeMap<String, BTreeSet<String>>,
    ) -> Result<(), GameError> {
        self.configure_run_mode(
            selected,
            characters,
            objective,
            hostility,
            RunMode::Adventure,
        )
    }

    /// Establish an arena on a fresh game after all participants are loaded.
    /// Selected-character death is recorded but does not stop surviving actors.
    /// Arena recipes/drivers separately own encounter bounds and team results.
    pub fn configure_arena_run(
        &mut self,
        selected: ActorId,
        participants: BTreeSet<ActorId>,
        hostility: BTreeMap<String, BTreeSet<String>>,
    ) -> Result<(), GameError> {
        self.configure_arena_run_with_limits(
            selected,
            participants,
            hostility,
            crate::arena::ArenaLimits::default(),
        )
    }

    pub fn configure_arena_run_with_limits(
        &mut self,
        selected: ActorId,
        participants: BTreeSet<ActorId>,
        hostility: BTreeMap<String, BTreeSet<String>>,
        limits: crate::arena::ArenaLimits,
    ) -> Result<(), GameError> {
        if self.combat.selected.is_some() {
            return Err(GameError::ActorBusy);
        }
        if participants
            .iter()
            .any(|id| self.creature(*id).is_none() || !self.alive(*id))
        {
            return Err(GameError::InvalidLocation);
        }
        let arena =
            crate::arena::ArenaRun::new(limits, self.tick).ok_or(GameError::InvalidLocation)?;
        self.configure_run_mode(selected, participants, None, hostility, RunMode::Arena)?;
        self.combat.arena = Some(arena);
        self.check_arena_end();
        Ok(())
    }

    fn configure_run_mode(
        &mut self,
        selected: ActorId,
        characters: BTreeSet<ActorId>,
        objective: Option<Objective>,
        hostility: BTreeMap<String, BTreeSet<String>>,
        mode: RunMode,
    ) -> Result<(), GameError> {
        // Runtime edits cannot turn an existing arena into an adventure or
        // reopen a previously configured run by changing its death policy.
        if self.combat.run_mode == RunMode::Arena && self.combat.selected.is_some() {
            return Err(GameError::ActorBusy);
        }
        // Characters may be in regions that are detached or not built yet.
        let known = |id: &ActorId| self.actors.contains_key(id) || self.detached_actor(*id);
        if !known(&selected) || !characters.contains(&selected) || !characters.iter().all(known) {
            return Err(GameError::UnknownActor);
        }
        self.combat.run_mode = mode;
        self.combat.selected = Some(selected);
        self.combat.input_boundaries.clear();
        self.combat.input_boundaries.insert(selected);
        self.combat.characters = characters;
        self.combat.objective = objective;
        self.combat.hostility = hostility;
        self.check_objective();
        Ok(())
    }

    pub fn run_outcome(&self) -> &RunOutcome {
        &self.combat.outcome
    }

    pub fn hostile(&self, actor: ActorId, target: ActorId) -> bool {
        let faction = |id| {
            self.actors
                .get(&id)
                .and_then(|a| a.combat.as_ref())
                .map(|c| &c.spec.faction)
        };
        match (faction(actor), faction(target)) {
            (Some(a), Some(b)) => self
                .combat
                .hostility
                .get(a)
                .is_some_and(|enemies| enemies.contains(b)),
            _ => false,
        }
    }

    pub(crate) fn check_objective(&mut self) {
        if self.combat.outcome.terminal || self.combat.outcome.victor.is_some() {
            return;
        }
        let Some(objective) = &self.combat.objective else {
            return;
        };
        for id in &self.combat.characters {
            if self.alive(*id)
                && self.actors[id].location == objective.anchor
                && objective.item.is_none_or(|item| {
                    self.items
                        .get(&item)
                        .is_some_and(|i| i.location == ItemLocation::Carried(*id))
                })
            {
                self.combat.outcome.victor = Some(*id);
                self.combat.outcome.terminal = !objective.continue_play;
                return;
            }
        }
    }
    pub fn health(&self, actor: ActorId) -> Option<(u32, u32)> {
        let state = self.actors.get(&actor)?.combat.as_ref()?;
        Some((state.hp(), state.spec.max_hp))
    }

    pub(crate) fn apply_damage(
        &mut self,
        actor: ActorId,
        damage: &BTreeMap<DamageType, u32>,
    ) -> u32 {
        let Some(effective) = self.effective_combat(actor) else {
            return 0;
        };
        let damage_taken = effective.damage_taken(damage);
        self.apply_injury(actor, damage_taken)
    }

    /// Apply damage already resolved through protection; never reduce it twice.
    pub(crate) fn apply_injury(&mut self, actor: ActorId, damage_taken: u32) -> u32 {
        self.settle_creature_time();
        let Some(mut edited) = self.actors.get_mut(&actor) else {
            return 0;
        };
        let Some(state) = edited.combat.as_mut() else {
            return 0;
        };
        let loss = state.damage(damage_taken);
        let died = loss > 0 && state.hp() == 0;
        let mut interrupted = false;
        let mut intention = None;
        if loss > 0 {
            if let Some(pending) = edited.pending.as_mut().filter(|p| p.active) {
                pending.remaining = pending
                    .remaining
                    .saturating_sub(self.tick.saturating_sub(pending.started));
                pending.active = false;
                intention = pending.intention;
                interrupted = true;
            }
        }
        if interrupted {
            edited.ready_at = self.tick;
        }
        drop(edited);
        if interrupted {
            self.combat
                .events
                .push(CombatEvent::Interrupted { actor, intention });
            if !self.is_ai(actor) {
                self.combat.input_boundaries.insert(actor);
            }
        }
        if died {
            self.finish_death(actor);
        }
        loss
    }

    pub(crate) fn finish_death(&mut self, id: ActorId) {
        self.combat.input_boundaries.remove(&id);
        let mut actor = self.actors.get_mut(&id).unwrap();
        actor.cancel_preparation();
        actor.equipment.clear();
        let location = actor.location;
        let motion = actor.motion.clone();
        let orientation = actor.orientation;
        actor.motion = Default::default();
        let inventory: Vec<_> = self.items.at(ItemLocation::Carried(id)).collect();
        for item in inventory {
            self.items
                .edit(item, |item| {
                    item.location = ItemLocation::Ground(location);
                    item.motion = motion.clone();
                    item.orientation = orientation;
                })
                .expect("carried item");
        }
        let corpse_id = ItemId(self.next_item_id);
        self.next_item_id = self
            .next_item_id
            .checked_add(1)
            .expect("corpse identity capacity validated before combat");
        let mut spec = ItemSpec::ordinary(format!(
            "{} corpse",
            actor.combat.as_ref().unwrap().spec.name
        ));
        spec.class = crate::ItemClass::Corpse;
        spec.properties.insert("actor".into(), id.0.to_string());
        spec.properties
            .insert("death_tick".into(), self.tick.to_string());
        self.items.insert(
            corpse_id,
            Item {
                spec: tor_world::Shared::new(spec),
                quantity: 1,
                location: ItemLocation::Ground(location),
                motion,
                orientation,
            },
        );
        self.combat.events.push(CombatEvent::Died { actor: id });
        if self.combat.selected == Some(id) {
            self.combat.outcome.deceased = Some(id);
            self.combat.outcome.terminal |= self.combat.run_mode == RunMode::Adventure;
        }
        drop(actor);
        self.check_arena_end();
    }

    pub fn alive(&self, id: ActorId) -> bool {
        self.actors.get(&id).is_some_and(|a| a.alive())
    }

    pub fn combat_events(&self) -> &[CombatEvent] {
        &self.combat.events
    }

    pub fn preparation(&self, actor: ActorId) -> Option<&Preparation> {
        self.actors.get(&actor)?.pending.as_ref()
    }

    pub fn pause_preparation(&mut self, actor: ActorId) -> Option<crate::Work> {
        let mut a = self.actors.get_mut(&actor)?;
        let p = a.pending.as_mut()?;
        if !p.active {
            return None;
        }
        p.remaining = p
            .remaining
            .saturating_sub(self.tick.saturating_sub(p.started));
        p.active = false;
        let work = p.work;
        a.ready_at = self.tick;
        self.combat.input_boundaries.insert(actor);
        Some(work)
    }

    pub fn attack_available(&self, actor: ActorId, target: ActorId) -> bool {
        if actor == target {
            return false;
        }
        let (Some(a), Some(b)) = (self.actors.get(&actor), self.actors.get(&target)) else {
            return false;
        };
        if a.combat.as_ref().is_none_or(|c| c.hp() == 0)
            || b.combat.as_ref().is_none_or(|c| c.hp() == 0)
        {
            return false;
        }
        // Targeting needs occupied-cell visibility, not item descriptions, material
        // surfaces, inventory, or combat narration from a complete observation.
        let Some(target_cells) = self.body_cells(b.location, b.orientation, &b.body) else {
            return false;
        };
        let target_cells: BTreeSet<_> = target_cells.into_iter().map(|(at, _)| at).collect();
        let Ok(scene) = self.scene(actor) else {
            return false;
        };
        let visible: BTreeSet<_> = scene
            .iter()
            .map(|cell| cell.location)
            .filter(|at| target_cells.contains(at))
            .collect();
        let Some(cells) = self.body_cells(a.location, a.orientation, &a.body) else {
            return false;
        };
        cells.iter().any(|(from, _)| {
            (-1..=1).any(|x| {
                (-1..=1).any(|y| {
                    (-1..=1).any(|z| {
                        [x, y, z] != [0, 0, 0]
                            && self
                                .melee_neighbor(*from, [x, y, z])
                                .is_some_and(|to| visible.contains(&to))
                    })
                })
            })
        })
    }

    pub(crate) fn melee_neighbor(
        &self,
        from: tor_world::Location,
        delta: [i64; 3],
    ) -> Option<tor_world::Location> {
        let mut destination = None;
        for order in [
            [0, 1, 2],
            [0, 2, 1],
            [1, 0, 2],
            [1, 2, 0],
            [2, 0, 1],
            [2, 1, 0],
        ] {
            let mut at = from;
            let mut frame = 0;
            for axis in order {
                if delta[axis] == 0 {
                    continue;
                }
                let mut component = [0; 3];
                component[axis] = delta[axis];
                let direction =
                    tor_world::Direction::from_delta(tor_world::rotate_vector(frame, component))?;
                let (next, rotation) = self.world.physics_neighbor(at, direction)?;
                if !self.world.walkable(next) {
                    return None;
                }
                at = next;
                frame = tor_world::compose_rotation(frame, rotation);
            }
            if destination.is_some_and(|old| old != (at, frame)) {
                return None;
            }
            destination = Some((at, frame));
        }
        destination.map(|(at, _)| at)
    }

    pub(crate) fn next_attack_tick(&self) -> Option<u64> {
        self.actors
            .iter()
            .filter(|(id, _)| !self.actor_frozen(**id))
            .filter_map(|(_, a)| a.pending.as_ref())
            .filter(|p| p.active)
            .map(|p| p.started + p.remaining)
            .min()
    }

    pub(crate) fn resolve_attacks(&mut self) {
        let ids: Vec<_> = self.actors.keys().copied().collect();
        for id in ids {
            if self.combat.outcome.terminal {
                break;
            }
            if self.actor_frozen(id) {
                continue;
            }
            let Some(p) = self.preparation(id).cloned() else {
                continue;
            };
            let valid = self.work_duration(id, p.work).is_ok();
            if !valid || self.physics.displaced.contains(&id) {
                self.actors.get_mut(&id).unwrap().cancel_preparation();
                if p.active {
                    self.actors.get_mut(&id).unwrap().ready_at = self.tick;
                }
                self.combat.events.push(CombatEvent::Interrupted {
                    actor: id,
                    intention: p.intention,
                });
                continue;
            }
            if !p.active {
                continue;
            }
            if p.work.item().is_some() && !self.visible_hostiles(id).is_subset(&p.threats) {
                self.pause_preparation(id);
                self.combat.events.push(CombatEvent::Interrupted {
                    actor: id,
                    intention: p.intention,
                });
                if self.is_ai(id) {
                    self.combat.input_boundaries.remove(&id);
                }
                continue;
            }
            if p.started + p.remaining > self.tick {
                continue;
            }
            self.settle_creature_time();
            let diagnostic = p
                .work
                .target()
                .and_then(|target| self.begin_combat_diagnostic(id, target, &p));
            self.actors.get_mut(&id).unwrap().complete_preparation();
            if let crate::Work::UseAbility { ability, target } = p.work {
                self.actors.get_mut(&id).unwrap().ready_at = self.tick + p.recovery;
                let resolved = self.combat.events.len();
                let (applied, damage) = self.resolve_paid_ability(id, ability, target);
                self.finish_combat_diagnostic(diagnostic, applied, damage);
                self.combat.events.insert(
                    resolved,
                    CombatEvent::AbilityResolved {
                        intention: p.intention,
                        actor: id,
                        target,
                        ability,
                        applied,
                        damage,
                    },
                );
                continue;
            }
            let Some(target) = p.work.target() else {
                self.actors.get_mut(&id).unwrap().ready_at = self.tick;
                self.finish_item_work(id, p.work, p.intention);
                continue;
            };
            let defense = self.effective_combat(target).unwrap().defense;
            self.actors.get_mut(&id).unwrap().ready_at = self.tick + p.recovery;
            // The blow comes before any death it causes.
            let resolved = self.combat.events.len();
            let (hit, damage) = self.resolve_creature_melee(id, target, defense, 0);
            self.finish_combat_diagnostic(diagnostic, hit, damage);
            self.combat.events.insert(
                resolved,
                CombatEvent::Resolved {
                    intention: p.intention,
                    actor: id,
                    target,
                    hit,
                    damage,
                },
            );
        }
    }
}

impl crate::Actor {
    pub(crate) fn alive(&self) -> bool {
        self.combat.as_ref().is_none_or(|c| c.hp() > 0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DamageType {
    Energy,
    Impact,
    Keen,
    Spirit,
    Vital,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttackSpec {
    pub bonus: i32,
    pub wind_up: u64,
    pub recovery: u64,
    pub damage: BTreeMap<DamageType, u32>,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(not(test), derive(Clone))]
#[serde(default, deny_unknown_fields)]
pub struct CombatSpec {
    pub name: String,
    pub max_hp: u32,
    pub defense: i32,
    pub attack: AttackSpec,
    pub immunities: BTreeSet<DamageType>,
    pub reductions: BTreeMap<DamageType, u32>,
    pub faction: String,
}

#[cfg(test)]
thread_local! {
    static COMBAT_DEFINITION_COPIES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
impl Clone for CombatSpec {
    fn clone(&self) -> Self {
        COMBAT_DEFINITION_COPIES.with(|copies| copies.set(copies.get() + 1));
        Self {
            name: self.name.clone(),
            max_hp: self.max_hp,
            defense: self.defense,
            attack: self.attack.clone(),
            immunities: self.immunities.clone(),
            reductions: self.reductions.clone(),
            faction: self.faction.clone(),
        }
    }
}

impl Default for CombatSpec {
    fn default() -> Self {
        Self {
            name: "figure".into(),
            max_hp: 30,
            defense: 10,
            attack: AttackSpec {
                bonus: 2,
                wind_up: 60,
                recovery: 40,
                damage: BTreeMap::from([(DamageType::Impact, 4)]),
            },
            immunities: BTreeSet::new(),
            reductions: BTreeMap::new(),
            faction: "neutral".into(),
        }
    }
}

impl CombatSpec {
    pub fn valid(&self) -> bool {
        (1..=1_000_000).contains(&self.max_hp)
            && !self.name.is_empty()
            && self.name.len() <= 60
            && !self.name.chars().any(char::is_control)
            && (-1_000..=1_000).contains(&self.defense)
            && (-1_000..=1_000).contains(&self.attack.bonus)
            && (1..=1_000_000).contains(&self.attack.wind_up)
            && (1..=1_000_000).contains(&self.attack.recovery)
            && !self.attack.damage.is_empty()
            && self.attack.damage.values().all(|n| *n <= 1_000_000)
            && self.reductions.values().all(|n| *n <= 1_000_000)
            && !self.faction.is_empty()
            && self.faction.len() <= 80
            && !self.faction.chars().any(char::is_control)
    }

    pub fn damage_taken(&self, components: &BTreeMap<DamageType, u32>) -> u32 {
        components.iter().fold(0u32, |total, (kind, amount)| {
            total.saturating_add(if self.immunities.contains(kind) {
                0
            } else {
                amount.saturating_sub(*self.reductions.get(kind).unwrap_or(&0))
            })
        })
    }
}

/// Threshold only: opportunities and complications can be added independently.
pub fn hits(roll: u8, bonus: i32, defense: i32) -> bool {
    i64::from(roll) + i64::from(bonus) >= i64::from(defense)
}

pub fn impact_damage(incoming_velocity: i64) -> u32 {
    incoming_velocity
        .unsigned_abs()
        .saturating_sub(2048)
        .div_ceil(512)
        .min(u64::from(u32::MAX)) as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Action, ActorId, Game};

    #[test]
    fn arena_setup_requires_loaded_creature_backed_participants() {
        let mut game = fixture();
        let before = game.clone();
        assert!(game
            .configure_arena_run(ActorId(1), BTreeSet::from([ActorId(1)]), BTreeMap::new())
            .is_err());
        assert_eq!(game, before);
    }

    #[test]
    fn damage_does_not_copy_unrelated_combat_definitions() {
        for count in [16, 256, 4096] {
            let mut world = tor_world::World::new(vec![], vec![]).unwrap();
            world
                .add_region(tor_world::Region {
                    id: tor_world::RegionId(1),
                    name: "combat".into(),
                    bounds: tor_world::Extent::new(count + 2, 3, 1).unwrap(),
                })
                .unwrap();
            let mut game = Game::new(world, 42);
            for x in 0..count {
                let actor = game
                    .spawn_actor(
                        tor_world::Location {
                            region: tor_world::RegionId(1),
                            position: tor_world::Position { x, y: 1, z: 0 },
                        },
                        std::num::NonZeroU64::new(100).unwrap(),
                    )
                    .unwrap();
                configure_subject(&mut game, actor, 0, 60);
            }
            let old = game.clone();
            let before = COMBAT_DEFINITION_COPIES.with(|copies| copies.get());
            assert_eq!(
                game.apply_damage(ActorId(1), &BTreeMap::from([(DamageType::Impact, 1)])),
                1
            );
            let copied = COMBAT_DEFINITION_COPIES.with(|copies| copies.get()) - before;
            assert_eq!(copied, 0, "actor count {count}");
            assert_eq!(old.health(ActorId(1)), Some((30, 30)));
            assert_eq!(game.health(ActorId(1)), Some((29, 30)));
            assert_eq!(
                old.health(ActorId(count as u64)),
                game.health(ActorId(count as u64))
            );
        }
    }

    fn fixture() -> Game {
        let mut game = Game::two_room_in_stone(42);
        game.spawn_actor(
            tor_world::Location {
                region: tor_world::RegionId(1),
                position: tor_world::Position { x: 1, y: 1, z: 0 },
            },
            std::num::NonZeroU64::new(100).unwrap(),
        )
        .unwrap();
        game
    }

    fn configure_subject(game: &mut Game, actor: ActorId, bonus: i32, wind_up: u64) {
        use crate::attributes::{Attributes, ManaBinding, Skill};
        use crate::creatures::{CreatureBuild, Species};
        use crate::damage::{DamageComponent, DamageSpec};
        use crate::progression::{CreatureType, HdLedger, HdSource};
        let component = DamageComponent::fixed(DamageType::Impact, None, 4);
        let primary = component.key();
        let build = CreatureBuild::new(
            Species {
                id: "combat_subject".into(),
                kind: CreatureType::Humanoid,
                subtypes: BTreeSet::new(),
                default_attributes: Attributes::new([0; 6]).unwrap(),
                anatomy: crate::AnatomySpec::humanoid(),
                melee: crate::attacks::MeleeAttack::new(
                    Skill::HeavyWeaponry,
                    bonus,
                    wind_up,
                    40,
                    DamageSpec::new(vec![component], Some(primary)).unwrap(),
                )
                .unwrap(),
                grants: vec![crate::grants::Grant::Health(22)],
            },
            HdLedger::seeded(vec![HdSource::Racial], 42).unwrap(),
            ManaBinding::Intellect,
        )
        .unwrap();
        game.configure_creature(
            actor,
            crate::CreatureIdentity {
                name: "subject".into(),
                faction: "neutral".into(),
            },
            build,
        )
        .unwrap();
    }

    fn opponents() -> Game {
        opponents_with_windups([60, 60])
    }

    fn opponents_with_windups(wind_ups: [u64; 2]) -> Game {
        let mut game = fixture();
        game.spawn_actor(
            tor_world::Location {
                region: tor_world::RegionId(1),
                position: tor_world::Position { x: 2, y: 1, z: 0 },
            },
            std::num::NonZeroU64::new(100).unwrap(),
        )
        .unwrap();
        for (id, wind_up) in [ActorId(1), ActorId(2)].into_iter().zip(wind_ups) {
            configure_subject(&mut game, id, 100, wind_up);
        }
        game
    }

    #[test]
    fn attacks_resolve_after_windup_and_charge_recovery() {
        let mut game = opponents();
        game.act(ActorId(1), Action::Attack { target: ActorId(2) })
            .unwrap();
        assert_eq!(game.tick(), 0);
        assert_eq!(game.health(ActorId(2)), Some((30, 30)));
        game.act(ActorId(2), Action::Wait).unwrap();
        assert_eq!(game.tick(), 100);
        assert_eq!(game.health(ActorId(2)), Some((26, 30)));
        assert!(game.preparation(ActorId(1)).is_none());
    }

    #[test]
    fn earlier_hit_interrupts_without_erasing_spent_preparation() {
        let mut game = opponents_with_windups([60, 30]);
        game.act(ActorId(1), Action::Attack { target: ActorId(2) })
            .unwrap();
        game.act(ActorId(2), Action::Attack { target: ActorId(1) })
            .unwrap();
        assert_eq!(game.tick(), 30);
        let progress = game.preparation(ActorId(1)).unwrap();
        assert!(!progress.active);
        assert_eq!(progress.remaining, 30);
        assert_eq!(game.health(ActorId(1)), Some((26, 30)));
        game.act(ActorId(1), Action::Attack { target: ActorId(2) })
            .unwrap();
        assert_eq!(game.health(ActorId(2)), Some((26, 30)));
        assert_eq!(game.tick(), 70);
    }

    #[test]
    fn death_drops_once_and_removes_living_occupancy() {
        let mut game = opponents();
        let damage = BTreeMap::from([(DamageType::Vital, 100)]);
        assert_eq!(game.apply_damage(ActorId(2), &damage), 30);
        let count = game.items.len();
        assert_eq!(game.apply_damage(ActorId(2), &damage), 0);
        assert_eq!(game.items.len(), count);
        assert!(!game.alive(ActorId(2)));
        assert!(!game.occupied(game.actors[&ActorId(2)].location));
    }

    #[test]
    fn authored_combat_survives_checkpoints_without_changing_rng() {
        let mut game = fixture();
        configure_subject(&mut game, ActorId(1), 0, 60);
        let source = game
            .inspect_creature(ActorId(1))
            .expect("owned combat source");
        assert_eq!(source.creature.build().ledger().total_hd(), 1);
        let mut shared = crate::checkpoint::SharedState::default();
        let restored = Game::restore_checkpoint(game.checkpoint(&mut shared), &shared).unwrap();
        assert_eq!(game, restored);
        assert_eq!(game.health(ActorId(1)), Some((30, 30)));
    }

    #[test]
    fn damage_persists_without_wait_healing() {
        let mut game = fixture();
        configure_subject(&mut game, ActorId(1), 0, 60);
        game.apply_damage(ActorId(1), &BTreeMap::from([(DamageType::Impact, 4)]));
        assert_eq!(game.health(ActorId(1)), Some((26, 30)));
        game.act(ActorId(1), Action::Wait).unwrap();
        assert_eq!(game.health(ActorId(1)), Some((26, 30)));
    }

    #[test]
    fn threshold_has_no_automatic_extremes() {
        assert!(hits(1, 9, 10));
        assert!(!hits(20, 0, 21));
        assert!(!hits(9, 0, 10));
        assert!(hits(10, 0, 10));
    }

    #[test]
    fn components_apply_their_own_immunities_and_reductions() {
        let mut defender = CombatSpec::default();
        defender.immunities.insert(DamageType::Energy);
        defender.reductions.insert(DamageType::Impact, 5);
        defender.reductions.insert(DamageType::Keen, 2);
        assert_eq!(
            defender.damage_taken(&BTreeMap::from([
                (DamageType::Energy, 100),
                (DamageType::Impact, 3),
                (DamageType::Keen, 7),
                (DamageType::Vital, 4),
            ])),
            9
        );
    }

    #[test]
    fn impacts_have_a_safe_threshold_and_ignore_sign() {
        assert_eq!(impact_damage(2048), 0);
        assert_eq!(impact_damage(2049), 1);
        assert_eq!(impact_damage(-2560), 1);
        assert_eq!(impact_damage(8192), 12);
    }
}
