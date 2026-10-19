//! Exact-unit release and recovery live only within the caller's action
//! transaction. No in-flight item or second payment ledger is persisted.
use super::{FrozenWeaponSelection, ProjectileSource, RuleError};
use crate::combat::{CombatEvent, Preparation, SequenceEnd};
use crate::lifecycle::ActorOutcome;
use crate::projectile_profiles::HitRecovery;
use crate::supplies::{ItemClaimKey, ItemPayment};
use crate::{ActorId, Game, GameError, Item, ItemId, ItemLocation, ScheduleError};
use tor_world::Location;

struct ReleasedProjectile {
    id: ItemId,
    knowledge_source: ItemId,
    item: Item,
}

impl Game {
    fn extract_projectile_unit(
        &mut self,
        preparation: &mut Preparation,
    ) -> Result<ReleasedProjectile, GameError> {
        if !matches!(preparation.item_payment, ItemPayment::Held { .. })
            || !self.preparation_items_valid(preparation, false)
        {
            return Err(GameError::ItemUnavailable);
        }
        let references = preparation.item_payment.refs();
        let [reference] = references else {
            return Err(GameError::ItemUnavailable);
        };
        let source = reference.item;
        let key = ItemClaimKey {
            lineage: preparation
                .item_lineage()
                .ok_or(GameError::ItemUnavailable)?,
            cost_ordinal: reference.cost_ordinal,
        };
        let original = self.items.get(&source).ok_or(GameError::ItemUnavailable)?;
        let index = original
            .holds
            .binary_search_by_key(&key, |hold| hold.key)
            .map_err(|_| GameError::ItemUnavailable)?;
        if original.holds[index].quantity != 1 {
            return Err(GameError::ItemUnavailable);
        }
        let mut item = original.clone();
        let id = if original.quantity == 1 {
            if original.holds.len() != 1 {
                return Err(GameError::ItemUnavailable);
            }
            self.items.remove(&source);
            source
        } else {
            let next = self
                .next_item_id
                .checked_add(1)
                .ok_or(GameError::IdentityExhausted)?;
            self.items
                .edit(source, |remaining| {
                    remaining.quantity -= 1;
                    remaining.holds.remove(index);
                })
                .ok_or(GameError::ItemUnavailable)?;
            let id = ItemId(self.next_item_id);
            self.next_item_id = next;
            id
        };
        item.quantity = 1;
        item.holds.clear();
        item.motion = crate::MotionState::default();
        item.orientation = 0;
        preparation.item_payment = ItemPayment::Settled;
        Ok(ReleasedProjectile {
            id,
            knowledge_source: source,
            item,
        })
    }

    /// `completion` already owns resource settlement. Refresh that exact state
    /// when releasing equipped gear; never replace it with a fresh actor clone.
    pub(crate) fn complete_projectile_release(
        &mut self,
        mut preparation: Preparation,
        mut completion: ActorOutcome,
        due: u64,
    ) -> Result<(), GameError> {
        let plan = preparation
            .plan
            .as_projectile()
            .cloned()
            .ok_or(GameError::Rule(RuleError::InvalidPlan))?;
        let actor = plan.actor;
        let item = plan.source.item().ok_or(GameError::ItemUnavailable)?;
        if !plan.source.available(self, actor) {
            return Err(GameError::ItemUnavailable);
        }
        completion.state.ready_at = due
            .checked_add(plan.recovery)
            .ok_or(GameError::Schedule(ScheduleError::TimeExhausted))?;
        preparation.hold = None;
        if matches!(plan.source, ProjectileSource::ThrowEquipped { .. })
            && self
                .items
                .get(&item)
                .is_some_and(|item| item.condition.cursed)
        {
            completion.state.known_equipment.learn_curse(item, true)?;
            // No extraction: ordinary completion releases the exact unpaid item
            // claim while retaining equipment and consuming the attempted work.
            self.apply_outcome(completion)?;
            self.push_event(
                CombatEvent::ItemCurseBlocked {
                    actor,
                    item,
                    intention: preparation.intention,
                },
                None,
            )?;
            return self.finish_projectile_sequence(actor, preparation.intention);
        }
        if let ProjectileSource::ThrowEquipped {
            weapon: FrozenWeaponSelection::Equipped { item, socket, .. },
        } = &plan.source
        {
            let mut equipment = completion.state.equipment.clone();
            if equipment.remove(socket) != Some(*item) {
                return Err(GameError::ItemUnavailable);
            }
            self.refresh_equipment_outcome(&mut completion, equipment)?;
        }
        completion.spent_items = preparation.item_lineage();
        let released = self.extract_projectile_unit(&mut preparation)?;
        self.apply_outcome(completion)?;
        let impact = self.trace_projectile_flight(
            actor,
            plan.origin,
            plan.direction,
            &plan.launch_footprint,
            plan.range,
        )?;
        let successful_hit = if let Some(target) = impact.target {
            self.resolve_projectile_encounter(
                &plan,
                target,
                impact.distance_feet,
                preparation.intention,
            )?
            .check
            .is_some_and(|check| check.success)
        } else {
            false
        };
        let consumed = successful_hit
            && matches!(&plan.source,
            ProjectileSource::Launch { payload, .. } if payload.recovery == HitRecovery::ConsumedOnHit);
        let mut discarded = consumed;
        if !consumed {
            let landing = if successful_hit {
                // Ground item validity requires a loaded cell, not support. This
                // preserves durable hits on flying/multicell/self-return targets.
                if !self.world.contains(impact.location) {
                    return Err(GameError::InvalidLocation);
                }
                Some(impact.location)
            } else {
                self.projectile_miss_landing(actor, impact.location)?
            };
            if let Some(location) = landing {
                self.land_projectile_unit(actor, released, location)?;
            } else {
                discarded = true;
            }
            // With no eligible miss landing, pinned Rogue recovery loses the unit.
        }
        if discarded && self.survival_diagnostics.is_some() {
            let mut flow = crate::survival_diagnostics::SurvivalFlows {
                items_consumed: 1,
                ..Default::default()
            };
            flow.consumed_items.insert(item, 1);
            self.record_survival(actor, &flow);
        }
        self.finish_projectile_sequence(actor, preparation.intention)
    }

    fn finish_projectile_sequence(
        &mut self,
        actor: ActorId,
        intention: Option<crate::IntentionId>,
    ) -> Result<(), GameError> {
        self.push_event(
            CombatEvent::SequenceEnded {
                actor,
                intention,
                reason: SequenceEnd::Completed,
            },
            None,
        )
    }

    fn projectile_miss_landing(
        &mut self,
        actor: ActorId,
        center: Location,
    ) -> Result<Option<Location>, GameError> {
        let mut candidates = Vec::with_capacity(9);
        for y in -1..=1 {
            for x in -1..=1 {
                let Some(x) = center.position.x.checked_add(x) else {
                    continue;
                };
                let Some(y) = center.position.y.checked_add(y) else {
                    continue;
                };
                let at = Location {
                    region: center.region,
                    position: tor_world::Position {
                        x,
                        y,
                        z: center.position.z,
                    },
                };
                if self.world.walkable(at)
                    && !self.region_frozen(at.region)
                    && !self.actors.at(&self.world, at).contains_key(&actor)
                {
                    candidates.push(at);
                }
            }
        }
        if candidates.is_empty() {
            return Ok(None);
        }
        let index = crate::quantity::QuantityRange::new(1, candidates.len() as u64)?
            .sample(&mut self.rules_rng)?
            - 1;
        Ok(Some(candidates[index as usize]))
    }

    fn land_projectile_unit(
        &mut self,
        actor: ActorId,
        mut released: ReleasedProjectile,
        location: Location,
    ) -> Result<(), GameError> {
        released.item.location = ItemLocation::Ground(location);
        if !self.item_state_valid(released.id, &released.item) {
            return Err(GameError::ItemUnavailable);
        }
        let destination = self.items.at(released.item.location).find(|id| {
            self.merge_destination_available(*id)
                && self.items[id].stacks_with(&released.item)
                && self.items[id].motion == crate::MotionState::default()
                && self.items[id].orientation == 0
        });
        self.items.insert(released.id, released.item);
        let transfer = self.prepare_stack_transfer(
            released.id,
            destination,
            1,
            ItemLocation::Ground(location),
            None,
        )?;
        let inherited = self.actors.get(&actor).and_then(|owner| {
            owner.known_equipment.inherited_transfer(
                released.knowledge_source,
                destination,
                transfer.result(),
            )
        });
        self.apply_prepared_transfer(&transfer)?;
        if let Some(facts) = inherited {
            if let Some(mut owner) = self.actors.get_mut(&actor) {
                owner.known_equipment.remember_inherited(facts);
            }
        }
        Ok(())
    }
}
