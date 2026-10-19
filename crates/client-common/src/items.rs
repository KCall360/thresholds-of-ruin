//! Item choices derived solely from the current disclosed inventory and anatomy.
use tor_protocol::{Action, Direction, ItemTarget, ItemView, Observation};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemOperation {
    Equip,
    Unequip,
    Drink,
    Eat,
    Read,
    Zap,
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
            ItemOperation::Eat => affordance.edible,
            ItemOperation::Read => affordance.readable,
            ItemOperation::Zap => affordance.zappable,
        }
    })
}
impl ItemOperation {
    pub fn verb(self) -> &'static str {
        match self {
            Self::Equip => "equip",
            Self::Unequip => "remove",
            Self::Drink => "drink",
            Self::Eat => "eat",
            Self::Read => "read",
            Self::Zap => "zap",
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
        ItemOperation::Eat if affordance.edible && view.combat.is_some() => {
            Ok(Action::Eat { item })
        }
        ItemOperation::Eat => Err("You can't eat that item.".into()),
        ItemOperation::Read if affordance.readable && view.combat.is_some() => {
            Ok(Action::Read { item })
        }
        ItemOperation::Read => Err("You can't read that item.".into()),
        ItemOperation::Zap if affordance.zappable && view.combat.is_some() => {
            Err("Choose a direction to zap.".into())
        }
        ItemOperation::Zap => Err("You can't zap that item.".into()),
        ItemOperation::Unequip if affordance.equipped_slot.is_some() => {
            Ok(Action::Unequip { item })
        }
        ItemOperation::Unequip => Err("That item isn't equipped.".into()),
        ItemOperation::Equip => {
            if affordance.equipped_slot.is_some() {
                return Err("That item is already equipped.".into());
            }
            let kind = affordance
                .slot
                .as_ref()
                .ok_or("You can't equip that item.")?;
            let slot = interactions
                .slots
                .iter()
                .enumerate()
                .find_map(|(index, slot)| {
                    let index = u16::try_from(index).ok()?;
                    (slot.id == kind.id
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

/// Use only the disclosed device affordance. Unknown or empty charge counts must
/// never become an eligibility oracle; the backend owns paid no-op settlement.
pub fn zap_action(
    view: &Observation,
    item: ItemTarget,
    direction: Direction,
) -> Result<Action, String> {
    if view.combat.is_none()
        || !item_choices(view, ItemOperation::Zap).any(|candidate| candidate.id == item)
    {
        return Err("You can't zap that item.".into());
    }
    Ok(Action::Zap { item, direction })
}

/// Resolve or abandon the current paid owner choice using its exact opaque token.
pub fn identification_action(
    view: &Observation,
    item: Option<ItemTarget>,
) -> Result<Action, String> {
    let pending = view
        .interactions
        .as_ref()
        .and_then(|interaction| interaction.identification.as_ref())
        .ok_or("There is no identification choice waiting.")?;
    if let Some(item) = item {
        if !pending.items.contains(&item)
            || !view
                .inventory
                .iter()
                .any(|candidate| candidate.id == item && candidate.class == pending.class)
        {
            return Err("That item is not an available identification choice.".into());
        }
    }
    Ok(Action::Identify {
        choice: pending.choice.clone(),
        item,
    })
}

pub fn identification_prompt(view: &Observation) -> Option<String> {
    let pending = view.interactions.as_ref()?.identification.as_ref()?;
    let items = view
        .inventory
        .iter()
        .filter(|item| pending.items.contains(&item.id) && item.class == pending.class)
        .map(|item| {
            format!(
                "{} (#{})",
                item.name
                    .chars()
                    .filter(|ch| !ch.is_control())
                    .collect::<String>(),
                item.id
            )
        })
        .collect::<Vec<_>>();
    Some(format!("Choose an item to identify: {}. Use identify <item> or identify cancel. The scroll has already been used.", if items.is_empty() { "no eligible carried items".into() } else { items.join(", ") }))
}

/// Instance counts are displayed only when explicitly disclosed, never by type.
pub fn charges(view: &Observation, item: ItemTarget) -> Option<u16> {
    view.interactions
        .as_ref()?
        .inventory
        .iter()
        .find(|candidate| candidate.item == item)?
        .charges
}

/// Display only the owner's remembered instance facts, never inferred bonuses.
pub fn equipment_details(view: &Observation, item: ItemTarget) -> Option<String> {
    let offer = view
        .interactions
        .as_ref()?
        .inventory
        .iter()
        .find(|offer| offer.item == item)?;
    let mut parts = Vec::new();
    if let Some(known) = &offer.known_equipment {
        if let Some(attack) = &known.attack {
            parts.push(format!(
                "attack {:+}, damage enchantment {:+}",
                attack.bonus, attack.damage_enchantment
            ));
        }
        for modifier in &known.modifiers {
            parts.extend(crate::combat_diagnostics::trace_lines(modifier));
        }
    }
    if let Some(bonus) = &offer.known_projectile_bonus {
        parts.push(format!(
            "remembered projectile hit bonus {:+}, damage bonus {:+}",
            bonus.hit, bonus.damage
        ));
    }
    if let Some(cursed) = offer.known_cursed {
        parts.push(if cursed { "cursed" } else { "uncursed" }.into());
    }
    if let Some(protected) = offer.known_protected {
        parts.push(
            if protected {
                "rust protected"
            } else {
                "not rust protected"
            }
            .into(),
        );
    }
    (!parts.is_empty()).then(|| parts.join(", "))
}
