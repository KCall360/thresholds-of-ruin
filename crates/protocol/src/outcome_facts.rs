//! Ordered presentation facts captured from already-disclosed views. These are
//! historical descriptions, not targeting capabilities or input permissions.
use crate::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Private causal receipt position used while capturing disclosed outcomes.
/// This is neither serialized nor exposed as a client-visible activity counter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EventProvenance {
    pub batch: u64,
    pub through: usize,
}

/// Every encoded fact/name consumes more than eight bytes. The wire byte limit
/// remains authoritative; this also bounds collection allocation during decode.
pub const MAX_OUTCOME_FACTS: usize = MAX_STATE_BYTES / 8;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutcomeFacts {
    pub self_target: ActorTarget,
    /// Complete current disclosed progress; independent of narrated Status facts.
    #[serde(deserialize_with = "read_status")]
    pub status: Option<OutcomeStatusFacts>,
    #[serde(deserialize_with = "read_names")]
    pub actor_names: Vec<DisclosedActorName>,
    #[serde(deserialize_with = "read_facts")]
    pub facts: Vec<OutcomeFact>,
}
fn read_status<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Option<OutcomeStatusFacts>, D::Error> {
    Option::deserialize(d)
}
fn read_names<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<DisclosedActorName>, D::Error> {
    crate::bounded::bounded::<D, _, MAX_OUTCOME_FACTS>(d)
}
fn read_facts<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<OutcomeFact>, D::Error> {
    crate::bounded::bounded::<D, _, MAX_OUTCOME_FACTS>(d)
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DisclosedActorName {
    pub actor: ActorTarget,
    pub name: String,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutcomeNameFallback {
    Item,
    Object,
    Stairs,
    ObjectiveItem,
    Door,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutcomeName {
    pub text: String,
    pub fallback: OutcomeNameFallback,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemWorkKind {
    Equip,
    Unequip,
    Drink,
    Eat,
    Read,
    Ignite,
    Extinguish,
    Zap,
}
impl ItemWorkKind {
    pub fn from_action(action: &Action) -> Option<(Self, ItemTarget)> {
        Some(match action {
            Action::Equip { item, .. } => (Self::Equip, *item),
            Action::Unequip { item } => (Self::Unequip, *item),
            Action::Drink { item } => (Self::Drink, *item),
            Action::Eat { item } => (Self::Eat, *item),
            Action::Read { item } => (Self::Read, *item),
            Action::Ignite { item } => (Self::Ignite, *item),
            Action::Extinguish { item } => (Self::Extinguish, *item),
            Action::Zap { item, .. } => (Self::Zap, *item),
            _ => return None,
        })
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum OutcomeFact {
    OwnAction {
        event: Event,
        name: Option<OutcomeName>,
        basic_attack: bool,
    },
    TrapDiscovered {
        family: Option<TrapFamily>,
    },
    DoorDiscovered,
    Displaced,
    Impacted,
    Noticed {
        actor: ActorTarget,
        name: String,
    },
    LostSight {
        actor: ActorTarget,
        name: String,
    },
    DoorChanged {
        name: String,
        open: bool,
    },
    Pacing {
        ready: bool,
    },
    ItemCompleted {
        work: ItemWorkKind,
        name: Option<String>,
    },
    ItemRefused {
        reason: ItemRefusalReason,
    },
    CurseBlocked {
        name: Option<String>,
    },
    PublicPlan {
        record: PublicPlanObservationView,
    },
    Combat {
        event: CombatEventView,
        basic_attack: bool,
    },
    Status {
        status: OutcomeStatusFacts,
    },
    /// Additional structured text-client feedback; not an ordinary common line.
    Injured {
        actor: ActorTarget,
        injury: Injury,
    },
    Objective {
        objective: ObjectiveKind,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutcomeStatusFacts {
    #[serde(with = "crate::integers::unsigned")]
    pub hp: u64,
    #[serde(with = "crate::integers::unsigned")]
    pub max_hp: u64,
    pub dead: bool,
    pub victory: bool,
    pub terminal: bool,
    pub sequence: Option<SequenceView>,
    #[serde(with = "crate::integers::optional_unsigned")]
    pub preparation_remaining: Option<u64>,
    pub preparation_active: bool,
    #[serde(with = "crate::integers::unsigned")]
    pub recovery_remaining: u64,
}
impl From<&CombatView> for OutcomeStatusFacts {
    fn from(c: &CombatView) -> Self {
        Self {
            hp: c.hp,
            max_hp: c.max_hp,
            dead: c.dead,
            victory: c.victory,
            terminal: c.terminal,
            sequence: c.sequence.clone(),
            preparation_remaining: c.preparation_remaining,
            preparation_active: c.preparation_active,
            recovery_remaining: c.recovery_remaining,
        }
    }
}

/// Non-wire narration inputs. Unlike a picture, these contain no geometry,
/// assets, targeting affordances, detailed statistics or chart capabilities.
/// Hosts must populate them from the same authorized event-time disclosure as
/// ordinary observations; reducing presentation work does not grant knowledge.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutcomeObservation {
    pub self_target: ActorTarget,
    pub tick: u64,
    pub ready: bool,
    pub visible_cells: Vec<OutcomeCell>,
    pub visible_actors: Vec<OutcomeActorName>,
    pub inventory: Vec<OutcomeItemName>,
    pub ground_items: Vec<OutcomeGroundName>,
    pub motion: Option<OutcomeMotion>,
    pub interactions: Option<OutcomeInteractions>,
    pub combat: Option<OutcomeCombat>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutcomeCell {
    pub key: String,
    pub wall: bool,
    pub door: Option<OutcomeDoor>,
    pub trap: Option<OutcomeTrap>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutcomeDoor {
    pub id: DoorTarget,
    pub open: bool,
    pub name: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutcomeTrap {
    pub id: TrapTarget,
    pub family: Option<TrapFamily>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutcomeActorName {
    pub id: ActorTarget,
    pub name: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutcomeItemName {
    pub id: ItemTarget,
    pub name: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutcomeGroundName {
    pub id: ItemTarget,
    pub name: OutcomeName,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutcomeMotion {
    pub displaced: bool,
    pub impacted: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutcomeInteractions {
    pub completed: Vec<Action>,
    pub refused: Vec<ItemRefusalView>,
    pub curse_blocked: Vec<ItemTarget>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutcomeAbility {
    pub id: String,
    pub basic_attack: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutcomeCombat {
    pub status: OutcomeStatusFacts,
    pub abilities: Vec<OutcomeAbility>,
    pub public_plan: Option<PublicPlanObservationView>,
    pub actors: Vec<CombatActorView>,
    pub events: Vec<CombatEventView>,
    pub objective: Option<ObjectiveKind>,
}

impl From<&Observation> for OutcomeObservation {
    fn from(view: &Observation) -> Self {
        Self {
            self_target: view.self_target,
            tick: view.tick,
            ready: view.ready,
            visible_cells: view
                .visible_cells
                .iter()
                .map(|cell| OutcomeCell {
                    key: cell.key.clone(),
                    wall: cell.wall,
                    door: cell.door.as_ref().map(|door| OutcomeDoor {
                        id: door.id,
                        open: door.open,
                        name: door.name.clone(),
                    }),
                    trap: cell.trap.as_ref().map(|trap| OutcomeTrap {
                        id: trap.id,
                        family: trap.family,
                    }),
                })
                .collect(),
            visible_actors: view
                .visible_actors
                .iter()
                .map(|actor| OutcomeActorName {
                    id: actor.id,
                    name: actor.name.clone(),
                })
                .collect(),
            inventory: view
                .inventory
                .iter()
                .map(|item| OutcomeItemName {
                    id: item.id,
                    name: item.name.clone(),
                })
                .collect(),
            ground_items: view
                .ground_items
                .iter()
                .map(|item| OutcomeGroundName {
                    id: item.item.id,
                    name: ground_outcome_name(item),
                })
                .collect(),
            motion: view.motion.as_ref().map(|motion| OutcomeMotion {
                displaced: motion.displaced,
                impacted: motion.impacted,
            }),
            interactions: view
                .interactions
                .as_ref()
                .map(|interactions| OutcomeInteractions {
                    completed: interactions.completed.clone(),
                    refused: interactions.refused.clone(),
                    curse_blocked: interactions.curse_blocked.clone(),
                }),
            combat: view.combat.as_ref().map(|combat| OutcomeCombat {
                status: combat.into(),
                abilities: combat
                    .own_stats
                    .as_ref()
                    .map(|stats| {
                        stats
                            .abilities
                            .iter()
                            .map(|ability| OutcomeAbility {
                                id: ability.id.clone(),
                                basic_attack: ability.basic_attack,
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
                public_plan: combat.public_plan.clone(),
                actors: combat.actors.clone(),
                events: combat.events.clone(),
                objective: combat.objective,
            }),
        }
    }
}

/// Cosmetic item fallback is shared with the full-view compatibility adapter.
fn ground_outcome_name(item: &GroundItemView) -> OutcomeName {
    match &item.presentation {
        Some(p) if p.name.trim().is_empty() => OutcomeName {
            text: String::new(),
            fallback: match p.kind {
                ObjectPresentationKind::Item { .. } => OutcomeNameFallback::Object,
                ObjectPresentationKind::Stairs => OutcomeNameFallback::Stairs,
            },
        },
        Some(p) => OutcomeName {
            text: p.name.clone(),
            fallback: OutcomeNameFallback::Item,
        },
        None => OutcomeName {
            text: item.item.name.clone(),
            fallback: OutcomeNameFallback::Item,
        },
    }
}

/// Preserve the exact name source used by own-action narration. No labels or
/// fallback words are rendered here; absence is distinct from an empty name.
pub fn outcome_action_name(event: &Event, view: &Observation) -> Option<OutcomeName> {
    let inventory = |id, fallback| {
        view.inventory
            .iter()
            .find(|item| item.id == id)
            .map(|item| OutcomeName {
                text: item.name.clone(),
                fallback,
            })
    };
    match event {
        Event::Escaped { item } => inventory(*item, OutcomeNameFallback::ObjectiveItem),
        Event::ItemStarted { action } => ItemWorkKind::from_action(action)
            .and_then(|(_, item)| inventory(item, OutcomeNameFallback::Item)),
        Event::Taken { result, .. } => inventory(*result, OutcomeNameFallback::Item),
        Event::Dropped { result, .. } => view
            .ground_items
            .iter()
            .find(|item| item.item.id == *result)
            .map(ground_outcome_name),
        Event::DoorChanged { door, .. } => view
            .visible_cells
            .iter()
            .filter_map(|cell| cell.door.as_ref())
            .find(|value| value.id == *door)
            .map(|value| OutcomeName {
                text: value.name.clone(),
                fallback: OutcomeNameFallback::Door,
            }),
        _ => None,
    }
}

fn compact_action_name(event: &Event, view: &OutcomeObservation) -> Option<OutcomeName> {
    let inventory = |id, fallback| {
        view.inventory
            .iter()
            .find(|item| item.id == id)
            .map(|item| OutcomeName {
                text: item.name.clone(),
                fallback,
            })
    };
    match event {
        Event::Escaped { item } => inventory(*item, OutcomeNameFallback::ObjectiveItem),
        Event::ItemStarted { action } => ItemWorkKind::from_action(action)
            .and_then(|(_, item)| inventory(item, OutcomeNameFallback::Item)),
        Event::Taken { result, .. } => inventory(*result, OutcomeNameFallback::Item),
        Event::Dropped { result, .. } => view
            .ground_items
            .iter()
            .find(|item| item.id == *result)
            .map(|item| item.name.clone()),
        Event::DoorChanged { door, .. } => view
            .visible_cells
            .iter()
            .filter_map(|cell| cell.door.as_ref())
            .find(|value| value.id == *door)
            .map(|value| OutcomeName {
                text: value.name.clone(),
                fallback: OutcomeNameFallback::Door,
            }),
        _ => None,
    }
}

#[derive(Clone, Copy)]
struct ReceiptSelection {
    combat: usize,
    completed: usize,
    refused: usize,
    curse: usize,
    displaced: bool,
    impacted: bool,
}

impl OutcomeFacts {
    /// Capture legacy presentation transitions. Causal hosts must instead use
    /// capture_causal: equal receipts and equal ticks are not event identities.
    pub fn capture(before: &Observation, after: &Observation, event: Option<&Event>) -> Self {
        Self::capture_selected(
            &before.into(),
            &after.into(),
            event,
            None,
            before.combat != after.combat,
        )
    }

    /// The stamps are host-private event generation/through boundaries and are
    /// never serialized. Sealed same-generation projections must retain an exact
    /// prefix: no receipt caps, later grants or mutable historical identities.
    pub fn capture_causal(
        before: &Observation,
        after: &Observation,
        event: Option<&Event>,
        before_stamp: EventProvenance,
        after_stamp: EventProvenance,
    ) -> Result<Self, InvalidState> {
        Self::capture_compact_causal(
            &before.into(),
            &after.into(),
            event,
            before_stamp,
            after_stamp,
        )
    }

    /// Capture from compact authorized semantic inputs without constructing a picture.
    pub fn capture_compact_causal(
        before: &OutcomeObservation,
        after: &OutcomeObservation,
        event: Option<&Event>,
        before_stamp: EventProvenance,
        after_stamp: EventProvenance,
    ) -> Result<Self, InvalidState> {
        fn suffix<T: PartialEq>(
            before: &[T],
            after: &[T],
            unchanged: bool,
        ) -> Result<usize, InvalidState> {
            if !after.starts_with(before) || (unchanged && before.len() != after.len()) {
                return Err(InvalidState::InvalidCombat);
            }
            Ok(before.len())
        }
        let new_batch = before_stamp.batch != after_stamp.batch;
        let unchanged = before_stamp == after_stamp;
        if !new_batch && after_stamp.through < before_stamp.through {
            return Err(InvalidState::InvalidCombat);
        }
        let mut selected = ReceiptSelection {
            combat: 0,
            completed: 0,
            refused: 0,
            curse: 0,
            displaced: after.motion.as_ref().is_some_and(|m| m.displaced)
                && (new_batch || before.motion.as_ref().is_none_or(|m| !m.displaced)),
            impacted: after.motion.as_ref().is_some_and(|m| m.impacted)
                && (new_batch || before.motion.as_ref().is_none_or(|m| !m.impacted)),
        };
        if !new_batch {
            selected.combat = suffix(
                before.combat.as_ref().map_or(&[], |c| c.events.as_slice()),
                after.combat.as_ref().map_or(&[], |c| c.events.as_slice()),
                unchanged,
            )?;
            selected.completed = suffix(
                before
                    .interactions
                    .as_ref()
                    .map_or(&[], |i| i.completed.as_slice()),
                after
                    .interactions
                    .as_ref()
                    .map_or(&[], |i| i.completed.as_slice()),
                unchanged,
            )?;
            selected.refused = suffix(
                before
                    .interactions
                    .as_ref()
                    .map_or(&[], |i| i.refused.as_slice()),
                after
                    .interactions
                    .as_ref()
                    .map_or(&[], |i| i.refused.as_slice()),
                unchanged,
            )?;
            selected.curse = suffix(
                before
                    .interactions
                    .as_ref()
                    .map_or(&[], |i| i.curse_blocked.as_slice()),
                after
                    .interactions
                    .as_ref()
                    .map_or(&[], |i| i.curse_blocked.as_slice()),
                unchanged,
            )?;
        }
        // Motion flags have batch lifetime, not an event-log entry of their own.
        // An unchanged batch cannot create another copy of an old pulse.
        if unchanged {
            selected.displaced = false;
            selected.impacted = false;
        }
        Ok(Self::capture_selected(
            before,
            after,
            event,
            Some(selected),
            false,
        ))
    }

    fn capture_selected(
        before: &OutcomeObservation,
        after: &OutcomeObservation,
        event: Option<&Event>,
        receipts: Option<ReceiptSelection>,
        legacy_combat_changed: bool,
    ) -> Self {
        let mut facts = Vec::new();
        fn abilities(view: &OutcomeObservation) -> Option<&[OutcomeAbility]> {
            view.combat
                .as_ref()
                .map(|combat| combat.abilities.as_slice())
        }
        let basic = |ability: &ContentRef| {
            abilities(after)
                .into_iter()
                .flatten()
                .chain(abilities(before).into_iter().flatten())
                .find(|candidate| candidate.id == ability.id)
                .is_some_and(|candidate| candidate.basic_attack)
        };
        if let Some(event) = event {
            facts.push(OutcomeFact::OwnAction { event:event.clone(), name:compact_action_name(event, after),
                basic_attack:matches!(event, Event::AbilityStarted { ability, .. } if basic(ability)) });
        }
        let traps = |view: &OutcomeObservation| -> BTreeMap<TrapTarget, Option<TrapFamily>> {
            view.visible_cells
                .iter()
                .filter_map(|cell| cell.trap.as_ref())
                .map(|trap| (trap.id, trap.family))
                .collect()
        };
        let old = traps(before);
        for (id, family) in traps(after) {
            if matches!(
                (old.get(&id), family),
                (None, None) | (None | Some(None), Some(_))
            ) {
                facts.push(OutcomeFact::TrapDiscovered { family });
            }
        }
        let old: BTreeMap<_, _> = before
            .visible_cells
            .iter()
            .map(|cell| (&cell.key, cell))
            .collect();
        let mut discovered = BTreeSet::new();
        for cell in &after.visible_cells {
            if let Some(door) = &cell.door {
                if old
                    .get(&cell.key)
                    .is_some_and(|old| old.wall && old.door.is_none())
                    && discovered.insert(door.id)
                {
                    facts.push(OutcomeFact::DoorDiscovered);
                }
            }
        }
        if receipts.map_or(
            before.tick != after.tick && after.motion.as_ref().is_some_and(|m| m.displaced),
            |r| r.displaced,
        ) {
            facts.push(OutcomeFact::Displaced);
        }
        if receipts.map_or(
            before.tick != after.tick && after.motion.as_ref().is_some_and(|m| m.impacted),
            |r| r.impacted,
        ) {
            facts.push(OutcomeFact::Impacted);
        }
        let actors = |view: &OutcomeObservation| -> BTreeMap<ActorTarget, String> {
            view.visible_actors
                .iter()
                .filter(|actor| actor.id != view.self_target)
                .map(|actor| (actor.id, actor.name.clone()))
                .collect()
        };
        let old = actors(before);
        let new = actors(after);
        for (actor, name) in &new {
            if !old.contains_key(actor) {
                facts.push(OutcomeFact::Noticed {
                    actor: *actor,
                    name: name.clone(),
                });
            }
        }
        for (actor, name) in &old {
            if !new.contains_key(actor) {
                facts.push(OutcomeFact::LostSight {
                    actor: *actor,
                    name: name.clone(),
                });
            }
        }
        let doors = |view: &OutcomeObservation| -> BTreeMap<DoorTarget, (bool, String)> {
            view.visible_cells
                .iter()
                .filter_map(|cell| cell.door.as_ref())
                .map(|door| (door.id, (door.open, door.name.clone())))
                .collect()
        };
        let acted = match event {
            Some(Event::DoorChanged { door, .. }) => Some(*door),
            _ => None,
        };
        let old = doors(before);
        for (id, (open, name)) in doors(after) {
            if Some(id) != acted && old.get(&id).is_some_and(|(was, _)| *was != open) {
                facts.push(OutcomeFact::DoorChanged { name, open });
            }
        }
        if before.ready != after.ready && after.combat.as_ref().is_none_or(|c| !c.status.terminal) {
            facts.push(OutcomeFact::Pacing { ready: after.ready });
        }
        if let Some(interactions) = &after.interactions {
            if receipts.is_some()
                || !(before.tick == after.tick
                    && before.interactions.as_ref().is_some_and(|old| {
                        old.completed == interactions.completed
                            && old.refused == interactions.refused
                            && old.curse_blocked == interactions.curse_blocked
                    }))
            {
                let name = |id| {
                    before
                        .inventory
                        .iter()
                        .chain(&after.inventory)
                        .find(|item| item.id == id)
                        .map(|item| item.name.clone())
                };
                for action in interactions
                    .completed
                    .iter()
                    .skip(receipts.map_or(0, |r| r.completed))
                {
                    if let Some((work, item)) = ItemWorkKind::from_action(action) {
                        facts.push(OutcomeFact::ItemCompleted {
                            work,
                            name: name(item),
                        });
                    }
                }
                for refusal in interactions
                    .refused
                    .iter()
                    .skip(receipts.map_or(0, |r| r.refused))
                {
                    facts.push(OutcomeFact::ItemRefused {
                        reason: refusal.reason,
                    });
                }
                for item in interactions
                    .curse_blocked
                    .iter()
                    .skip(receipts.map_or(0, |r| r.curse))
                {
                    facts.push(OutcomeFact::CurseBlocked { name: name(*item) });
                }
            }
        }
        let mut referenced = BTreeSet::new();
        if let Some(combat) = &after.combat {
            if let Some(record) = combat.public_plan.as_ref().filter(|record| {
                before
                    .combat
                    .as_ref()
                    .and_then(|old| old.public_plan.as_ref())
                    != Some(*record)
            }) {
                facts.push(OutcomeFact::PublicPlan {
                    record: record.clone(),
                });
            }
            if receipts.is_some() || before.tick != after.tick || legacy_combat_changed {
                for event in combat.events.iter().skip(receipts.map_or(0, |r| r.combat)) {
                    let basic_attack = matches!(event, CombatEventView::Ability { caster:Some(caster), ability,.. }
                        if *caster == after.self_target && basic(ability));
                    referenced.extend(event_subjects(event));
                    facts.push(OutcomeFact::Combat {
                        event: event.clone(),
                        basic_attack,
                    });
                }
                if let Some(old) = &before.combat {
                    let dying: BTreeSet<_> = combat
                        .events
                        .iter()
                        .filter_map(|event| match event {
                            CombatEventView::Died { actor } => Some(*actor),
                            _ => None,
                        })
                        .collect();
                    for now in &combat.actors {
                        if old
                            .actors
                            .iter()
                            .find(|was| was.actor == now.actor)
                            .is_some_and(|was| was.injury != now.injury)
                            && now.actor != after.self_target
                            && !dying.contains(&now.actor)
                        {
                            referenced.insert(now.actor);
                            facts.push(OutcomeFact::Injured {
                                actor: now.actor,
                                injury: now.injury,
                            });
                        }
                    }
                }
                if before.combat.as_ref().is_none_or(|old| {
                    old.status.hp != combat.status.hp
                        || old.status.victory != combat.status.victory
                        || old.status.dead != combat.status.dead
                        || old.status.sequence != combat.status.sequence
                        || (combat.status.sequence.is_some()
                            && old.status.preparation_active != combat.status.preparation_active)
                }) {
                    facts.push(OutcomeFact::Status {
                        status: combat.status.clone(),
                    });
                }
            }
        }
        if let Some(objective) = after.combat.as_ref().and_then(|combat| combat.objective) {
            if before
                .combat
                .as_ref()
                .is_none_or(|old| old.objective.is_none())
            {
                facts.push(OutcomeFact::Objective { objective });
            }
        }
        let actor_names = referenced
            .into_iter()
            .filter(|id| *id != after.self_target)
            .filter_map(|id| {
                after
                    .visible_actors
                    .iter()
                    .chain(&before.visible_actors)
                    .find(|actor| actor.id == id && !actor.name.trim().is_empty())
                    .map(|actor| DisclosedActorName {
                        actor: id,
                        name: actor.name.clone(),
                    })
            })
            .collect();
        Self {
            self_target: after.self_target,
            status: after.combat.as_ref().map(|combat| combat.status.clone()),
            actor_names,
            facts,
        }
    }

    /// Validate before accepting into a client stream. Outer response encoding
    /// still enforces envelope bytes; this independently bounds retained facts.
    pub fn validate_at(&self, tick: u64) -> Result<(), InvalidState> {
        self.validate()?;
        if self.facts.iter().any(|fact| {
            matches!(fact,
            OutcomeFact::PublicPlan { record } if !record.valid_at(tick))
        }) {
            return Err(InvalidState::InvalidCombat);
        }
        Ok(())
    }

    pub fn validate(&self) -> Result<(), InvalidState> {
        if self.facts.len() > MAX_OUTCOME_FACTS
            || self.actor_names.len() > MAX_OUTCOME_FACTS
            || !matches!(crate::encoded_length(self, MAX_STATE_BYTES), Ok(Some(_)))
        {
            return Err(InvalidState::StateTooLarge);
        }
        if self.status.as_ref().is_some_and(|status| !status.valid()) {
            return Err(InvalidState::InvalidCombat);
        }
        let mut names = BTreeSet::new();
        let referenced: BTreeSet<_> = self
            .facts
            .iter()
            .flat_map(|fact| match fact {
                OutcomeFact::Combat { event, .. } => event_subjects(event),
                OutcomeFact::Injured { actor, .. } => vec![*actor],
                _ => Vec::new(),
            })
            .collect();
        if self.actor_names.iter().any(|entry| {
            entry.actor == self.self_target
                || !names.insert(entry.actor)
                || !referenced.contains(&entry.actor)
        }) {
            return Err(InvalidState::InvalidCombat);
        }
        for fact in &self.facts {
            match fact {
                OutcomeFact::Combat {
                    event:
                        CombatEventView::Ability {
                            ability, strike, ..
                        },
                    ..
                } if !ability.valid()
                    || strike.is_some_and(|ordinal| ordinal >= crate::MAX_SEQUENCE_STRIKES) =>
                {
                    return Err(InvalidState::InvalidCombat)
                }
                OutcomeFact::Combat {
                    event: CombatEventView::Condition { condition },
                    ..
                } if !condition.valid() => return Err(InvalidState::InvalidCombat),
                OutcomeFact::OwnAction {
                    event,
                    basic_attack: true,
                    ..
                } if !matches!(event, Event::AbilityStarted { .. }) => {
                    return Err(InvalidState::InvalidCombat)
                }
                OutcomeFact::Combat {
                    event,
                    basic_attack: true,
                } if !matches!(event, CombatEventView::Ability { caster:Some(caster),.. } if *caster == self.self_target) => {
                    return Err(InvalidState::InvalidCombat)
                }
                OutcomeFact::Status { status } if !status.valid() => {
                    return Err(InvalidState::InvalidCombat)
                }
                _ => {}
            }
        }
        Ok(())
    }
}
fn event_subjects(event: &CombatEventView) -> Vec<ActorTarget> {
    match event {
        CombatEventView::UnseenAbility { target, .. } => vec![*target],
        CombatEventView::AnonymousAbility { caster, target, .. }
        | CombatEventView::Ability { caster, target, .. } => {
            caster.iter().chain(target.iter()).copied().collect()
        }
        CombatEventView::Died { actor } => vec![*actor],
        _ => Vec::new(),
    }
}
impl OutcomeStatusFacts {
    fn valid(&self) -> bool {
        self.hp <= self.max_hp
            && self.dead == (self.hp == 0)
            && (!self.preparation_active || self.preparation_remaining.is_some())
            && self.sequence.as_ref().is_none_or(|sequence| {
                (2..=crate::MAX_SEQUENCE_STRIKES).contains(&sequence.total)
                    && sequence.completed < sequence.total
                    && self.preparation_remaining == Some(sequence.next_strike_remaining)
                    && self.recovery_remaining == 0
                    && if sequence.completed == 0 {
                        sequence.cancel_recovery_remaining == 0
                    } else {
                        sequence.cancel_recovery_remaining >= sequence.next_strike_remaining
                    }
                    && !self.dead
            })
    }
}

impl OutcomeFacts {
    /// Durable disclosed picture comparison. Batch-local feedback and the global
    /// tick never create a new visual demand merely by repeating or disappearing.
    pub fn picture_changed(before: &Observation, after: &Observation) -> bool {
        fn combat_equal(a: &Option<CombatView>, b: &Option<CombatView>) -> bool {
            match (a, b) {
                (None, None) => true,
                (Some(a), Some(b)) => {
                    let CombatView {
                        own_stats,
                        hp,
                        max_hp,
                        preparation_remaining,
                        preparation_active,
                        sequence,
                        public_plan: _,
                        recovery_remaining,
                        actors,
                        events: _,
                        objective,
                        exit,
                        victory,
                        dead,
                        terminal,
                    } = a;
                    own_stats == &b.own_stats
                        && hp == &b.hp
                        && max_hp == &b.max_hp
                        && preparation_remaining == &b.preparation_remaining
                        && preparation_active == &b.preparation_active
                        && sequence == &b.sequence
                        && recovery_remaining == &b.recovery_remaining
                        && actors == &b.actors
                        && objective == &b.objective
                        && exit == &b.exit
                        && victory == &b.victory
                        && dead == &b.dead
                        && terminal == &b.terminal
                }
                _ => false,
            }
        }
        type InteractionPicture<'a> = (
            &'a Option<IdentificationChoiceView>,
            &'a [EquipmentSlot],
            &'a Option<PreparationView>,
            &'a [ItemInteractionView],
        );
        fn interactions(view: &Option<InteractionView>) -> Option<InteractionPicture<'_>> {
            let view = view.as_ref()?;
            let InteractionView {
                identification,
                completed: _,
                refused: _,
                curse_blocked: _,
                slots,
                preparation,
                inventory,
            } = view;
            // Simulation may construct this wrapper only to carry a batch receipt.
            (identification.is_some()
                || !slots.is_empty()
                || preparation.is_some()
                || !inventory.is_empty())
            .then_some((
                identification,
                slots.as_slice(),
                preparation,
                inventory.as_slice(),
            ))
        }
        fn motion(view: &Option<MotionView>) -> Option<([i64; 3], u32)> {
            let MotionView {
                velocity,
                units_per_cell,
                displaced: _,
                impacted: _,
            } = view.as_ref()?;
            (*velocity != [0; 3]).then_some((*velocity, *units_per_cell))
        }
        let Observation {
            charts,
            self_appearance,
            interactions: old_interactions,
            combat,
            motion: old_motion,
            places,
            actor,
            self_target,
            linked_holds,
            tick: _,
            position,
            visible_cells,
            ground_items,
            inventory,
            visible_actors,
            ready,
        } = before;
        charts != &after.charts
            || self_appearance != &after.self_appearance
            || interactions(old_interactions) != interactions(&after.interactions)
            || !combat_equal(combat, &after.combat)
            || motion(old_motion) != motion(&after.motion)
            || places != &after.places
            || actor != &after.actor
            || self_target != &after.self_target
            || linked_holds != &after.linked_holds
            || position != &after.position
            || visible_cells != &after.visible_cells
            || ground_items != &after.ground_items
            || inventory != &after.inventory
            || visible_actors != &after.visible_actors
            || ready != &after.ready
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn observation() -> Observation {
        Observation {
            charts: vec![],
            self_appearance: None,
            interactions: None,
            combat: Some(CombatView {
                own_stats: None,
                hp: 10,
                max_hp: 10,
                preparation_remaining: None,
                preparation_active: false,
                sequence: None,
                public_plan: None,
                recovery_remaining: 0,
                actors: vec![],
                events: vec![],
                objective: None,
                exit: None,
                victory: false,
                dead: false,
                terminal: false,
            }),
            motion: None,
            places: vec![],
            actor: ActorId(1),
            self_target: ActorTarget::from_digest([1; 32]),
            linked_holds: vec![],
            tick: 1,
            position: Position { x: 0, y: 0, z: 0 },
            visible_cells: vec![],
            ground_items: vec![],
            inventory: vec![],
            visible_actors: vec![],
            ready: true,
        }
    }

    #[test]
    fn compact_capture_preserves_discovery_order_and_receipt_prefix() {
        let mut before = OutcomeObservation::from(&observation());
        before.visible_cells.push(OutcomeCell {
            key: "door-cell".into(),
            wall: true,
            door: None,
            trap: None,
        });
        let mut after = before.clone();
        after.visible_cells[0].wall = false;
        after.visible_cells[0].door = Some(OutcomeDoor {
            id: DoorTarget::from_digest([2; 32]),
            open: true,
            name: "door".into(),
        });
        after.visible_cells[0].trap = Some(OutcomeTrap {
            id: TrapTarget::from_digest([3; 32]),
            family: None,
        });
        after.motion = Some(OutcomeMotion {
            displaced: true,
            impacted: false,
        });
        let old = EventProvenance {
            batch: 1,
            through: 0,
        };
        let new = EventProvenance {
            batch: 2,
            through: 0,
        };
        let captured =
            OutcomeFacts::capture_compact_causal(&before, &after, None, old, new).unwrap();
        assert_eq!(
            captured.facts,
            vec![
                OutcomeFact::TrapDiscovered { family: None },
                OutcomeFact::DoorDiscovered,
                OutcomeFact::Displaced,
            ]
        );
        assert!(captured.validate_at(after.tick).is_ok());
        assert!(
            OutcomeFacts::capture_compact_causal(&after, &after, None, new, new)
                .unwrap()
                .facts
                .is_empty()
        );
        after
            .combat
            .as_mut()
            .unwrap()
            .events
            .push(CombatEventView::AnonymousAbility {
                caster: None,
                target: None,
                outcome: AbilityOutcome::Applied,
            });
        assert_eq!(
            OutcomeFacts::capture_compact_causal(&before, &after, None, new, new),
            Err(InvalidState::InvalidCombat)
        );
    }

    #[test]
    fn compact_basic_attack_lookup_keeps_current_false_before_prior_true() {
        let mut before = OutcomeObservation::from(&observation());
        before
            .combat
            .as_mut()
            .unwrap()
            .abilities
            .push(OutcomeAbility {
                id: "strike".into(),
                basic_attack: true,
            });
        let mut after = before.clone();
        after.combat.as_mut().unwrap().abilities[0].basic_attack = false;
        let event = Event::AbilityStarted {
            ability: ContentRef {
                id: "strike".into(),
                name: "Strike".into(),
            },
            target: after.self_target,
        };
        let stamp = EventProvenance {
            batch: 1,
            through: 0,
        };
        let capture = |after: &OutcomeObservation| {
            OutcomeFacts::capture_compact_causal(&before, after, Some(&event), stamp, stamp)
                .unwrap()
        };
        assert!(matches!(
            capture(&after).facts[0],
            OutcomeFact::OwnAction {
                basic_attack: false,
                ..
            }
        ));
        after.combat.as_mut().unwrap().abilities.clear();
        assert!(matches!(
            capture(&after).facts[0],
            OutcomeFact::OwnAction {
                basic_attack: true,
                ..
            }
        ));
    }

    #[test]
    fn legacy_capture_preserves_full_combat_change_gate() {
        let mut before = observation();
        before
            .combat
            .as_mut()
            .unwrap()
            .events
            .push(CombatEventView::AnonymousAbility {
                caster: None,
                target: None,
                outcome: AbilityOutcome::Applied,
            });
        let mut after = before.clone();
        after.combat.as_mut().unwrap().exit = Some("new-exit".into());
        assert_eq!(
            OutcomeObservation::from(&before),
            OutcomeObservation::from(&after)
        );
        assert!(OutcomeFacts::capture(&before, &before, None)
            .facts
            .is_empty());
        assert!(matches!(
            OutcomeFacts::capture(&before, &after, None)
                .facts
                .as_slice(),
            [OutcomeFact::Combat { .. }]
        ));
        let stamp = EventProvenance {
            batch: 1,
            through: 1,
        };
        assert!(
            OutcomeFacts::capture_causal(&before, &after, None, stamp, stamp)
                .unwrap()
                .facts
                .is_empty()
        );
    }

    #[test]
    fn outcome_validation_rejects_unreferenced_names_and_false_basic_identity() {
        let actor = ActorTarget::from_digest([1; 32]);
        let other = ActorTarget::from_digest([2; 32]);
        let mut facts = OutcomeFacts {
            self_target: actor,
            status: None,
            actor_names: vec![],
            facts: vec![],
        };
        assert!(facts.validate().is_ok());
        facts.actor_names.push(DisclosedActorName {
            actor: other,
            name: "figure".into(),
        });
        assert_eq!(facts.validate(), Err(InvalidState::InvalidCombat));
        facts.actor_names.clear();
        facts.facts.push(OutcomeFact::OwnAction {
            event: Event::Waited,
            name: None,
            basic_attack: true,
        });
        assert_eq!(facts.validate(), Err(InvalidState::InvalidCombat));
        facts.facts = vec![OutcomeFact::Combat {
            event: CombatEventView::AnonymousAbility {
                caster: Some(other),
                target: Some(actor),
                outcome: AbilityOutcome::Applied,
            },
            basic_attack: true,
        }];
        assert_eq!(facts.validate(), Err(InvalidState::InvalidCombat));
    }
}
