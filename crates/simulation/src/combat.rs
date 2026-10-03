//! Deterministic combat values. These are authoritative rules, never wire views.
use crate::{ActorId, Game, GameError, Item, ItemId, ItemLocation, ItemSpec};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CombatState {
    pub spec: CombatSpec,
    pub hp: u32,
    pub pending: Option<Preparation>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Preparation {
    pub target: ActorId,
    pub remaining: u64,
    pub started: u64,
    pub active: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CombatWorld {
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CombatEvent {
    Resolved {
        actor: ActorId,
        target: ActorId,
        hit: bool,
        damage: u32,
    },
    Interrupted {
        actor: ActorId,
    },
    Died {
        actor: ActorId,
    },
}

impl CombatWorld {
    pub fn new(seed: u64) -> Self {
        Self {
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

    fn d20(&mut self) -> u8 {
        // SplitMix64 with rejection rather than modulo bias. State is checkpointed.
        loop {
            self.rng = self.rng.wrapping_add(0x9e3779b97f4a7c15);
            let mut z = self.rng;
            z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
            z ^= z >> 31;
            if z < u64::MAX - u64::MAX % 20 {
                return (z % 20 + 1) as u8;
            }
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
                    && o.item
                        .is_none_or(|id| self.items.contains_key(&id) || self.detached_item(id))
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
            && self.actors.iter().all(|(id, actor)| {
                actor.combat.as_ref().is_none_or(|c| {
                    c.spec.valid()
                        && c.hp <= c.spec.max_hp
                        && c.pending.as_ref().is_none_or(|p| {
                            c.hp > 0
                                && p.target != *id
                                && self.actors.contains_key(&p.target)
                                && p.remaining <= c.spec.attack.wind_up
                                && p.started <= self.tick
                                && p.started
                                    .checked_add(p.remaining)
                                    .and_then(|t| t.checked_add(c.spec.attack.recovery))
                                    .is_some()
                                && (!p.active || p.started + p.remaining >= self.actor_clock(*id))
                        })
                })
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
    ) -> Option<CombatView> {
        let a = self.actors.get(&id)?;
        let c = a.combat.as_ref()?;
        let actors = self
            .actors
            .iter()
            .filter(|(other, a)| {
                **other != id
                    && a.alive()
                    && self
                        .body_cells(a.location, a.orientation, &a.body)
                        .is_some_and(|cells| cells.iter().any(|(p, _)| visible(*p)))
            })
            .filter_map(|(other, a)| {
                let c = a.combat.as_ref()?;
                let injury = if c.hp == c.spec.max_hp {
                    Injury::Healthy
                } else if u64::from(c.hp) * 4 <= u64::from(c.spec.max_hp) {
                    Injury::NearDeath
                } else if u64::from(c.hp) * 2 <= u64::from(c.spec.max_hp) {
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
                CombatEvent::Resolved {
                    actor,
                    target,
                    hit,
                    damage,
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
                CombatEvent::Interrupted { actor } if *actor == id => {
                    Some(DisclosedCombatEvent::Interrupted { actor: *actor })
                }
                CombatEvent::Died { actor } if disclosed(*actor) => {
                    Some(DisclosedCombatEvent::Died { actor: *actor })
                }
                _ => None,
            })
            .collect();
        Some(CombatView {
            hp: c.hp,
            max_hp: c.spec.max_hp,
            preparation_remaining: c.pending.as_ref().map(|p| {
                if p.active {
                    p.remaining
                        .saturating_sub(self.tick.saturating_sub(p.started))
                } else {
                    p.remaining
                }
            }),
            preparation_active: c.pending.as_ref().is_some_and(|p| p.active),
            recovery_remaining: if c.pending.is_none() {
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
            dead: c.hp == 0,
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
        // Characters may be in regions that are detached or not built yet.
        let known = |id: &ActorId| self.actors.contains_key(id) || self.detached_actor(*id);
        if !known(&selected) || !characters.contains(&selected) || !characters.iter().all(known) {
            return Err(GameError::UnknownActor);
        }
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
    pub fn configure_combat(&mut self, actor: ActorId, spec: CombatSpec) -> Result<(), GameError> {
        if !spec.valid() {
            return Err(GameError::InvalidLocation);
        }
        let actor = self.actors.get_mut(&actor).ok_or(GameError::UnknownActor)?;
        actor.combat = Some(CombatState {
            hp: spec.max_hp,
            spec,
            pending: None,
        });
        Ok(())
    }

    pub fn health(&self, actor: ActorId) -> Option<(u32, u32)> {
        let state = self.actors.get(&actor)?.combat.as_ref()?;
        Some((state.hp, state.spec.max_hp))
    }

    pub(crate) fn apply_damage(
        &mut self,
        actor: ActorId,
        damage: &BTreeMap<DamageType, u32>,
    ) -> u32 {
        let Some(state) = self.actors.get_mut(&actor).and_then(|a| a.combat.as_mut()) else {
            return 0;
        };
        let loss = state.spec.damage_taken(damage).min(state.hp);
        state.hp -= loss;
        let died = loss > 0 && state.hp == 0;
        if loss > 0 {
            if let Some(pending) = state.pending.as_mut().filter(|p| p.active) {
                pending.remaining = pending
                    .remaining
                    .saturating_sub(self.tick.saturating_sub(pending.started));
                pending.active = false;
                self.combat.events.push(CombatEvent::Interrupted { actor });
                if !self.is_ai(actor) {
                    self.combat.input_boundaries.insert(actor);
                }
                self.actors
                    .get_mut(&actor)
                    .expect("existing actor")
                    .ready_at = self.tick;
            }
        }
        if died {
            self.finish_death(actor);
        }
        loss
    }

    fn finish_death(&mut self, id: ActorId) {
        self.combat.input_boundaries.remove(&id);
        let actor = self.actors.get_mut(&id).unwrap();
        actor.combat.as_mut().unwrap().pending = None;
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
        spec.properties.insert("actor".into(), id.0.to_string());
        spec.properties
            .insert("death_tick".into(), self.tick.to_string());
        self.items.insert(
            corpse_id,
            Item {
                spec,
                quantity: 1,
                location: ItemLocation::Ground(location),
                motion,
                orientation,
            },
        );
        self.combat.events.push(CombatEvent::Died { actor: id });
        if self.combat.selected == Some(id) {
            self.combat.outcome.deceased = Some(id);
            self.combat.outcome.terminal = true;
        }
    }

    pub fn alive(&self, id: ActorId) -> bool {
        self.actors.get(&id).is_some_and(|a| a.alive())
    }

    pub fn combat_events(&self) -> &[CombatEvent] {
        &self.combat.events
    }

    pub fn preparation(&self, actor: ActorId) -> Option<&Preparation> {
        self.actors.get(&actor)?.combat.as_ref()?.pending.as_ref()
    }

    pub fn pause_preparation(&mut self, actor: ActorId) -> Option<ActorId> {
        let a = self.actors.get_mut(&actor)?;
        let p = a.combat.as_mut()?.pending.as_mut()?;
        if !p.active {
            return None;
        }
        p.remaining = p
            .remaining
            .saturating_sub(self.tick.saturating_sub(p.started));
        p.active = false;
        a.ready_at = self.tick;
        self.combat.input_boundaries.insert(actor);
        Some(p.target)
    }

    pub fn attack_available(&self, actor: ActorId, target: ActorId) -> bool {
        if actor == target {
            return false;
        }
        let (Some(a), Some(b)) = (self.actors.get(&actor), self.actors.get(&target)) else {
            return false;
        };
        if a.combat.as_ref().is_none_or(|c| c.hp == 0)
            || b.combat.as_ref().is_none_or(|c| c.hp == 0)
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

    fn melee_neighbor(
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

    pub(crate) fn start_attack(&mut self, actor: ActorId, target: ActorId) {
        let c = self
            .actors
            .get_mut(&actor)
            .unwrap()
            .combat
            .as_mut()
            .unwrap();
        let remaining = c
            .pending
            .as_ref()
            .filter(|p| p.target == target)
            .map_or(c.spec.attack.wind_up, |p| p.remaining);
        c.pending = Some(Preparation {
            target,
            remaining,
            started: self.tick,
            active: true,
        });
    }

    pub(crate) fn next_attack_tick(&self) -> Option<u64> {
        self.actors
            .iter()
            .filter(|(id, _)| !self.actor_frozen(**id))
            .filter_map(|(_, a)| a.combat.as_ref()?.pending.as_ref())
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
            let valid = self.attack_available(id, p.target);
            if !valid || self.physics.displaced.contains(&id) {
                self.actors
                    .get_mut(&id)
                    .unwrap()
                    .combat
                    .as_mut()
                    .unwrap()
                    .pending = None;
                if p.active {
                    self.actors.get_mut(&id).unwrap().ready_at = self.tick;
                }
                self.combat
                    .events
                    .push(CombatEvent::Interrupted { actor: id });
                continue;
            }
            if !p.active {
                continue;
            }
            if p.started + p.remaining > self.tick {
                continue;
            }
            let attack = self.actors[&id]
                .combat
                .as_ref()
                .unwrap()
                .spec
                .attack
                .clone();
            let defense = self.actors[&p.target].combat.as_ref().unwrap().spec.defense;
            let hit = hits(self.combat.d20(), attack.bonus, defense);
            self.actors
                .get_mut(&id)
                .unwrap()
                .combat
                .as_mut()
                .unwrap()
                .pending = None;
            self.actors.get_mut(&id).unwrap().ready_at = self.tick + attack.recovery;
            // The blow comes before any death it causes.
            let resolved = self.combat.events.len();
            let damage = if hit {
                self.apply_damage(p.target, &attack.damage)
            } else {
                0
            };
            self.combat.events.insert(
                resolved,
                CombatEvent::Resolved {
                    actor: id,
                    target: p.target,
                    hit,
                    damage,
                },
            );
        }
    }
}

impl crate::Actor {
    pub(crate) fn alive(&self) -> bool {
        self.combat.as_ref().is_none_or(|c| c.hp > 0)
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttackSpec {
    pub bonus: i32,
    pub wind_up: u64,
    pub recovery: u64,
    pub damage: BTreeMap<DamageType, u32>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
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

    fn opponents() -> Game {
        let mut game = fixture();
        game.spawn_actor(
            tor_world::Location {
                region: tor_world::RegionId(1),
                position: tor_world::Position { x: 2, y: 1, z: 0 },
            },
            std::num::NonZeroU64::new(100).unwrap(),
        )
        .unwrap();
        for id in [ActorId(1), ActorId(2)] {
            let mut spec = CombatSpec::default();
            spec.attack.bonus = 100;
            game.configure_combat(id, spec).unwrap();
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
        let mut game = opponents();
        game.actors
            .get_mut(&ActorId(2))
            .unwrap()
            .combat
            .as_mut()
            .unwrap()
            .spec
            .attack
            .wind_up = 30;
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
        game.configure_combat(ActorId(1), CombatSpec::default())
            .unwrap();
        let mut shared = crate::checkpoint::SharedState::default();
        let restored = Game::restore_checkpoint(game.checkpoint(&mut shared), &shared).unwrap();
        assert_eq!(game, restored);
        assert_eq!(game.health(ActorId(1)), Some((30, 30)));
    }

    #[test]
    fn damage_persists_without_wait_healing() {
        let mut game = fixture();
        game.configure_combat(ActorId(1), CombatSpec::default())
            .unwrap();
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
