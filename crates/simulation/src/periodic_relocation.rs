//! Exact equipped sources own independent actor-active relocation trials.
//! Equipped fractions live only in the actor book; unequipped fractions live
//! only on their item. Observation projects clocks but never rolls or relocates.
use crate::{ActorId, Game, GameError, ItemId};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PeriodicRelocation {
    pub interval: u64,
    pub chance_denominator: u32,
}
impl PeriodicRelocation {
    pub fn valid(self) -> bool {
        (1..=crate::limits::NUMERIC as u64).contains(&self.interval)
            && (1..=crate::limits::NUMERIC as u32).contains(&self.chance_denominator)
    }
}
impl<'de> Deserialize<'de> for PeriodicRelocation {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            interval: u64,
            chance_denominator: u32,
        }
        let f: Fields = crate::bounded::named_map(d)?;
        let value = Self {
            interval: f.interval,
            chance_denominator: f.chance_denominator,
        };
        if !value.valid() {
            return Err(serde::de::Error::custom("invalid periodic relocation"));
        }
        Ok(value)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
struct SourceClock {
    item: ItemId,
    policy: PeriodicRelocation,
    elapsed: u64,
}
impl<'de> Deserialize<'de> for SourceClock {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            item: ItemId,
            policy: PeriodicRelocation,
            elapsed: u64,
        }
        let f: Fields = crate::bounded::named_map(d)?;
        if f.item.0 == 0 || f.elapsed >= f.policy.interval {
            return Err(serde::de::Error::custom("invalid relocation source clock"));
        }
        Ok(Self {
            item: f.item,
            policy: f.policy,
            elapsed: f.elapsed,
        })
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
struct PendingTrial {
    item: ItemId,
    policy: PeriodicRelocation,
}
impl<'de> Deserialize<'de> for PendingTrial {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            item: ItemId,
            policy: PeriodicRelocation,
        }
        let f: Fields = crate::bounded::named_map(d)?;
        if f.item.0 == 0 {
            return Err(serde::de::Error::custom("invalid relocation trial"));
        }
        Ok(Self {
            item: f.item,
            policy: f.policy,
        })
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RelocationBook {
    sources: Vec<SourceClock>,
    pending: Vec<PendingTrial>,
}
impl<'de> Deserialize<'de> for RelocationBook {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            sources: crate::bounded::Bounded<SourceClock, { crate::limits::EQUIPMENT_SOCKETS }>,
            pending: crate::bounded::Bounded<PendingTrial, { crate::limits::EQUIPMENT_SOCKETS }>,
        }
        let f: Fields = crate::bounded::named_map(d)?;
        if !f
            .sources
            .0
            .windows(2)
            .all(|pair| pair[0].item < pair[1].item)
            || !f
                .pending
                .0
                .windows(2)
                .all(|pair| pair[0].item < pair[1].item)
        {
            return Err(serde::de::Error::custom("unordered relocation sources"));
        }
        Ok(Self {
            sources: f.sources.0,
            pending: f.pending.0,
        })
    }
}
impl RelocationBook {
    pub(crate) fn pending(&self) -> bool {
        !self.pending.is_empty()
    }
    pub(crate) fn next_in(&self, alive: bool) -> Option<u64> {
        alive
            .then(|| {
                self.sources
                    .iter()
                    .map(|clock| clock.policy.interval - clock.elapsed)
                    .min()
            })
            .flatten()
    }
    /// A scheduler stops at the earliest source deadline. Reject implicit
    /// catch-up rather than rolling several historical relocations at one place.
    pub(crate) fn advance(&mut self, ticks: u64) -> Result<(), GameError> {
        if ticks == 0 {
            return Ok(());
        }
        if self.pending() || self.next_in(true).is_some_and(|next| ticks > next) {
            return Err(GameError::Schedule(crate::ScheduleError::TimeExhausted));
        }
        for clock in &mut self.sources {
            clock.elapsed += ticks;
            if clock.elapsed == clock.policy.interval {
                clock.elapsed = 0;
                self.pending.push(PendingTrial {
                    item: clock.item,
                    policy: clock.policy,
                });
            }
        }
        Ok(())
    }
    pub(crate) fn valid_for(
        &self,
        actor: &crate::Actor,
        items: &BTreeMap<ItemId, crate::Item>,
    ) -> bool {
        if self.pending() {
            return false;
        }
        let expected: Vec<_> = actor
            .equipment
            .values()
            .copied()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .filter_map(|id| item_policy(items.get(&id)?).map(|policy| (id, policy)))
            .collect();
        expected.len() == self.sources.len()
            && expected
                .iter()
                .zip(&self.sources)
                .all(|((id, policy), clock)| {
                    *id == clock.item
                        && *policy == clock.policy
                        && clock.elapsed < policy.interval
                        && items
                            .get(id)
                            .is_some_and(|item| item.relocation_elapsed.is_none())
                })
    }
}
pub(crate) fn item_policy(item: &crate::Item) -> Option<PeriodicRelocation> {
    item.spec.equipment.as_ref()?.periodic_relocation
}
pub(crate) fn item_clock_valid(item: &crate::Item, equipped: bool) -> bool {
    let Some(policy) = item_policy(item) else {
        return item.relocation_elapsed.is_none();
    };
    if equipped {
        item.relocation_elapsed.is_none()
    } else {
        item.relocation_elapsed
            .is_some_and(|elapsed| elapsed < policy.interval)
    }
}

impl Game {
    /// The old book has already been projected to this boundary. Preserve its
    /// earned trials even if their source is removed by the same paid action.
    pub(crate) fn reconcile_relocation_sources(
        &self,
        outcome: &mut crate::lifecycle::ActorOutcome,
    ) -> Result<(), GameError> {
        let alive = outcome.state.alive();
        let book = &mut outcome.state.periodic_relocations;
        let mut sources = Vec::new();
        for id in outcome
            .state
            .equipment
            .values()
            .copied()
            .collect::<BTreeSet<_>>()
        {
            let item = self.items.get(&id).ok_or(GameError::ItemUnavailable)?;
            let Some(policy) = item_policy(item) else {
                continue;
            };
            let elapsed = if let Some(clock) = book.sources.iter().find(|clock| clock.item == id) {
                if clock.policy != policy {
                    return Err(GameError::ItemUnavailable);
                }
                clock.elapsed
            } else {
                item.relocation_elapsed.ok_or(GameError::ItemUnavailable)?
            };
            if elapsed >= policy.interval {
                return Err(GameError::ItemUnavailable);
            }
            sources.push(SourceClock {
                item: id,
                policy,
                elapsed,
            });
        }
        book.sources = sources;
        if !alive {
            book.pending.clear();
        }
        Ok(())
    }

    /// Transfer fraction ownership, never duplicate it. Publication is already
    /// inside the caller's candidate transaction; this method draws no RNG.
    pub(crate) fn publish_relocation_sources(
        &mut self,
        outcome: &crate::lifecycle::ActorOutcome,
    ) -> Result<(), GameError> {
        let previous = self
            .actors
            .get(&outcome.id)
            .ok_or(GameError::UnknownActor)?;
        let elapsed = outcome
            .state
            .active_since
            .saturating_sub(previous.active_since);
        let next = &outcome.state.periodic_relocations.sources;
        let retired: Vec<_> = previous
            .periodic_relocations
            .sources
            .iter()
            .filter(|clock| !next.iter().any(|next| next.item == clock.item))
            .map(|clock| {
                if elapsed > clock.policy.interval - clock.elapsed {
                    return Err(GameError::Schedule(crate::ScheduleError::TimeExhausted));
                }
                Ok((
                    clock.item,
                    (clock.elapsed + elapsed) % clock.policy.interval,
                ))
            })
            .collect::<Result<_, GameError>>()?;
        for (id, elapsed) in retired {
            self.items
                .edit(id, |item| item.relocation_elapsed = Some(elapsed))
                .ok_or(GameError::ItemUnavailable)?;
        }
        for clock in next {
            self.items
                .edit(clock.item, |item| item.relocation_elapsed = None)
                .ok_or(GameError::ItemUnavailable)?;
        }
        Ok(())
    }

    pub(crate) fn relocation_item_state_valid(&self, id: ItemId, item: &crate::Item) -> bool {
        let equipped = match item.location {
            crate::ItemLocation::Carried(owner) => self
                .actors
                .get(&owner)
                .is_some_and(|actor| actor.equipment.values().any(|owned| *owned == id)),
            crate::ItemLocation::Ground(_) => false,
        };
        item_clock_valid(item, equipped)
    }

    pub(crate) fn settle_periodic_relocations(&mut self) -> Result<(), GameError> {
        let owners: Vec<_> = self.actors.pending_relocations().collect();
        for id in owners {
            self.settle_periodic_relocations_for(id)?;
        }
        Ok(())
    }
    pub(crate) fn settle_periodic_relocations_for(&mut self, id: ActorId) -> Result<(), GameError> {
        let Some(mut actor) = self.actors.get_mut(&id) else {
            return Ok(());
        };
        // Retire the entire earned batch before any relocation/contact can
        // reenter lifecycle settlement. Exact ItemId order is deterministic.
        let pending = std::mem::take(&mut actor.periodic_relocations.pending);
        let alive = actor.alive();
        drop(actor);
        if !alive {
            return Ok(());
        }
        for trial in pending {
            if self.actors.get(&id).is_none_or(|actor| !actor.alive()) {
                break;
            }
            let mut rng = self.rules_rng;
            let hit =
                crate::quantity::QuantityRange::new(1, u64::from(trial.policy.chance_denominator))?
                    .sample(&mut rng)?
                    == 1;
            self.rules_rng = rng;
            if hit {
                self.relocate_random_in_transaction(id, &mut rng, false)?;
                self.rules_rng = rng;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn source(id: u64, elapsed: u64) -> SourceClock {
        SourceClock {
            item: ItemId(id),
            policy: PeriodicRelocation {
                interval: 100,
                chance_denominator: 50,
            },
            elapsed,
        }
    }
    #[test]
    fn exact_sources_keep_distinct_phases_and_never_collapse_duplicate_trials() {
        let mut book = RelocationBook {
            sources: vec![source(7, 0), source(9, 25)],
            pending: vec![],
        };
        assert_eq!(book.next_in(true), Some(75));
        book.advance(75).unwrap();
        assert_eq!(
            book.sources
                .iter()
                .map(|clock| clock.elapsed)
                .collect::<Vec<_>>(),
            [75, 0]
        );
        assert_eq!(
            book.pending
                .iter()
                .map(|trial| trial.item)
                .collect::<Vec<_>>(),
            [ItemId(9)]
        );
        book.pending.clear();
        book.advance(25).unwrap();
        assert_eq!(
            book.pending
                .iter()
                .map(|trial| trial.item)
                .collect::<Vec<_>>(),
            [ItemId(7)]
        );
        let mut duplicate = RelocationBook {
            sources: vec![source(7, 99), source(9, 99)],
            pending: vec![],
        };
        duplicate.advance(1).unwrap();
        assert_eq!(
            duplicate
                .pending
                .iter()
                .map(|trial| trial.item)
                .collect::<Vec<_>>(),
            [ItemId(7), ItemId(9)]
        );
        assert_eq!(duplicate.next_in(false), None);
    }
    #[test]
    fn strict_clocks_reject_catchup_duplicate_identity_and_unknown_or_missing_fields() {
        let book = RelocationBook {
            sources: vec![source(7, 30)],
            pending: vec![],
        };
        let positive = serde_json::to_value(&book).unwrap();
        assert_eq!(
            serde_json::from_value::<RelocationBook>(positive.clone()).unwrap(),
            book
        );
        let mutations: [fn(&mut serde_json::Value); 7] = [
            |value: &mut serde_json::Value| {
                value["sources"][0]["elapsed"] = 100.into();
            },
            |value: &mut serde_json::Value| {
                value["sources"][0]["item"] = 0.into();
            },
            |value: &mut serde_json::Value| {
                value["sources"][0]["policy"]["interval"] = 0.into();
            },
            |value: &mut serde_json::Value| {
                value["sources"][0]["policy"]["chance_denominator"] = 0.into();
            },
            |value: &mut serde_json::Value| {
                value.as_object_mut().unwrap().remove("pending");
            },
            |value: &mut serde_json::Value| {
                value["unknown"] = true.into();
            },
            |value: &mut serde_json::Value| {
                let second = value["sources"][0].clone();
                value["sources"].as_array_mut().unwrap().push(second);
            },
        ];
        for mutate in mutations {
            let mut malformed = positive.clone();
            mutate(&mut malformed);
            assert!(serde_json::from_value::<RelocationBook>(malformed).is_err());
        }
        let mut late = book.clone();
        assert!(late.advance(71).is_err());
        assert_eq!(
            late, book,
            "rejected historical catchup cannot alter fractions"
        );
        late.advance(70).unwrap();
        assert!(
            late.advance(1).is_err(),
            "earned pulse must settle before more active time"
        );
    }
}

#[cfg(test)]
#[path = "periodic_relocation_tests.rs"]
mod contract_tests;
