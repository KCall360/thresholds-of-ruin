//! Finite-item authority. Quantities live only on items; preparations keep references.
use crate::rules::{SupplyAccess, SupplySpendAt};
use crate::{
    abilities::Plan, combat::Preparation, ActorId, Game, GameError, IntentionId, Item, ItemId,
    ItemLocation,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const MAX_ITEM_FRAGMENTS: usize = 32;
pub const MAX_ITEM_HOLDS: usize = 64;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SupplySelection {
    #[default]
    Carried,
    Explicit {
        items: Vec<SelectedSupply>,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SelectedSupply {
    pub cost_ordinal: u8,
    pub item: ItemId,
}
fn read_selection<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<SelectedSupply>, D::Error> {
    Ok(crate::bounded::Bounded::<SelectedSupply, MAX_ITEM_FRAGMENTS>::deserialize(d)?.0)
}
impl<'de> Deserialize<'de> for SupplySelection {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            Carried {},
            Explicit {
                #[serde(deserialize_with = "read_selection")]
                items: Vec<SelectedSupply>,
            },
        }
        Ok(match crate::bounded::named_map(deserializer)? {
            Fields::Carried {} => Self::Carried,
            Fields::Explicit { items } => Self::Explicit { items },
        })
    }
}
impl SupplySelection {
    pub(crate) fn valid(&self, cost_count: usize) -> bool {
        match self {
            Self::Carried => true,
            Self::Explicit { items } => {
                !items.is_empty() && items.len() <= MAX_ITEM_FRAGMENTS
                && items.windows(2).all(|pair| pair[0] < pair[1])
                // Current costs have one spend stage and distinct capabilities;
                // each physical item has only one capability. This also checks
                // historical paid tails without reopening deleted item sources.
                && items.iter().map(|row| row.item).collect::<std::collections::BTreeSet<_>>().len() == items.len()
                && items.iter().all(|row| usize::from(row.cost_ordinal) < cost_count
                    && row.item.0 != 0 && row.item.0 != u64::MAX)
                && (0..cost_count).all(|ordinal| items.iter().any(|row| usize::from(row.cost_ordinal) == ordinal))
            }
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PreparationLineage {
    pub actor: ActorId,
    pub origin: IntentionId,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ItemClaimKey {
    pub lineage: PreparationLineage,
    pub cost_ordinal: u8,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ItemHold {
    pub key: ItemClaimKey,
    pub quantity: u64,
}
// Claim records have one current named shape, never positional tuple aliases.
macro_rules! named_record {
    ($name:ident { $($field:ident: $ty:ty),+ $(,)? }) => {
        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self,D::Error> {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Fields { $($field: $ty),+ }
                let fields: Fields = crate::bounded::named_map(deserializer)?;
                Ok(Self { $($field: fields.$field),+ })
            }
        }
    };
}
named_record!(SelectedSupply {
    cost_ordinal: u8,
    item: ItemId
});
named_record!(PreparationLineage {
    actor: ActorId,
    origin: IntentionId
});
named_record!(ItemClaimKey {
    lineage: PreparationLineage,
    cost_ordinal: u8
});
named_record!(ItemHold {
    key: ItemClaimKey,
    quantity: u64
});
named_record!(ItemClaimRef {
    cost_ordinal: u8,
    item: ItemId
});

pub(crate) fn read_holds<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Vec<ItemHold>, D::Error> {
    Ok(crate::bounded::Bounded::<ItemHold, MAX_ITEM_HOLDS>::deserialize(d)?.0)
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ItemClaimRef {
    pub cost_ordinal: u8,
    pub item: ItemId,
}
fn read_refs<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<ItemClaimRef>, D::Error> {
    Ok(crate::bounded::Bounded::<ItemClaimRef, MAX_ITEM_FRAGMENTS>::deserialize(d)?.0)
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ItemPayment {
    #[default]
    None,
    Held {
        refs: Vec<ItemClaimRef>,
    },
    Settled,
}
impl<'de> Deserialize<'de> for ItemPayment {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            None {},
            Held {
                #[serde(deserialize_with = "read_refs")]
                refs: Vec<ItemClaimRef>,
            },
            Settled {},
        }
        Ok(match crate::bounded::named_map(deserializer)? {
            Fields::None {} => Self::None,
            Fields::Held { refs } => Self::Held { refs },
            Fields::Settled {} => Self::Settled,
        })
    }
}
impl ItemPayment {
    pub(crate) fn refs(&self) -> &[ItemClaimRef] {
        match self {
            Self::Held { refs } => refs,
            _ => &[],
        }
    }
    pub(crate) fn shape_valid(&self, plan: &Plan, paid_prefix: bool) -> bool {
        match (plan.needs_item_payment(), paid_prefix, self) {
            (false, _, Self::None) => true,
            (true, true, Self::Settled) => true,
            (true, false, Self::Held { refs }) => {
                !refs.is_empty()
                    && refs.len() <= MAX_ITEM_FRAGMENTS
                    && refs.windows(2).all(|pair| pair[0] < pair[1])
                    && refs.iter().all(|row| {
                        row.item.0 != 0
                            && row.item.0 != u64::MAX
                            && usize::from(row.cost_ordinal) < plan.item_cost_count()
                    })
            }
            _ => false,
        }
    }
}
/// One exhaustive source of finite-payment operands. New plan kinds must choose
/// a contract here before allocation, reciprocal validation or settlement compile.
enum ItemContract<'a> {
    Supplies {
        costs: &'a [crate::rules::SupplyCost],
        selection: &'a SupplySelection,
        spend_at: SupplySpendAt,
    },
    ExactUnit {
        item: ItemId,
    },
    None,
}
impl Plan {
    fn item_contract(&self) -> ItemContract<'_> {
        match self {
            Self::Ability(plan) => ItemContract::Supplies {
                costs: &plan.supply_costs,
                selection: &plan.supplies,
                spend_at: SupplySpendAt::FirstResolution,
            },
            Self::Respec(plan) => ItemContract::Supplies {
                costs: std::slice::from_ref(&plan.ration),
                selection: &plan.supplies,
                spend_at: SupplySpendAt::ReplacementCommit,
            },
            Self::Drink { item, .. } | Self::Eat { item, .. } => {
                ItemContract::ExactUnit { item: *item }
            }
            Self::Equip { .. } | Self::Unequip { .. } => ItemContract::None,
        }
    }
    fn supply_selection(&self) -> Option<&SupplySelection> {
        match self.item_contract() {
            ItemContract::Supplies { selection, .. } => Some(selection),
            ItemContract::ExactUnit { .. } | ItemContract::None => None,
        }
    }
    pub(crate) fn needs_item_payment(&self) -> bool {
        self.item_cost_count() > 0
    }
    pub(crate) fn item_cost_count(&self) -> usize {
        match self.item_contract() {
            ItemContract::Supplies { costs, .. } => costs.len(),
            ItemContract::ExactUnit { .. } => 1,
            ItemContract::None => 0,
        }
    }
    fn item_cost_quantity(&self, ordinal: u8) -> Option<u64> {
        match self.item_contract() {
            ItemContract::Supplies { costs, .. } => {
                costs.get(usize::from(ordinal)).map(|cost| cost.quantity)
            }
            ItemContract::ExactUnit { .. } => (ordinal == 0).then_some(1),
            ItemContract::None => None,
        }
    }
}
impl Preparation {
    pub(crate) fn item_lineage(&self) -> Option<PreparationLineage> {
        Some(PreparationLineage {
            actor: self.plan.actor(),
            origin: self.origin?,
        })
    }
}
impl Item {
    pub(crate) fn free_quantity(&self, retiring: Option<PreparationLineage>) -> Option<u64> {
        let held = self
            .holds
            .iter()
            .filter(|hold| Some(hold.key.lineage) != retiring)
            .try_fold(0u64, |sum, hold| sum.checked_add(hold.quantity))?;
        self.quantity.checked_sub(held)
    }
    pub(crate) fn holds_valid(&self) -> bool {
        self.holds.len() <= MAX_ITEM_HOLDS
            && self.holds.windows(2).all(|pair| pair[0].key < pair[1].key)
            && self.holds.iter().all(|hold| {
                hold.quantity > 0
                    && hold.quantity <= crate::limits::NUMERIC as u64
                    && hold.key.lineage.actor.0 > 0
                    && hold.key.lineage.actor.0 != u64::MAX
                    && hold.key.lineage.origin.0 > 0
                    && hold.key.lineage.origin.0 != u64::MAX
                    && usize::from(hold.key.cost_ordinal) < crate::limits::SUPPLY_COSTS
            })
            && self.free_quantity(None).is_some()
    }
}

impl Game {
    /// Offline loaded-authority diagnostics, excluded from observation and action paths.
    pub fn profile_finite_claims(
        &self,
    ) -> Result<crate::diagnostics::FiniteClaimProfile, GameError> {
        let mut profile = crate::diagnostics::FiniteClaimProfile::default();
        let mut actors = std::collections::BTreeSet::new();
        for (_, item) in self.items.claimed() {
            profile.item_holds += item.holds.len();
            profile.max_holds_per_item = profile.max_holds_per_item.max(item.holds.len());
            for hold in &item.holds {
                actors.insert(hold.key.lineage.actor);
                profile.held_units = profile
                    .held_units
                    .checked_add(hold.quantity)
                    .ok_or(GameError::InvalidQuantity)?;
            }
        }
        profile.claimant_actors = actors.len();
        profile.preparation_refs = self
            .actors
            .values()
            .filter_map(|actor| actor.pending.as_ref())
            .map(|preparation| preparation.item_payment.refs().len())
            .sum();
        Ok(profile)
    }

    fn known_supply(&self, actor: ActorId, item: &Item) -> bool {
        !item.spec.concealed
            || self
                .actors
                .get(&actor)
                .is_some_and(|actor| actor.knowledge.contains(&item.spec.identity))
    }
    /// Structural physical reach also works for frozen endpoints. Execution checks residency separately.
    pub(crate) fn supply_access(&self, actor: ActorId, item: &Item, access: &SupplyAccess) -> bool {
        match item.location {
            ItemLocation::Carried(owner) => actor == owner,
            ItemLocation::Ground(at) if matches!(access, SupplyAccess::CarriedOrGroundTouch) => {
                // Reuse the ordinary disclosed scene under its existing exact
                // geometry/body witness, including the current AI decision.
                self.scene_contains(actor, at).unwrap_or(false) && self.touches_supply(actor, at)
            }
            ItemLocation::Ground(_) => false,
        }
    }
    /// Physical contact only. Callers separately require ordinary disclosure.
    pub(crate) fn touches_supply(&self, actor: ActorId, at: tor_world::Location) -> bool {
        let Some(owner) = self.actors.get(&actor) else {
            return false;
        };
        let Some(cells) = self.body_cells(owner.location, owner.orientation, &owner.body) else {
            return false;
        };
        cells.iter().any(|(from, _)| {
            *from == at
                || (-1..=1).any(|x| {
                    (-1..=1).any(|y| {
                        (-1..=1).any(|z| {
                            [x, y, z] != [0, 0, 0]
                                && self.melee_neighbor(*from, [x, y, z]) == Some(at)
                        })
                    })
                })
        })
    }
    fn plan_item_matches(&self, plan: &Plan, reference: ItemClaimRef, item: &Item) -> bool {
        match plan.item_contract() {
            ItemContract::ExactUnit { item: selected } => {
                reference.cost_ordinal == 0
                    && selected == reference.item
                    && item.location == ItemLocation::Carried(plan.actor())
                    && match plan {
                        Plan::Drink { .. } => item.spec.consumable.is_some(),
                        Plan::Eat { .. } => item.spec.edible.is_some(),
                        _ => false,
                    }
            }
            ItemContract::Supplies {
                costs,
                selection,
                spend_at,
            } => costs
                .get(usize::from(reference.cost_ordinal))
                .is_some_and(|cost| {
                    cost.spend_at == spend_at
                        && self.known_supply(plan.actor(), item)
                        && item.spec.supply.as_ref() == Some(&cost.supply)
                        && self.supply_access(plan.actor(), item, &cost.access)
                        && match selection {
                            SupplySelection::Carried => {
                                item.location == ItemLocation::Carried(plan.actor())
                            }
                            SupplySelection::Explicit { items } => items
                                .binary_search(&SelectedSupply {
                                    cost_ordinal: reference.cost_ordinal,
                                    item: reference.item,
                                })
                                .is_ok(),
                        }
                }),
            ItemContract::None => false,
        }
    }
    /// Pure preflight. A replacement may discount only its own exact old origin.
    pub(crate) fn item_allocation(
        &self,
        plan: &Plan,
    ) -> Result<Vec<(ItemClaimRef, u64)>, GameError> {
        self.allocate_items(plan, true)
    }
    fn physical_item_allocation(&self, plan: &Plan) -> Result<Vec<(ItemClaimRef, u64)>, GameError> {
        self.allocate_items(plan, false)
    }
    fn allocate_items(
        &self,
        plan: &Plan,
        committed: bool,
    ) -> Result<Vec<(ItemClaimRef, u64)>, GameError> {
        if !plan.needs_item_payment() {
            return Ok(vec![]);
        }
        let actor = plan.actor();
        let retiring = self
            .preparation(actor)
            .filter(|p| p.plan.work() != plan.work())
            .and_then(Preparation::item_lineage);
        let mut result = Vec::new();
        for ordinal in 0..plan.item_cost_count() {
            let ordinal = ordinal as u8;
            let mut need = plan
                .item_cost_quantity(ordinal)
                .ok_or(GameError::ItemUnavailable)?;
            let candidates: Box<dyn Iterator<Item = ItemId> + '_> = match plan.item_contract() {
                ItemContract::ExactUnit { item } => Box::new(std::iter::once(item)),
                ItemContract::Supplies { selection, .. } => match selection {
                    SupplySelection::Carried => {
                        Box::new(self.items.at(ItemLocation::Carried(actor)))
                    }
                    SupplySelection::Explicit { items } => Box::new(
                        items
                            .iter()
                            .filter(move |row| row.cost_ordinal == ordinal)
                            .map(|row| row.item),
                    ),
                },
                ItemContract::None => return Err(GameError::ItemUnavailable),
            };
            for id in candidates {
                let explicit = matches!(plan.item_contract(), ItemContract::ExactUnit { .. })
                    || matches!(
                        plan.supply_selection(),
                        Some(SupplySelection::Explicit { .. })
                    );
                if need == 0 {
                    if explicit {
                        return Err(GameError::ItemUnavailable);
                    }
                    break;
                }
                let reference = ItemClaimRef {
                    cost_ordinal: ordinal,
                    item: id,
                };
                let item = self.items.get(&id).ok_or(GameError::ItemUnavailable)?;
                if !self.plan_item_matches(plan, reference, item) {
                    if explicit {
                        return Err(GameError::ItemUnavailable);
                    }
                    continue;
                }
                let available = if committed {
                    item.free_quantity(retiring)
                        .ok_or(GameError::ItemUnavailable)?
                } else {
                    item.quantity
                };
                if available == 0 {
                    if explicit {
                        return Err(GameError::ItemUnavailable);
                    }
                    continue;
                }
                if result.len() == MAX_ITEM_FRAGMENTS
                    || (committed
                        && item
                            .holds
                            .iter()
                            .filter(|hold| Some(hold.key.lineage) != retiring)
                            .count()
                            >= MAX_ITEM_HOLDS)
                {
                    return Err(GameError::ItemUnavailable);
                }
                let quantity = available.min(need);
                result.push((reference, quantity));
                need -= quantity;
            }
            if need != 0 {
                return Err(GameError::ItemUnavailable);
            }
        }
        Ok(result)
    }
    /// AI previews use only disclosed, identified physical candidates.
    /// Execution separately validates allocation; holds cannot influence this choice.
    pub(crate) fn physical_ai_plan(
        &self,
        mut plan: Plan,
        view: &crate::Observation,
    ) -> Option<Plan> {
        if self.preparation(plan.actor()).is_some_and(|pending| {
            pending.plan == plan && self.preparation_still_available(pending, None)
        }) {
            return Some(plan);
        }
        if self.physical_item_allocation(&plan).is_ok() {
            return Some(plan);
        }
        let ability = plan.as_ability()?;
        let mut selected = Vec::new();
        for (ordinal, cost) in ability.supply_costs.iter().enumerate() {
            let mut need = cost.quantity;
            let mut carried = view.inventory.iter().map(|item| item.id).peekable();
            let mut ground = view.ground_items.iter().map(|item| item.id).peekable();
            loop {
                let id = match (carried.peek(), ground.peek()) {
                    (Some(a), Some(b)) if a <= b => carried.next(),
                    (Some(_), Some(_)) | (None, Some(_)) => ground.next(),
                    (Some(_), None) => carried.next(),
                    (None, None) => None,
                };
                let Some(id) = id else {
                    break;
                };
                let item = self.items.get(&id)?;
                if !self.known_supply(ability.actor, item)
                    || item.spec.supply.as_ref() != Some(&cost.supply)
                    || !self.supply_access(ability.actor, item, &cost.access)
                {
                    continue;
                }
                let available = item.quantity;
                if available == 0 {
                    continue;
                }
                if selected.len() == MAX_ITEM_FRAGMENTS {
                    return None;
                }
                selected.push(SelectedSupply {
                    cost_ordinal: ordinal as u8,
                    item: id,
                });
                need = need.saturating_sub(available);
                if need == 0 {
                    break;
                }
            }
            if need != 0 {
                return None;
            }
        }
        let Plan::Ability(ability) = &mut plan else {
            return None;
        };
        ability.supplies = SupplySelection::Explicit { items: selected };
        self.physical_item_allocation(&plan).ok()?;
        Some(plan)
    }

    /// Caller owns the outer action transaction and has retired replaced work.
    pub(crate) fn reserve_item_payment(
        &mut self,
        plan: &Plan,
        origin: Option<IntentionId>,
    ) -> Result<ItemPayment, GameError> {
        if !plan.needs_item_payment() {
            return Ok(ItemPayment::None);
        }
        let origin = origin
            .filter(|id| self.intention_issued(*id))
            .ok_or(GameError::Schedule(crate::ScheduleError::InvalidIntention))?;
        let lineage = PreparationLineage {
            actor: plan.actor(),
            origin,
        };
        let allocation = self.item_allocation(plan)?;
        let mut refs = Vec::with_capacity(allocation.len());
        for (reference, quantity) in allocation {
            let key = ItemClaimKey {
                lineage,
                cost_ordinal: reference.cost_ordinal,
            };
            self.items
                .edit(reference.item, |item| {
                    let position = item
                        .holds
                        .binary_search_by_key(&key, |hold| hold.key)
                        .map_or_else(Ok, |_| Err(GameError::ItemUnavailable))?;
                    if item.free_quantity(None).is_none_or(|free| free < quantity)
                        || item.holds.len() >= MAX_ITEM_HOLDS
                    {
                        return Err(GameError::ItemUnavailable);
                    }
                    item.holds.insert(position, ItemHold { key, quantity });
                    Ok(())
                })
                .ok_or(GameError::ItemUnavailable)??;
            refs.push(reference);
        }
        Ok(ItemPayment::Held { refs })
    }
    pub(crate) fn release_item_payment(
        &mut self,
        preparation: &Preparation,
    ) -> Result<(), GameError> {
        let Some(lineage) = preparation.item_lineage() else {
            return if preparation.item_payment.refs().is_empty() {
                Ok(())
            } else {
                Err(GameError::ItemUnavailable)
            };
        };
        // Retirement is also used by direct authored lifecycle changes: all
        // references must validate before the first hold is removed.
        if preparation.item_payment.refs().iter().any(|reference| {
            let key = ItemClaimKey {
                lineage,
                cost_ordinal: reference.cost_ordinal,
            };
            self.items.get(&reference.item).is_none_or(|item| {
                item.holds
                    .binary_search_by_key(&key, |hold| hold.key)
                    .is_err()
            })
        }) {
            return Err(GameError::ItemUnavailable);
        }
        for reference in preparation.item_payment.refs() {
            let key = ItemClaimKey {
                lineage,
                cost_ordinal: reference.cost_ordinal,
            };
            self.items
                .edit(reference.item, |item| -> Result<(), GameError> {
                    let index = item
                        .holds
                        .binary_search_by_key(&key, |hold| hold.key)
                        .map_err(|_| GameError::ItemUnavailable)?;
                    item.holds.remove(index);
                    Ok(())
                })
                .ok_or(GameError::ItemUnavailable)??;
        }
        Ok(())
    }
    /// Validate the complete set before the first mutation; no second quantity ledger is retained.
    pub(crate) fn spend_item_payment(
        &mut self,
        preparation: &mut Preparation,
    ) -> Result<(), GameError> {
        if !matches!(preparation.item_payment, ItemPayment::Held { .. }) {
            return Ok(());
        }
        if !self.preparation_items_valid(preparation, false) {
            return Err(GameError::ItemUnavailable);
        }
        let lineage = preparation
            .item_lineage()
            .ok_or(GameError::ItemUnavailable)?;
        for reference in preparation.item_payment.refs() {
            let key = ItemClaimKey {
                lineage,
                cost_ordinal: reference.cost_ordinal,
            };
            let depleted = self
                .items
                .edit(reference.item, |item| -> Result<bool, GameError> {
                    let index = item
                        .holds
                        .binary_search_by_key(&key, |hold| hold.key)
                        .map_err(|_| GameError::ItemUnavailable)?;
                    let quantity = item.holds[index].quantity;
                    item.quantity = item
                        .quantity
                        .checked_sub(quantity)
                        .ok_or(GameError::ItemUnavailable)?;
                    item.holds.remove(index);
                    Ok(item.quantity == 0)
                })
                .ok_or(GameError::ItemUnavailable)??;
            if depleted {
                self.items.remove(&reference.item);
            }
        }
        preparation.item_payment = ItemPayment::Settled;
        Ok(())
    }
    pub(crate) fn preparation_items_valid(
        &self,
        preparation: &Preparation,
        allow_detached: bool,
    ) -> bool {
        if !preparation
            .item_payment
            .shape_valid(&preparation.plan, preparation.has_prefix())
        {
            return false;
        }
        if preparation.item_payment.refs().is_empty() {
            return true;
        }
        if let Some(SupplySelection::Explicit { items }) = preparation.plan.supply_selection() {
            if !items
                .iter()
                .map(|row| (row.cost_ordinal, row.item))
                .eq(preparation
                    .item_payment
                    .refs()
                    .iter()
                    .map(|row| (row.cost_ordinal, row.item)))
            {
                return false;
            }
        }
        let Some(lineage) = preparation
            .item_lineage()
            .filter(|lineage| self.intention_issued(lineage.origin))
        else {
            return false;
        };
        let mut totals = BTreeMap::<u8, u64>::new();
        let mut missing = std::collections::BTreeSet::new();
        for reference in preparation.item_payment.refs() {
            let Some(item) = self.items.get(&reference.item) else {
                if allow_detached && self.detached_item(reference.item) {
                    missing.insert(reference.cost_ordinal);
                    continue;
                }
                return false;
            };
            let key = ItemClaimKey {
                lineage,
                cost_ordinal: reference.cost_ordinal,
            };
            let Some(hold) = item.holds.iter().find(|hold| hold.key == key) else {
                return false;
            };
            if !item.holds_valid() || !self.plan_item_matches(&preparation.plan, *reference, item) {
                return false;
            }
            let total = totals.entry(reference.cost_ordinal).or_default();
            let Some(sum) = total.checked_add(hold.quantity) else {
                return false;
            };
            *total = sum;
        }
        (0..preparation.plan.item_cost_count()).all(|ordinal| {
            let ordinal = ordinal as u8;
            let expected = preparation.plan.item_cost_quantity(ordinal).unwrap_or(0);
            let actual = totals.get(&ordinal).copied().unwrap_or(0);
            if missing.contains(&ordinal) {
                actual < expected
            } else {
                actual == expected
            }
        })
    }
    pub(crate) fn item_claims_valid(&self, allow_detached: bool) -> bool {
        self.actors.values().all(|actor| {
            actor
                .pending
                .as_ref()
                .is_none_or(|p| self.preparation_items_valid(p, allow_detached))
        }) && self.items.claimed().all(|(id, item)| {
            item.holds_valid()
                && item.holds.iter().all(|hold| {
                    if !self.intention_issued(hold.key.lineage.origin) {
                        return false;
                    }
                    match self.actors.get(&hold.key.lineage.actor) {
                        Some(actor) => actor.pending.as_ref().is_some_and(|p| {
                            p.item_lineage() == Some(hold.key.lineage)
                                && p.item_payment
                                    .refs()
                                    .binary_search(&ItemClaimRef {
                                        cost_ordinal: hold.key.cost_ordinal,
                                        item: id,
                                    })
                                    .is_ok()
                        }),
                        None => allow_detached && self.detached_actor(hold.key.lineage.actor),
                    }
                })
        })
    }
    /// Forced ground movement invalidates the whole lineage, even if it stays in reach.
    pub(crate) fn invalidate_item_claimants(&mut self, item: ItemId) -> Result<(), GameError> {
        let owners: std::collections::BTreeSet<_> = self
            .items
            .get(&item)
            .ok_or(GameError::ItemUnavailable)?
            .holds
            .iter()
            .map(|hold| hold.key.lineage)
            .collect();
        for owner in owners {
            if self
                .preparation(owner.actor)
                .and_then(Preparation::item_lineage)
                != Some(owner)
            {
                return Err(GameError::ItemUnavailable);
            }
            let mut outcome = self.actor_outcome(owner.actor)?;
            outcome.interrupt(
                crate::lifecycle::Interruption::Discard,
                self.actor_clock(owner.actor),
            )?;
            self.apply_outcome(outcome)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn phase5_carried_selection_rejects_surplus_fields() {
        assert!(
            serde_json::from_str::<SupplySelection>(r#"{"kind":"carried","items":[]}"#).is_err()
        );
    }
    #[test]
    fn phase5_settled_payment_rejects_surplus_claims() {
        assert!(serde_json::from_str::<ItemPayment>(r#"{"kind":"settled","refs":[]}"#).is_err());
    }
    #[test]
    fn phase5_absent_payment_rejects_surplus_claims() {
        assert!(serde_json::from_str::<ItemPayment>(r#"{"kind":"none","refs":[]}"#).is_err());
    }
    #[test]
    fn phase5_explicit_selection_cannot_reuse_one_source_across_costs() {
        let selection = SupplySelection::Explicit {
            items: vec![
                SelectedSupply {
                    cost_ordinal: 0,
                    item: ItemId(10),
                },
                SelectedSupply {
                    cost_ordinal: 1,
                    item: ItemId(10),
                },
            ],
        };
        assert!(!selection.valid(2));
    }
    #[test]
    fn phase5_explicit_selection_must_cover_each_authored_cost() {
        let selection = SupplySelection::Explicit {
            items: vec![SelectedSupply {
                cost_ordinal: 0,
                item: ItemId(10),
            }],
        };
        assert!(selection.valid(1));
        assert!(!selection.valid(2));
    }
    #[test]
    fn phase5_selected_supply_is_a_named_record() {
        assert!(serde_json::from_str::<SelectedSupply>("[0,10]").is_err());
    }
    #[test]
    fn phase5_claim_reference_is_a_named_record() {
        assert!(serde_json::from_str::<ItemClaimRef>("[0,10]").is_err());
    }
    #[test]
    fn phase5_lineage_is_a_named_record() {
        assert!(serde_json::from_str::<PreparationLineage>("[1,1]").is_err());
    }
    #[test]
    fn phase5_claim_key_is_a_named_record() {
        assert!(serde_json::from_str::<ItemClaimKey>(r#"[{"actor":1,"origin":1},0]"#).is_err());
    }
    #[test]
    fn phase5_item_hold_is_a_named_record() {
        assert!(serde_json::from_str::<ItemHold>(
            r#"[{"lineage":{"actor":1,"origin":1},"cost_ordinal":0},1]"#
        )
        .is_err());
    }
    fn split_fixture() -> (Game, ActorId, ItemId) {
        use std::num::NonZeroU64;
        use tor_world::{Location, Position, RegionId};
        let mut game = Game::two_room_in_stone(42, crate::test_creatures::content());
        let at = Location {
            region: RegionId(1),
            position: Position { x: 1, y: 1, z: 0 },
        };
        let actor = game.spawn_actor(at, NonZeroU64::new(1).unwrap()).unwrap();
        let mut spec = crate::ItemSpec::ordinary("split supplies".into());
        spec.stackable = true;
        let source = game.place_item_stack(10, at, None, 2, spec).unwrap();
        (game, actor, source)
    }

    #[test]
    fn phase5_transfer_mutation_rejects_identity_exhaustion_before_splitting() {
        // Ordinary prepare_transfer already rejects exhaustion. This deliberately
        // exercises the private mutation guard without its caller's preflight.
        let (mut game, actor, source) = split_fixture();
        game.next_item_id = u64::MAX;
        let before = game.clone();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            game.apply_transfer(source, ItemId(u64::MAX), 1, ItemLocation::Carried(actor))
        }));
        assert_eq!(result.ok(), Some(Err(GameError::IdentityExhausted)));
        assert_eq!(game, before);
    }

    #[test]
    fn phase5_transfer_mutation_requires_the_exact_next_split_identity() {
        let (mut game, actor, source) = split_fixture();
        let future = ItemId(game.next_item_id + 1);
        let before = game.clone();
        assert_eq!(
            game.apply_transfer(source, future, 1, ItemLocation::Carried(actor)),
            Err(GameError::ItemUnavailable)
        );
        assert_eq!(game, before);
    }

    #[test]
    fn phase5_transfer_mutation_rejects_destination_overflow_before_source_change() {
        use std::num::NonZeroU64;
        use tor_world::{Location, Position, RegionId};
        let mut game = Game::two_room_in_stone(42, crate::test_creatures::content());
        let at = Location {
            region: RegionId(1),
            position: Position { x: 1, y: 1, z: 0 },
        };
        let actor = game.spawn_actor(at, NonZeroU64::new(1).unwrap()).unwrap();
        crate::test_creatures::configure(
            &mut game,
            actor,
            "neutral",
            crate::test_creatures::species(),
        );
        game.spawn_actor(
            Location {
                position: Position { x: 2, y: 1, z: 0 },
                ..at
            },
            NonZeroU64::new(1).unwrap(),
        )
        .unwrap();
        let mut spec = crate::ItemSpec::ordinary("bandage".into());
        spec.stackable = true;
        spec.supply = Some(crate::content_ids::SupplyId::new("bandage").unwrap());
        let destination = game
            .place_item_stack(10, at, Some(actor), u64::MAX, spec.clone())
            .unwrap();
        let source = game.place_item_stack(11, at, None, 1, spec).unwrap();
        game.admit_intention(
            actor,
            crate::Action::UseAbility {
                id: crate::content_ids::AbilityId::new("bind_wounds").unwrap(),
                invocation: crate::abilities::AbilityInvocation::ActorTarget { target: actor },
                supplies: SupplySelection::Carried,
            },
            crate::IntentionOrigin::Human,
        )
        .unwrap();
        game.execute_next_intention().unwrap().outcome.unwrap();
        assert!(matches!(
            game.preparation(actor).unwrap().item_payment,
            ItemPayment::Held { .. }
        ));
        let before = game.clone();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            game.apply_transfer(source, destination, 1, ItemLocation::Carried(actor))
        }));
        assert_eq!(result.ok(), Some(Err(GameError::InvalidQuantity)));
        assert_eq!(game, before);
    }
    #[test]
    fn phase5_transfer_mutation_cannot_spend_a_held_unit() {
        use std::num::NonZeroU64;
        use tor_world::{Location, Position, RegionId};
        let mut game = Game::two_room_in_stone(42, crate::test_creatures::content());
        let at = Location {
            region: RegionId(1),
            position: Position { x: 1, y: 1, z: 0 },
        };
        let actor = game.spawn_actor(at, NonZeroU64::new(1).unwrap()).unwrap();
        crate::test_creatures::configure(
            &mut game,
            actor,
            "neutral",
            crate::test_creatures::species(),
        );
        let mut spec = crate::ItemSpec::ordinary("bandage".into());
        spec.supply = Some(crate::content_ids::SupplyId::new("bandage").unwrap());
        let item = game.place_item_stack(10, at, Some(actor), 1, spec).unwrap();
        game.spawn_actor(
            Location {
                position: Position { x: 2, y: 1, z: 0 },
                ..at
            },
            NonZeroU64::new(1).unwrap(),
        )
        .unwrap();
        game.admit_intention(
            actor,
            crate::Action::UseAbility {
                id: crate::content_ids::AbilityId::new("bind_wounds").unwrap(),
                invocation: crate::abilities::AbilityInvocation::ActorTarget { target: actor },
                supplies: SupplySelection::Carried,
            },
            crate::IntentionOrigin::Human,
        )
        .unwrap();
        game.execute_next_intention().unwrap().outcome.unwrap();
        assert!(matches!(
            game.preparation(actor).unwrap().item_payment,
            ItemPayment::Held { .. }
        ));
        assert_eq!(game.items[&item].quantity, 1);
        let before = game.clone();
        assert_eq!(
            game.apply_transfer(item, item, 1, ItemLocation::Ground(at)),
            Err(GameError::ItemUnavailable)
        );
        assert_eq!(game, before);
    }
}
