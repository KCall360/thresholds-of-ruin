//! Checked inventory riders. Authored capabilities become immutable numerical
//! operands; inventory eligibility belongs to the successful live transaction.
use crate::{
    content_ids::FlagId,
    quantity::{QuantityExpr, QuantityRange},
    ActorId, CurrencyKind, Game, GameError, ItemId, ItemLocation,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use tor_world::RegionId;

/// Private-host load boundary, never an observation or persisted work queue.
/// The actor and original/latest intention identify the paid lineage even when
/// several actors start work without advancing the global clock.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct PreparationRegionBoundary {
    pub actor: ActorId,
    pub origin: Option<crate::IntentionId>,
    pub intention: Option<crate::IntentionId>,
    pub started: u64,
    pub members: BTreeSet<RegionId>,
}

#[cfg(test)]
#[path = "theft_merge_tests.rs"]
mod merge_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use std::num::NonZeroU64;
    use tor_world::{Location, Position};

    fn at(x: i32) -> Location {
        Location {
            region: RegionId(1),
            position: Position { x, y: 1, z: 0 },
        }
    }
    fn game() -> Game {
        let mut game = Game::region_corridor(42, 2, crate::test_creatures::content());
        game.spawn_authored_actor(1, at(1), NonZeroU64::new(100).unwrap())
            .unwrap();
        game.spawn_authored_actor(2, at(2), NonZeroU64::new(100).unwrap())
            .unwrap();
        game
    }
    fn gold(name: &str) -> crate::ItemSpec {
        let mut item = crate::ItemSpec::ordinary(name.into());
        item.class = crate::ItemClass::Coin;
        item.currency = Some(CurrencyKind::Gold);
        item.stackable = true;
        item
    }
    fn selection(amount: u64) -> PlannedTheftSelection {
        PlannedTheftSelection::Currency {
            currency: CurrencyKind::Gold,
            amount: QuantityRange::new(amount, amount).unwrap(),
        }
    }

    #[test]
    fn gold_caps_total_purse_across_specs_and_preserves_finite_stock() {
        let mut game = game();
        game.place_item_stack(10, at(2), Some(ActorId(2)), 3, gold("old gold"))
            .unwrap();
        game.place_item_stack(11, at(2), Some(ActorId(2)), 5, gold("new gold"))
            .unwrap();
        game.place_item_stack(12, at(1), Some(ActorId(1)), 2, gold("old gold"))
            .unwrap();
        let mut rng = 9;
        let receipt = game
            .transfer_theft(ActorId(1), ActorId(2), &selection(7), &mut rng)
            .unwrap()
            .unwrap();
        assert_eq!(receipt.quantity(), 7);
        assert_eq!(rng, 9);
        assert_eq!(
            game.items.values().map(|item| item.quantity).sum::<u64>(),
            10
        );
        assert_eq!(game.items[&ItemId(12)].quantity, 5);
        assert_eq!(game.items[&ItemId(11)].quantity, 1);
        let receipt = game
            .transfer_theft(ActorId(1), ActorId(2), &selection(u64::MAX), &mut rng)
            .unwrap()
            .unwrap();
        assert_eq!(receipt.quantity(), 1);
        assert!(game
            .transfer_theft(ActorId(1), ActorId(2), &selection(1), &mut rng)
            .unwrap()
            .is_none());
    }

    #[test]
    fn magical_selection_moves_one_unknown_unit_without_learning_identity() {
        let mut game = game();
        let mut spec = crate::ItemSpec::ordinary("hidden charm".into());
        spec.magical = true;
        spec.concealed = true;
        spec.appearance = "unmarked charm".into();
        spec.stackable = true;
        game.place_item_stack(10, at(2), Some(ActorId(2)), 3, spec)
            .unwrap();
        game.place_item_stack(
            11,
            at(2),
            Some(ActorId(2)),
            1,
            crate::ItemSpec::ordinary("ordinary armor".into()),
        )
        .unwrap();
        let knowledge = game.actors[&ActorId(1)].knowledge.clone();
        let mut rng = 9;
        assert_eq!(
            game.transfer_theft(
                ActorId(1),
                ActorId(2),
                &PlannedTheftSelection::MagicalUnit,
                &mut rng
            )
            .unwrap()
            .unwrap()
            .quantity(),
            1
        );
        assert_eq!(game.items[&ItemId(10)].quantity, 2);
        assert_eq!(game.actors[&ActorId(1)].knowledge, knowledge);
        assert_eq!(rng, 9, "one eligible entry requires no random draw");
        let stolen = game
            .items
            .at(ItemLocation::Carried(ActorId(1)))
            .next()
            .unwrap();
        assert!(game.items[&stolen].spec.magical);
        assert!(game.items[&stolen].spec.concealed);
    }

    #[test]
    fn self_transfer_and_stale_destination_cannot_create_a_receipt() {
        let mut game = game();
        game.place_item_stack(10, at(2), Some(ActorId(2)), 3, gold("gold"))
            .unwrap();
        game.place_item_stack(11, at(1), Some(ActorId(1)), 3, gold("gold"))
            .unwrap();
        let before = game.clone();
        assert!(game
            .transfer_theft(ActorId(2), ActorId(2), &selection(3), &mut 9)
            .unwrap()
            .is_none());
        assert_eq!(game, before);
        let plan = game
            .prepare_owned_transfer(ItemId(10), ActorId(2), ActorId(1), 2)
            .unwrap();
        game.items
            .edit(ItemId(11), |item| {
                item.location = ItemLocation::Ground(at(1))
            })
            .unwrap();
        let before = game.clone();
        assert_eq!(
            game.apply_owned_transfer(plan),
            Err(GameError::ItemUnavailable)
        );
        assert_eq!(game, before);
    }
    #[test]
    fn live_equipment_and_claims_exclude_units_without_identifying_inventory() {
        let mut game = game();
        let mut spec = crate::ItemSpec::ordinary("charm".into());
        spec.magical = true;
        spec.stackable = true;
        game.place_item_stack(10, at(2), Some(ActorId(2)), 3, spec.clone())
            .unwrap();
        game.place_item_stack(11, at(2), Some(ActorId(2)), 1, spec)
            .unwrap();
        let hold = crate::supplies::ItemHold {
            key: crate::supplies::ItemClaimKey {
                lineage: crate::supplies::PreparationLineage {
                    actor: ActorId(2),
                    origin: crate::IntentionId(7),
                },
                cost_ordinal: 0,
            },
            quantity: 2,
        };
        game.items
            .edit(ItemId(10), |item| item.holds.push(hold))
            .unwrap();
        game.actors
            .get_mut(&ActorId(2))
            .unwrap()
            .equipment
            .insert(crate::EquipmentSlotId(0), ItemId(11));
        let mut rng = 9;
        assert_eq!(
            game.transfer_theft(
                ActorId(1),
                ActorId(2),
                &PlannedTheftSelection::MagicalUnit,
                &mut rng
            )
            .unwrap()
            .unwrap()
            .quantity(),
            1
        );
        assert_eq!(game.items[&ItemId(10)].quantity, 2);
        assert_eq!(game.items[&ItemId(10)].holds, vec![hold]);
        assert_eq!(game.items[&ItemId(11)].quantity, 1);
        assert!(game
            .transfer_theft(
                ActorId(1),
                ActorId(2),
                &PlannedTheftSelection::MagicalUnit,
                &mut rng
            )
            .unwrap()
            .is_none());
        assert_eq!(rng, 9);
    }

    #[test]
    fn exact_transfer_rechecks_equipment_and_fresh_identity_absence() {
        for changed_equipment in [false, true] {
            let mut game = game();
            game.place_item_stack(10, at(2), Some(ActorId(2)), 3, gold("gold"))
                .unwrap();
            let plan = game
                .prepare_owned_transfer(ItemId(10), ActorId(2), ActorId(1), 1)
                .unwrap();
            if changed_equipment {
                game.actors
                    .get_mut(&ActorId(2))
                    .unwrap()
                    .equipment
                    .insert(crate::EquipmentSlotId(0), ItemId(10));
            } else {
                // Reusing the reserved fresh ID without advancing the allocator
                // must still be detected by the exact destination witness.
                let forged = game.items[&ItemId(10)].clone();
                game.items.insert(ItemId(game.next_item_id), forged);
            }
            let before = game.clone();
            assert_eq!(
                game.apply_owned_transfer(plan),
                Err(GameError::ItemUnavailable)
            );
            assert_eq!(game, before);
        }
    }

    #[test]
    fn no_legal_landing_keeps_transferred_loot_and_unique_amulet_identity() {
        use crate::floor_activation::{PopulationPlan, PopulationProfile, PreparedScope};
        let mut game = game();
        game.configure_scenario_policy(Default::default(), Some(ActorId(1)))
            .unwrap();
        game.configure_population(PopulationPlan {
            protagonist: ActorId(1),
            carried_item_pools: Default::default(),
            profiles: [(
                "empty".into(),
                PopulationProfile {
                    version: 1,
                    stages: vec![],
                },
            )]
            .into(),
        })
        .unwrap();
        game.register_activation_scope(PreparedScope {
            scope: "floor/7".into(),
            depth: 7,
            profile: "empty".into(),
            members: [RegionId(1)].into(),
            cells: vec![at(1)],
            protected: Default::default(),
            rooms: vec![],
        })
        .unwrap();
        let mut spec = crate::ItemSpec::ordinary("Amulet of Yendor".into());
        spec.class = crate::ItemClass::Amulet;
        spec.magical = true;
        game.place_item_stack(10, at(2), Some(ActorId(2)), 1, spec.clone())
            .unwrap();
        let mut rng = 9;
        let receipt = game
            .transfer_theft(
                ActorId(1),
                ActorId(2),
                &PlannedTheftSelection::MagicalUnit,
                &mut rng,
            )
            .unwrap()
            .unwrap();
        let location = game.actors[&ActorId(1)].location;
        let relocation = PlannedRelocation {
            policy: RelocationPolicy::SameFloor,
            scope: "floor/7".into(),
        };
        assert_eq!(
            game.relocate_after_theft(ActorId(1), &relocation, &receipt, &mut rng)
                .unwrap(),
            None
        );
        assert_eq!(game.actors[&ActorId(1)].location, location);
        assert_eq!(
            game.items[&ItemId(10)].location,
            ItemLocation::Carried(ActorId(1))
        );
        assert_eq!(&*game.items[&ItemId(10)].spec, &spec);
        assert_eq!(rng, 9);
        let before = game.clone();
        assert!(game
            .relocate_after_theft(ActorId(2), &relocation, &receipt, &mut rng)
            .is_err());
        assert_eq!(game, before);
    }

    #[test]
    fn authored_theft_requires_one_checked_success_rider() {
        use crate::rules::{validate_strike_shapes, RiderTrigger, StrikeDef, StrikeSource};
        let rules = crate::test_creatures::ruleset();
        let StrikeSource::NaturalMelee { additional_riders } = &rules
            .document()
            .abilities
            .iter()
            .find(|ability| ability.id.as_str() == "filch_gold")
            .unwrap()
            .strikes
        else {
            unreachable!()
        };
        let original = StrikeDef {
            cancellable: false,
            offset: 0,
            unlock: None,
            check: None,
            on_success: vec![],
            on_failure: vec![],
            descriptors: vec![],
            riders: additional_riders.clone(),
        };
        validate_strike_shapes(std::slice::from_ref(&original)).unwrap();
        for corruption in 0..4 {
            let mut strike = original.clone();
            match corruption {
                0 => strike.on_success = std::mem::take(&mut strike.riders[0].effects),
                1 => strike.riders[0].check = None,
                2 => strike.riders[0].trigger = RiderTrigger::Failure,
                3 => {
                    let effect = strike.riders[0].effects[0].clone();
                    strike.riders[0].effects.push(effect);
                }
                _ => unreachable!(),
            }
            assert!(validate_strike_shapes(std::slice::from_ref(&strike)).is_err());
            let encoded = serde_json::to_value(StrikeSource::Authored {
                strikes: vec![strike],
            })
            .unwrap();
            assert!(serde_json::from_value::<StrikeSource>(encoded).is_err());
        }
    }

    #[test]
    fn theft_operands_decode_only_explicit_current_shapes() {
        let theft = PlannedTheft {
            selection: PlannedTheftSelection::MagicalUnit,
            depth: None,
            quantity_scope: None,
            relocation: None,
        };
        let value = serde_json::to_value(&theft).unwrap();
        assert_eq!(
            serde_json::from_value::<PlannedTheft>(value.clone()).unwrap(),
            theft
        );
        for field in ["depth", "quantity_scope", "relocation"] {
            let mut missing = value.clone();
            missing.as_object_mut().unwrap().remove(field);
            assert!(serde_json::from_value::<PlannedTheft>(missing).is_err());
        }
        let mut surplus = value;
        surplus["selection"]["private_inventory"] = serde_json::json!([]);
        assert!(serde_json::from_value::<PlannedTheft>(surplus).is_err());
        let mut mismatch = theft;
        mismatch.depth = Some(7);
        mismatch.quantity_scope = Some("floor/7".into());
        mismatch.relocation = Some(PlannedRelocation {
            policy: RelocationPolicy::SameFloor,
            scope: "floor/8".into(),
        });
        assert!(!mismatch.valid());
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TheftSelection {
    Currency {
        currency: CurrencyKind,
        amount: QuantityExpr,
    },
    MagicalUnit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelocationPolicy {
    SameFloor,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RelocationGate {
    pub flag: FlagId,
    pub policy: RelocationPolicy,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PlannedTheftSelection {
    Currency {
        currency: CurrencyKind,
        amount: QuantityRange,
    },
    MagicalUnit,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PlannedRelocation {
    pub policy: RelocationPolicy,
    pub scope: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PlannedTheft {
    pub selection: PlannedTheftSelection,
    /// Exact depth context, retained to validate contextual frozen amounts.
    #[serde(deserialize_with = "crate::bounded::required_option")]
    pub depth: Option<u32>,
    #[serde(deserialize_with = "crate::bounded::required_option")]
    pub quantity_scope: Option<String>,
    #[serde(deserialize_with = "crate::bounded::required_option")]
    pub relocation: Option<PlannedRelocation>,
}

impl PlannedTheft {
    pub(crate) fn valid(&self) -> bool {
        self.depth != Some(0)
            && self.depth.is_some() == self.quantity_scope.is_some()
            && self.quantity_scope.as_ref().is_none_or(|scope| {
                !scope.is_empty() && scope.len() <= 128 && !scope.chars().any(char::is_control)
            })
            && match &self.selection {
                PlannedTheftSelection::Currency { amount, .. } => amount.valid(),
                PlannedTheftSelection::MagicalUnit => self.depth.is_none(),
            }
            && self.relocation.as_ref().is_none_or(|relocation| {
                !relocation.scope.is_empty()
                    && relocation.scope.len() <= 128
                    && !relocation.scope.chars().any(char::is_control)
                    && self
                        .quantity_scope
                        .as_ref()
                        .is_none_or(|scope| scope == &relocation.scope)
            })
    }

    pub(crate) fn matches(
        &self,
        selection: &TheftSelection,
        relocation: Option<&RelocationGate>,
    ) -> bool {
        let selection_matches = match (selection, &self.selection) {
            (TheftSelection::MagicalUnit, PlannedTheftSelection::MagicalUnit) => {
                self.depth.is_none()
            }
            (
                TheftSelection::Currency { currency, amount },
                PlannedTheftSelection::Currency {
                    currency: actual,
                    amount: range,
                },
            ) => {
                currency == actual
                    && self.depth.is_some()
                        == matches!(amount, QuantityExpr::DepthScaledUniform { .. })
                    && amount
                        .resolve(self.depth)
                        .is_ok_and(|expected| expected == *range)
            }
            _ => false,
        };
        self.valid()
            && selection_matches
            && self
                .relocation
                .as_ref()
                .is_none_or(|actual| relocation.is_some_and(|gate| gate.policy == actual.policy))
    }
}

/// Only a positive, already committed ownership change creates this receipt.
/// It cannot be constructed from a replay payload or a successful check alone.
pub(crate) struct TransferReceipt {
    source: ActorId,
    target: ActorId,
    quantity: u64,
}
impl TransferReceipt {
    pub(crate) fn matches(&self, source: ActorId, target: ActorId) -> bool {
        self.source == source && self.target == target
    }
    pub(crate) fn quantity(&self) -> u64 {
        self.quantity
    }
}

impl Game {
    pub(crate) fn theft_contexts_valid(&self, ability: &crate::abilities::AbilityPlan) -> bool {
        // One compilation observes one source floor. Never let forged riders
        // expand a paid action into a union of otherwise valid historical floors.
        let mut captured_scope: Option<&str> = None;
        ability
            .strikes
            .iter()
            .flat_map(|strike| &strike.riders)
            .flat_map(|rider| &rider.effects)
            .all(|effect| {
                let crate::abilities::PlannedEffect::Theft { theft } = effect else {
                    return true;
                };
                if !theft.valid() {
                    return false;
                }
                for id in theft.quantity_scope.as_deref().into_iter().chain(
                    theft
                        .relocation
                        .as_ref()
                        .map(|relocation| relocation.scope.as_str()),
                ) {
                    if captured_scope.is_some_and(|captured| captured != id)
                        || self.activation_scope(id).is_none()
                    {
                        return false;
                    }
                    captured_scope = Some(id);
                }
                theft.quantity_scope.as_deref().is_none_or(|id| {
                    self.activation_scope(id)
                        .is_some_and(|scope| Some(scope.depth) == theft.depth)
                })
            })
    }

    fn preparation_required_regions(
        &self,
        plan: &crate::abilities::Plan,
    ) -> Result<BTreeSet<RegionId>, GameError> {
        let mut members = BTreeSet::new();
        if let Some(ray) = plan
            .as_ray()
            .filter(|ray| ray.traversal == crate::abilities::RayTraversal::FirstBody)
            .filter(|_| !matches!(plan, crate::abilities::Plan::Zap { charges: 0, .. }))
        {
            let trace = self
                .trace_first_body(
                    ray.actor,
                    ray.origin.location,
                    tor_world::Direction::from(ray.direction).rotated(ray.origin.orientation),
                    &ray.launch_footprint,
                    crate::abilities::MAX_CONTACT_STATES,
                )
                .map_err(|_| GameError::Rule(crate::abilities::RuleError::InvalidPlan))?;
            members.extend(trace.regions);
        }
        if plan
            .on_success()
            .iter()
            .any(|effect| crate::detection_lifecycle::planned_kind(effect).is_some())
            && !matches!(plan, crate::abilities::Plan::Zap { charges: 0, .. })
        {
            if plan
                .on_success()
                .iter()
                .any(|effect| matches!(effect, crate::abilities::PlannedEffect::MapTopology))
            {
                members.extend(self.mapping_required_regions(plan.actor())?);
            } else {
                members.extend(self.detection_required_regions(plan.actor())?);
            }
        }
        let room_light = plan
            .on_success()
            .iter()
            .any(|effect| matches!(effect, crate::abilities::PlannedEffect::RoomLight))
            || plan.as_ability().is_some_and(|ability| {
                ability.strikes.iter().any(|strike| {
                    strike
                        .on_success
                        .iter()
                        .chain(&strike.on_failure)
                        .chain(strike.riders.iter().flat_map(|rider| &rider.effects))
                        .any(|effect| matches!(effect, crate::abilities::PlannedEffect::RoomLight))
                })
            });
        if room_light && !matches!(plan, crate::abilities::Plan::Zap { charges: 0, .. }) {
            let actor = self
                .actors
                .get(&plan.actor())
                .ok_or(GameError::UnknownActor)?;
            members.extend(self.room_light_regions(actor.location)?);
        }
        let drain =
            plan.on_success()
                .iter()
                .any(|effect| matches!(effect, crate::abilities::PlannedEffect::DrainLife))
                || plan.as_ability().is_some_and(|ability| {
                    ability.strikes.iter().any(|strike| {
                        strike.on_success.iter().any(|effect| {
                            matches!(effect, crate::abilities::PlannedEffect::DrainLife)
                        })
                    })
                });
        if drain
            && !matches!(plan, crate::abilities::Plan::Zap { charges: 0, .. })
            && self
                .health(plan.actor())
                .is_some_and(|(health, _)| health >= 2)
        {
            let actor = self
                .actors
                .get(&plan.actor())
                .ok_or(GameError::UnknownActor)?;
            members.extend(self.drain_life_regions(actor.location)?);
        }
        if let Some(ability) = plan.as_ability() {
            for theft in ability
                .strikes
                .iter()
                .flat_map(|strike| &strike.riders)
                .flat_map(|rider| &rider.effects)
                .filter_map(|effect| match effect {
                    crate::abilities::PlannedEffect::Theft { theft } => Some(theft),
                    _ => None,
                })
            {
                if let Some(relocation) = &theft.relocation {
                    let scope = self
                        .activation_scope(&relocation.scope)
                        .ok_or(GameError::InvalidLocation)?;
                    members.extend(scope.members.iter().copied());
                }
            }
        }
        Ok(members)
    }

    pub fn preparation_region_boundary(&self) -> Option<&PreparationRegionBoundary> {
        self.preparation_region_boundary.as_ref()
    }

    pub fn preparation_missing_regions(
        &self,
        boundary: &PreparationRegionBoundary,
    ) -> BTreeSet<RegionId> {
        let loaded: BTreeSet<_> = self.world.loaded_regions().collect();
        boundary.members.difference(&loaded).copied().collect()
    }

    pub(crate) fn ensure_preparation_residency(&mut self, actor: ActorId) -> Result<(), GameError> {
        let Some(preparation) = self
            .preparation(actor)
            .filter(|preparation| preparation.active)
        else {
            return Ok(());
        };
        let boundary = PreparationRegionBoundary {
            actor,
            origin: preparation.origin,
            intention: preparation.intention,
            started: preparation.started,
            members: self.preparation_required_regions(&preparation.plan)?,
        };
        if self.preparation_missing_regions(&boundary).is_empty() {
            return Ok(());
        }
        if !self.host_region_continuation {
            return Err(GameError::InvalidLocation);
        }
        if self
            .preparation_region_boundary
            .as_ref()
            .is_some_and(|pending| pending != &boundary)
        {
            return Err(GameError::Rule(crate::abilities::RuleError::InvalidPlan));
        }
        self.preparation_region_boundary = Some(boundary);
        Ok(())
    }

    /// Roots are snapshotted from the complete ordinary ACTIVE closure before
    /// any supplemental floor is added. Loaded frozen actors never root floors.
    /// Room illumination dependencies load geometry without activating residents.
    pub(crate) fn relocation_loaded_closure(
        &self,
        active: &BTreeSet<RegionId>,
        loaded: &mut BTreeSet<RegionId>,
    ) {
        for (id, actor) in self
            .actors
            .iter()
            .filter(|(_, actor)| active.contains(&actor.location.region))
        {
            // Supplemental scope members remain load-only. Never add them to
            // live_regions, whose active owner would activate their residents.
            if self.monster_detection_active(*id) {
                if let Ok(members) = self.detection_required_regions(*id) {
                    loaded.extend(members);
                }
            }
            if let Some(preparation) = actor
                .pending
                .as_ref()
                .filter(|preparation| preparation.active)
            {
                if let Ok(members) = self.preparation_required_regions(&preparation.plan) {
                    loaded.extend(members);
                }
            }
        }
        if let Some(boundary) = &self.preparation_region_boundary {
            loaded.extend(boundary.members.iter().copied());
        }
    }

    pub(crate) fn compile_theft(
        &self,
        actor: ActorId,
        selection: &TheftSelection,
        gate: Option<&RelocationGate>,
    ) -> Result<PlannedTheft, GameError> {
        let source = self.actors.get(&actor).ok_or(GameError::UnknownActor)?;
        let scope = self.activation_scope_at(source.location.region);
        let contextual = matches!(
            selection,
            TheftSelection::Currency {
                amount: QuantityExpr::DepthScaledUniform { .. },
                ..
            }
        );
        let depth = contextual.then(|| scope.map(|scope| scope.depth)).flatten();
        let quantity_scope = contextual
            .then(|| scope.map(|scope| scope.scope.clone()))
            .flatten();
        let selection = match selection {
            TheftSelection::Currency { currency, amount } => PlannedTheftSelection::Currency {
                currency: *currency,
                amount: amount.resolve(depth)?,
            },
            TheftSelection::MagicalUnit => PlannedTheftSelection::MagicalUnit,
        };
        let relocation = gate
            .filter(|gate| {
                self.creature(actor)
                    .is_some_and(|creature| creature.derived().flags.contains(&gate.flag))
            })
            .map(|gate| {
                scope
                    .map(|scope| PlannedRelocation {
                        policy: gate.policy,
                        scope: scope.scope.clone(),
                    })
                    .ok_or(GameError::Rule(
                        crate::abilities::RuleError::InvalidDefinition,
                    ))
            })
            .transpose()?;
        Ok(PlannedTheft {
            selection,
            depth,
            quantity_scope,
            relocation,
        })
    }

    fn theft_eligible(&self, owner: ActorId, id: ItemId) -> bool {
        self.items.get(&id).is_some_and(|item| {
            item.location == ItemLocation::Carried(owner)
                && item.free_quantity(None).is_some_and(|free| free > 0)
        }) && self
            .actors
            .get(&owner)
            .is_some_and(|actor| !actor.equipment.values().any(|equipped| *equipped == id))
    }

    pub(crate) fn transfer_theft(
        &mut self,
        source: ActorId,
        target: ActorId,
        selection: &PlannedTheftSelection,
        rng: &mut u64,
    ) -> Result<Option<TransferReceipt>, GameError> {
        if source == target
            || !self.actors.get(&source).is_some_and(|actor| actor.alive())
            || !self.actors.get(&target).is_some_and(|actor| actor.alive())
        {
            return Ok(None);
        }
        match selection {
            PlannedTheftSelection::Currency { currency, amount } => {
                let requested = amount.sample(rng)?;
                let mut remaining = requested;
                // Lazy, candidate-local homogeneous-stack merge index. A no-op theft
                // never scans the recipient, and multiple fragments do so once.
                let mut destinations: Option<BTreeMap<_, _>> = None;
                // The indexed owner bucket is deterministic. Never sum a purse:
                // subtraction caps each fragment without overflowing u64.
                let mut previous = None;
                while remaining > 0 {
                    let Some(id) = self
                        .items
                        .next_at_after(ItemLocation::Carried(target), previous)
                    else {
                        break;
                    };
                    previous = Some(id);
                    if !self.theft_eligible(target, id)
                        || self.items[&id].spec.currency != Some(*currency)
                    {
                        continue;
                    }
                    let quantity = self.items[&id]
                        .free_quantity(None)
                        .ok_or(GameError::ItemUnavailable)?
                        .min(remaining);
                    let key = self.items[&id]
                        .stack_key()
                        .ok_or(GameError::ItemUnavailable)?;
                    let destinations = destinations.get_or_insert_with(|| {
                        let mut index = BTreeMap::new();
                        // Indexed IDs are ordered. Keep the lowest-ID exact
                        // match and its existing overflow behavior; Shared keys
                        // retain definitions without copying them.
                        for id in self.items.at(ItemLocation::Carried(source)) {
                            crate::diagnostics::stack_candidate();
                            let item = &self.items[&id];
                            if item.spec.currency == Some(*currency) {
                                if let Some(key) = item.stack_key() {
                                    index.entry(key).or_insert(id);
                                }
                            }
                        }
                        index
                    });
                    crate::diagnostics::stack_candidate();
                    let merge = destinations.get(&key).copied();
                    let transfer =
                        self.prepare_owned_transfer_into(id, target, source, quantity, merge)?;
                    let result = transfer.result();
                    self.apply_owned_transfer(transfer)?;
                    destinations.insert(key, result);
                    remaining -= quantity;
                }
                Ok((remaining < requested).then_some(TransferReceipt {
                    source,
                    target,
                    quantity: requested - remaining,
                }))
            }
            PlannedTheftSelection::MagicalUnit => {
                let eligible =
                    |id: &ItemId| self.theft_eligible(target, *id) && self.items[id].magical();
                let count = self
                    .items
                    .at(ItemLocation::Carried(target))
                    .filter(eligible)
                    .count();
                if count == 0 {
                    return Ok(None);
                }
                let selected = QuantityRange::new(1, count as u64)?.sample(rng)? - 1;
                let id = self
                    .items
                    .at(ItemLocation::Carried(target))
                    .filter(eligible)
                    .nth(selected as usize)
                    .ok_or(GameError::ItemUnavailable)?;
                let transfer = self.prepare_owned_transfer(id, target, source, 1)?;
                self.apply_owned_transfer(transfer)?;
                Ok(Some(TransferReceipt {
                    source,
                    target,
                    quantity: 1,
                }))
            }
        }
    }

    pub(crate) fn relocate_after_theft(
        &mut self,
        actor: ActorId,
        relocation: &PlannedRelocation,
        receipt: &TransferReceipt,
        rng: &mut u64,
    ) -> Result<Option<crate::abilities::Placement>, GameError> {
        if receipt.quantity == 0 || receipt.source != actor {
            return Err(GameError::Rule(crate::abilities::RuleError::InvalidPlan));
        }
        let scope = self
            .activation_scope(&relocation.scope)
            .ok_or(GameError::InvalidLocation)?;
        let loaded: BTreeSet<_> = self.world.loaded_regions().collect();
        if !scope.members.is_subset(&loaded) {
            return Err(GameError::InvalidLocation);
        }
        let source = self.actors.get(&actor).ok_or(GameError::UnknownActor)?;
        if !scope.members.contains(&source.location.region) {
            return Ok(None);
        }
        let legal = |at: &&tor_world::Location| {
            **at != source.location
                && self
                    .body_cells(**at, source.orientation, &source.body)
                    .is_some_and(|cells| {
                        cells
                            .iter()
                            .all(|(at, _)| scope.members.contains(&at.region))
                    })
                && self.stable_landing(actor, **at, source.orientation, &source.body)
        };
        let count = scope.cells.iter().filter(legal).count();
        if count == 0 {
            return Ok(None);
        }
        let index = QuantityRange::new(1, count as u64)?.sample(rng)? - 1;
        let location = *scope
            .cells
            .iter()
            .filter(legal)
            .nth(index as usize)
            .ok_or(GameError::InvalidLocation)?;
        let placement = crate::abilities::Placement {
            location,
            orientation: source.orientation,
        };
        let mut outcome = self.settled_actor_outcome(actor)?;
        outcome.state.location = location;
        outcome.state.motion = crate::MotionState::default();
        outcome.state.visited.insert(location.region);
        outcome.entry_cause = crate::floor_activation::EntryCause::Teleport;
        // Ordinary publication reconciles holds and contextual recovery without
        // discarding the already-paid sequence or its recovery cursor.
        // Destination contacts can draw from the same stream. Publish the
        // completed selection before contact, then return its evolved suffix.
        self.rules_rng = *rng;
        self.apply_outcome(outcome)?;
        self.sync_actor_lifecycle(actor)?;
        *rng = self.rules_rng;
        Ok(Some(placement))
    }
}

// Saved rule operands require named current records. Unit selector variants also
// reject surplus fields instead of treating them as an ignored private payload.
impl<'de> Deserialize<'de> for TheftSelection {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            Currency {
                currency: CurrencyKind,
                amount: QuantityExpr,
            },
            MagicalUnit {},
        }
        Ok(match crate::bounded::named_map(d)? {
            Fields::Currency { currency, amount } => Self::Currency { currency, amount },
            Fields::MagicalUnit {} => Self::MagicalUnit,
        })
    }
}
impl<'de> Deserialize<'de> for PlannedTheftSelection {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            Currency {
                currency: CurrencyKind,
                amount: QuantityRange,
            },
            MagicalUnit {},
        }
        Ok(match crate::bounded::named_map(d)? {
            Fields::Currency { currency, amount } => Self::Currency { currency, amount },
            Fields::MagicalUnit {} => Self::MagicalUnit,
        })
    }
}
impl<'de> Deserialize<'de> for RelocationGate {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            flag: FlagId,
            policy: RelocationPolicy,
        }
        let Fields { flag, policy } = crate::bounded::named_map(d)?;
        Ok(Self { flag, policy })
    }
}
impl<'de> Deserialize<'de> for PlannedRelocation {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            policy: RelocationPolicy,
            scope: String,
        }
        let Fields { policy, scope } = crate::bounded::named_map(d)?;
        if scope.is_empty() || scope.len() > 128 || scope.chars().any(char::is_control) {
            return Err(serde::de::Error::custom("invalid relocation scope"));
        }
        Ok(Self { policy, scope })
    }
}
impl<'de> Deserialize<'de> for PlannedTheft {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            selection: PlannedTheftSelection,
            #[serde(deserialize_with = "crate::bounded::required_option")]
            depth: Option<u32>,
            #[serde(deserialize_with = "crate::bounded::required_option")]
            quantity_scope: Option<String>,
            #[serde(deserialize_with = "crate::bounded::required_option")]
            relocation: Option<PlannedRelocation>,
        }
        let Fields {
            selection,
            depth,
            quantity_scope,
            relocation,
        } = crate::bounded::named_map(d)?;
        let value = Self {
            selection,
            depth,
            quantity_scope,
            relocation,
        };
        if !value.valid() {
            return Err(serde::de::Error::custom("invalid planned theft"));
        }
        Ok(value)
    }
}
