//! Checked inventory riders. Authored capabilities become immutable numerical
//! operands; inventory eligibility belongs to the successful live transaction.
use crate::{content_ids::FlagId, quantity::{QuantityExpr, QuantityRange}, ActorId, CurrencyKind, Game, GameError, ItemId, ItemLocation};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
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
mod tests {
    use super::*;
    use std::num::NonZeroU64;
    use tor_world::{Location, Position};

    fn at(x: i32) -> Location { Location { region: RegionId(1), position: Position { x, y: 1, z: 0 } } }
    fn game() -> Game {
        let mut game = Game::region_corridor(42, 2, crate::test_creatures::content());
        game.spawn_authored_actor(1, at(1), NonZeroU64::new(100).unwrap()).unwrap();
        game.spawn_authored_actor(2, at(2), NonZeroU64::new(100).unwrap()).unwrap();
        game
    }
    fn gold(name: &str) -> crate::ItemSpec {
        let mut item = crate::ItemSpec::ordinary(name.into());
        item.class = crate::ItemClass::Coin;
        item.currency = Some(CurrencyKind::Gold);
        item.stackable = true;
        item
    }
    fn selection(amount: u64) -> PlannedTheftSelection { PlannedTheftSelection::Currency { currency: CurrencyKind::Gold, amount: QuantityRange::new(amount, amount).unwrap() } }

    #[test]
    fn gold_caps_total_purse_across_specs_and_preserves_finite_stock() {
        let mut game = game();
        game.place_item_stack(10, at(2), Some(ActorId(2)), 3, gold("old gold")).unwrap();
        game.place_item_stack(11, at(2), Some(ActorId(2)), 5, gold("new gold")).unwrap();
        game.place_item_stack(12, at(1), Some(ActorId(1)), 2, gold("old gold")).unwrap();
        let mut rng = 9;
        let receipt = game.transfer_theft(ActorId(1), ActorId(2), &selection(7), &mut rng).unwrap().unwrap();
        assert_eq!(receipt.quantity(), 7);
        assert_eq!(rng, 9);
        assert_eq!(game.items.values().map(|item| item.quantity).sum::<u64>(), 10);
        assert_eq!(game.items[&ItemId(12)].quantity, 5);
        assert_eq!(game.items[&ItemId(11)].quantity, 1);
        let receipt = game.transfer_theft(ActorId(1), ActorId(2), &selection(u64::MAX), &mut rng).unwrap().unwrap();
        assert_eq!(receipt.quantity(), 1);
        assert!(game.transfer_theft(ActorId(1), ActorId(2), &selection(1), &mut rng).unwrap().is_none());
    }

    #[test]
    fn magical_selection_moves_one_unknown_unit_without_learning_identity() {
        let mut game = game();
        let mut spec = crate::ItemSpec::ordinary("hidden charm".into());
        spec.magical = true;
        spec.concealed = true;
        spec.appearance = "unmarked charm".into();
        spec.stackable = true;
        game.place_item_stack(10, at(2), Some(ActorId(2)), 3, spec).unwrap();
        game.place_item_stack(11, at(2), Some(ActorId(2)), 1, crate::ItemSpec::ordinary("ordinary armor".into())).unwrap();
        let knowledge = game.actors[&ActorId(1)].knowledge.clone();
        let mut rng = 9;
        assert_eq!(game.transfer_theft(ActorId(1), ActorId(2), &PlannedTheftSelection::MagicalUnit, &mut rng).unwrap().unwrap().quantity(), 1);
        assert_eq!(game.items[&ItemId(10)].quantity, 2);
        assert_eq!(game.actors[&ActorId(1)].knowledge, knowledge);
        assert_eq!(rng, 9, "one eligible entry requires no random draw");
        let stolen = game.items.at(ItemLocation::Carried(ActorId(1))).next().unwrap();
        assert!(game.items[&stolen].spec.magical);
        assert!(game.items[&stolen].spec.concealed);
    }

    #[test]
    fn self_transfer_and_stale_destination_cannot_create_a_receipt() {
        let mut game = game();
        game.place_item_stack(10, at(2), Some(ActorId(2)), 3, gold("gold")).unwrap();
        game.place_item_stack(11, at(1), Some(ActorId(1)), 3, gold("gold")).unwrap();
        let before = game.clone();
        assert!(game.transfer_theft(ActorId(2), ActorId(2), &selection(3), &mut 9).unwrap().is_none());
        assert_eq!(game, before);
        let plan = game.prepare_owned_transfer(ItemId(10), ActorId(2), ActorId(1), 2).unwrap();
        game.items.edit(ItemId(11), |item| item.location = ItemLocation::Ground(at(1))).unwrap();
        let before = game.clone();
        assert_eq!(game.apply_owned_transfer(plan), Err(GameError::ItemUnavailable));
        assert_eq!(game, before);
    }

    #[test]
    fn later_fragment_overflow_rolls_back_the_enclosing_candidate() {
        let mut game = game();
        game.place_item_stack(10, at(2), Some(ActorId(2)), 2, gold("first")).unwrap();
        game.place_item_stack(11, at(2), Some(ActorId(2)), 2, gold("second")).unwrap();
        game.place_item_stack(12, at(1), Some(ActorId(1)), u64::MAX, gold("second")).unwrap();
        let before = game.clone();
        let mut candidate = game.clone();
        assert!(candidate.transfer_theft(ActorId(1), ActorId(2), &selection(4), &mut 9).is_err());
        assert_eq!(game, before);
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TheftSelection {
    Currency { currency: CurrencyKind, amount: QuantityExpr },
    MagicalUnit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelocationPolicy { SameFloor }

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RelocationGate { pub flag: FlagId, pub policy: RelocationPolicy }

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PlannedTheftSelection {
    Currency { currency: CurrencyKind, amount: QuantityRange },
    MagicalUnit,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlannedRelocation { pub policy: RelocationPolicy, pub scope: String }

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlannedTheft {
    pub selection: PlannedTheftSelection,
    /// Exact depth context, retained to validate contextual frozen amounts.
    #[serde(deserialize_with = "crate::bounded::required_option")]
    pub depth: Option<u32>,
    #[serde(deserialize_with = "crate::bounded::required_option")]
    pub relocation: Option<PlannedRelocation>,
}

impl PlannedTheft {
    pub(crate) fn valid(&self) -> bool {
        self.depth != Some(0)
            && match &self.selection { PlannedTheftSelection::Currency { amount, .. } => amount.valid(), PlannedTheftSelection::MagicalUnit => true }
            && self.relocation.as_ref().is_none_or(|relocation| !relocation.scope.is_empty() && relocation.scope.len() <= 128)
    }

    pub(crate) fn matches(&self, selection: &TheftSelection, relocation: Option<&RelocationGate>) -> bool {
        let selection_matches = match (selection, &self.selection) {
            (TheftSelection::MagicalUnit, PlannedTheftSelection::MagicalUnit) => true,
            (TheftSelection::Currency { currency, amount }, PlannedTheftSelection::Currency { currency: actual, amount: range }) => currency == actual && amount.resolve(self.depth).is_ok_and(|expected| expected == *range),
            _ => false,
        };
        self.valid() && selection_matches && self.relocation.as_ref().is_none_or(|actual| relocation.is_some_and(|gate| gate.policy == actual.policy))
    }
}

/// Only a positive, already committed ownership change creates this receipt.
/// It cannot be constructed from a replay payload or a successful check alone.
pub(crate) struct TransferReceipt { quantity: u64 }
impl TransferReceipt { pub(crate) fn quantity(&self) -> u64 { self.quantity } }

impl Game {
    fn relocation_members(&self, plan: &crate::abilities::Plan) -> Result<BTreeSet<RegionId>, GameError> {
        let mut members = BTreeSet::new();
        if let Some(ability) = plan.as_ability() {
            for theft in ability.strikes.iter().flat_map(|strike| &strike.riders).flat_map(|rider| &rider.effects).filter_map(|effect| match effect { crate::abilities::PlannedEffect::Theft { theft } => Some(theft), _ => None }) {
                if let Some(relocation) = &theft.relocation {
                    let scope = self.activation_scopes().find(|scope| scope.scope == relocation.scope).ok_or(GameError::InvalidLocation)?;
                    members.extend(scope.members.iter().copied());
                }
            }
        }
        Ok(members)
    }

    pub fn preparation_region_boundary(&self) -> Option<&PreparationRegionBoundary> { self.preparation_region_boundary.as_ref() }

    pub fn preparation_missing_regions(&self, boundary: &PreparationRegionBoundary) -> BTreeSet<RegionId> {
        let loaded: BTreeSet<_> = self.world.loaded_regions().collect();
        boundary.members.difference(&loaded).copied().collect()
    }

    pub(crate) fn ensure_preparation_residency(&mut self, actor: ActorId) -> Result<(), GameError> {
        let Some(preparation) = self.preparation(actor).filter(|preparation| preparation.active) else { return Ok(()); };
        let boundary = PreparationRegionBoundary { actor, origin: preparation.origin, intention: preparation.intention, started: preparation.started, members: self.relocation_members(&preparation.plan)? };
        if self.preparation_missing_regions(&boundary).is_empty() { return Ok(()); }
        if !self.host_region_continuation { return Err(GameError::InvalidLocation); }
        if self.preparation_region_boundary.as_ref().is_some_and(|pending| pending != &boundary) { return Err(GameError::Rule(crate::abilities::RuleError::InvalidPlan)); }
        self.preparation_region_boundary = Some(boundary);
        Ok(())
    }

    /// Roots are snapshotted from the complete ordinary ACTIVE closure before
    /// any supplemental floor is added. Loaded frozen thieves never root floors.
    pub(crate) fn relocation_loaded_closure(&self, active: &BTreeSet<RegionId>, loaded: &mut BTreeSet<RegionId>) {
        for actor in self.actors.values().filter(|actor| active.contains(&actor.location.region)) {
            if let Some(preparation) = actor.pending.as_ref().filter(|preparation| preparation.active) {
                if let Ok(members) = self.relocation_members(&preparation.plan) { loaded.extend(members); }
            }
        }
        if let Some(boundary) = &self.preparation_region_boundary { loaded.extend(boundary.members.iter().copied()); }
    }

    pub(crate) fn compile_theft(&self, actor: ActorId, selection: &TheftSelection, gate: Option<&RelocationGate>) -> Result<PlannedTheft, GameError> {
        let source = self.actors.get(&actor).ok_or(GameError::UnknownActor)?;
        let scope = self.activation_scope_at(source.location.region);
        let depth = scope.map(|scope| scope.depth);
        let selection = match selection {
            TheftSelection::Currency { currency, amount } => PlannedTheftSelection::Currency { currency: *currency, amount: amount.resolve(depth)? },
            TheftSelection::MagicalUnit => PlannedTheftSelection::MagicalUnit,
        };
        let relocation = gate.filter(|gate| self.creature(actor).is_some_and(|creature| creature.derived().flags.contains(&gate.flag)))
            .map(|gate| scope.map(|scope| PlannedRelocation { policy: gate.policy, scope: scope.scope.clone() }).ok_or(GameError::Rule(crate::abilities::RuleError::InvalidDefinition))).transpose()?;
        Ok(PlannedTheft { selection, depth, relocation })
    }

    fn theft_eligible(&self, owner: ActorId, id: ItemId) -> bool {
        self.items.get(&id).is_some_and(|item| item.location == ItemLocation::Carried(owner) && item.free_quantity(None).is_some_and(|free| free > 0))
            && self.actors.get(&owner).is_some_and(|actor| !actor.equipment.values().any(|equipped| *equipped == id))
    }

    pub(crate) fn transfer_theft(&mut self, source: ActorId, target: ActorId, selection: &PlannedTheftSelection, rng: &mut u64) -> Result<Option<TransferReceipt>, GameError> {
        if source == target || !self.actors.get(&source).is_some_and(|actor| actor.alive()) || !self.actors.get(&target).is_some_and(|actor| actor.alive()) { return Ok(None); }
        match selection {
            PlannedTheftSelection::Currency { currency, amount } => {
                let requested = amount.sample(rng)?;
                let mut remaining = requested;
                // The indexed owner bucket is deterministic. Never sum a purse:
                // subtraction caps each fragment without overflowing u64.
                let mut previous = None;
                while remaining > 0 {
                    let Some(id) = self.items.next_at_after(ItemLocation::Carried(target), previous) else { break; };
                    previous = Some(id);
                    if !self.theft_eligible(target, id) || self.items[&id].spec.currency != Some(*currency) { continue; }
                    let quantity = self.items[&id].free_quantity(None).ok_or(GameError::ItemUnavailable)?.min(remaining);
                    let transfer = self.prepare_owned_transfer(id, target, source, quantity)?;
                    self.apply_owned_transfer(transfer)?;
                    remaining -= quantity;
                }
                Ok((remaining < requested).then_some(TransferReceipt { quantity: requested - remaining }))
            }
            PlannedTheftSelection::MagicalUnit => {
                let eligible = |id: &ItemId| self.theft_eligible(target, *id) && self.items[id].spec.magical;
                let count = self.items.at(ItemLocation::Carried(target)).filter(eligible).count();
                if count == 0 { return Ok(None); }
                let selected = QuantityRange::new(1, count as u64)?.sample(rng)? - 1;
                let id = self.items.at(ItemLocation::Carried(target)).filter(eligible).nth(selected as usize).ok_or(GameError::ItemUnavailable)?;
                let transfer = self.prepare_owned_transfer(id, target, source, 1)?;
                self.apply_owned_transfer(transfer)?;
                Ok(Some(TransferReceipt { quantity: 1 }))
            }
        }
    }

    pub(crate) fn relocate_after_theft(&mut self, actor: ActorId, relocation: &PlannedRelocation, receipt: &TransferReceipt, rng: &mut u64) -> Result<Option<crate::abilities::Placement>, GameError> {
        if receipt.quantity == 0 { return Err(GameError::Rule(crate::abilities::RuleError::InvalidPlan)); }
        let scope = self.activation_scopes().find(|scope| scope.scope == relocation.scope).ok_or(GameError::InvalidLocation)?;
        let loaded: BTreeSet<_> = self.world.loaded_regions().collect();
        if !scope.members.is_subset(&loaded) { return Err(GameError::InvalidLocation); }
        let source = self.actors.get(&actor).ok_or(GameError::UnknownActor)?;
        if !scope.members.contains(&source.location.region) { return Ok(None); }
        let legal = |at: &&tor_world::Location| **at != source.location
            && self.body_cells(**at, source.orientation, &source.body).is_some_and(|cells| cells.iter().all(|(at, _)| scope.members.contains(&at.region)))
            && self.stable_landing(actor, **at, source.orientation, &source.body);
        let count = scope.cells.iter().filter(legal).count();
        if count == 0 { return Ok(None); }
        let index = QuantityRange::new(1, count as u64)?.sample(rng)? - 1;
        let location = *scope.cells.iter().filter(legal).nth(index as usize).ok_or(GameError::InvalidLocation)?;
        let placement = crate::abilities::Placement { location, orientation: source.orientation };
        let mut outcome = self.settled_actor_outcome(actor)?;
        outcome.state.location = location;
        outcome.state.motion = crate::MotionState::default();
        outcome.state.visited.insert(location.region);
        outcome.entry_cause = crate::floor_activation::EntryCause::Teleport;
        // Ordinary publication reconciles holds and contextual recovery without
        // discarding the already-paid sequence or its recovery cursor.
        self.commit_actor_outcome(outcome)?;
        self.sync_actor_lifecycle(actor)?;
        Ok(Some(placement))
    }
}
