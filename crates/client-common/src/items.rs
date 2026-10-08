//! Item choices derived solely from the current disclosed inventory and anatomy.
use tor_protocol::{Action, EquipmentSlot, ItemTarget, ItemView, Observation};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemOperation {
    Equip,
    Unequip,
    Drink,
}

/// Potential choices in disclosed inventory order. A choice may still be blocked
/// by occupied anatomy; selecting it lets the client explain the current refusal.
pub fn item_choices(
    view: &Observation,
    operation: ItemOperation,
) -> impl Iterator<Item = &ItemView> {
    view.inventory.iter().filter(move |item| {
        let Some(affordance) = view.interactions.as_ref().and_then(|interactions| {
            interactions
                .inventory
                .iter()
                .find(|candidate| candidate.item == item.id)
        }) else {
            return false;
        };
        match operation {
            ItemOperation::Equip => affordance.slot.is_some() && affordance.equipped_slot.is_none(),
            ItemOperation::Unequip => affordance.equipped_slot.is_some(),
            ItemOperation::Drink => affordance.drinkable,
        }
    })
}
impl ItemOperation {
    pub fn verb(self) -> &'static str {
        match self {
            Self::Equip => "equip",
            Self::Unequip => "remove",
            Self::Drink => "drink",
        }
    }
}

/// Select the first free matching socket in disclosed anatomy order. Replacement
/// always requires a separate removal; a hidden statistic never selects an item.
pub fn item_action(
    view: &Observation,
    item: ItemTarget,
    operation: ItemOperation,
) -> Result<Action, String> {
    if !view.inventory.iter().any(|candidate| candidate.id == item) {
        return Err("You aren't carrying that item.".into());
    }
    let interactions = view
        .interactions
        .as_ref()
        .ok_or("That item has no usable interaction.")?;
    let affordance = interactions
        .inventory
        .iter()
        .find(|candidate| candidate.item == item)
        .ok_or("That item has no usable interaction.")?;
    match operation {
        ItemOperation::Drink if affordance.drinkable && view.combat.is_some() => {
            Ok(Action::Drink { item })
        }
        ItemOperation::Drink => Err("You can't drink that item.".into()),
        ItemOperation::Unequip if affordance.equipped_slot.is_some() => {
            Ok(Action::Unequip { item })
        }
        ItemOperation::Unequip => Err("That item isn't equipped.".into()),
        ItemOperation::Equip => {
            if affordance.equipped_slot.is_some() {
                return Err("That item is already equipped.".into());
            }
            let kind = affordance.slot.ok_or("You can't equip that item.")?;
            if kind == EquipmentSlot::Weapon
                && interactions.inventory.iter().any(|candidate| {
                    candidate.slot == Some(kind) && candidate.equipped_slot.is_some()
                })
            {
                return Err("Remove your equipped weapon first.".into());
            }
            let slot = interactions
                .slots
                .iter()
                .enumerate()
                .find_map(|(index, slot)| {
                    let index = u16::try_from(index).ok()?;
                    (*slot == kind
                        && !interactions
                            .inventory
                            .iter()
                            .any(|candidate| candidate.equipped_slot == Some(index)))
                    .then_some(index)
                })
                .ok_or("There is no free matching equipment slot. Remove an item first.")?;
            Ok(Action::Equip { item, slot })
        }
    }
}
