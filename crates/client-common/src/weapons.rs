//! Exact optional attack equipment choices from the owner's current disclosure.
use tor_protocol::{ItemTarget, Observation, WeaponSelection};

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct WeaponChoice {
    pub item: ItemTarget,
    pub socket: u16,
    pub name: String,
    pub equipped: String,
}

/// Socket capability is disclosed even when an item's attack numbers are unknown.
/// Inventory order is presentation only; requests retain exact item/socket pairs.
pub fn choices(view: &Observation) -> Vec<WeaponChoice> {
    let Some(interactions) = &view.interactions else {
        return Vec::new();
    };
    view.inventory
        .iter()
        .filter_map(|item| {
            let offer = interactions
                .inventory
                .iter()
                .find(|offer| offer.item == item.id)?;
            let socket = offer.equipped_slot?;
            let slot = interactions.slots.get(usize::from(socket))?;
            slot.requires_attack.then(|| WeaponChoice {
                item: item.id,
                socket,
                name: item.name.clone(),
                equipped: slot.equipped_phrase.clone(),
            })
        })
        .collect()
}

pub fn selection(view: &Observation, item: ItemTarget) -> Result<WeaponSelection, String> {
    let rows: Vec<_> = choices(view)
        .into_iter()
        .filter(|row| row.item == item)
        .collect();
    match rows.as_slice() {
        [row] => Ok(WeaponSelection::Equipped {
            item: row.item,
            socket: row.socket,
        }),
        _ => Err("Choose an equipped weapon from your current inventory.".into()),
    }
}

pub fn validate(view: &Observation, selected: WeaponSelection) -> Result<(), String> {
    match selected {
        WeaponSelection::Default => Ok(()),
        WeaponSelection::Equipped { item, socket } => {
            if selection(view, item)? == (WeaponSelection::Equipped { item, socket }) {
                Ok(())
            } else {
                Err(
                    "That weapon is no longer equipped in the selected socket. Choose again."
                        .into(),
                )
            }
        }
    }
}

pub fn selection_from_text(view: &Observation, text: &str) -> Result<WeaponSelection, String> {
    let text = text.trim().to_lowercase();
    let noun = text
        .strip_prefix("the ")
        .or_else(|| text.strip_prefix("my "))
        .unwrap_or(&text);
    if noun.is_empty() {
        return Err("Name an equipped weapon after 'with'.".into());
    }
    let exact = noun
        .strip_prefix('#')
        .and_then(|id| id.parse::<ItemTarget>().ok());
    let rows: Vec<_> = choices(view)
        .into_iter()
        .filter(|row| {
            exact == Some(row.item)
                || (exact.is_none()
                    && !noun.starts_with('#')
                    && row.name.to_lowercase().contains(noun))
        })
        .collect();
    match rows.as_slice() {
        [row] => Ok(WeaponSelection::Equipped {
            item: row.item,
            socket: row.socket,
        }),
        [] => Err("No equipped weapon matches that name.".into()),
        _ => Err(format!(
            "Which equipped weapon? Use with #id: {}",
            rows.iter()
                .map(|row| format!("{} (#{})", row.name, row.item))
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

/// Comes before the optional `using` supply clause in both text clients.
pub fn split_clause(text: &str) -> Result<(&str, Option<&str>), String> {
    let lower = text.to_ascii_lowercase();
    let split = if lower.starts_with("with ") {
        Some(("", &text[5..]))
    } else {
        lower
            .find(" with ")
            .map(|index| (&text[..index], &text[index + 6..]))
    };
    match split {
        Some((aim, weapon))
            if !weapon.trim().is_empty() && !weapon.to_ascii_lowercase().contains(" with ") =>
        {
            Ok((aim.trim(), Some(weapon.trim())))
        }
        Some(_) => Err("Name one equipped weapon after 'with'.".into()),
        None if lower.trim() == "with" || lower.trim_end().ends_with(" with") => {
            Err("Name an equipped weapon after 'with'.".into())
        }
        None => Ok((text.trim(), None)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{equipment_slot, observation};
    use tor_protocol::*;

    fn view() -> Observation {
        let mut view = observation();
        let item = ItemView {
            corpse: None,
            id: ItemTarget::from_digest([80; 32]),
            class: ItemClass::Weapon,
            name: "bronze sword".into(),
            description: String::new(),
            quantity: 1,
            appearance: "weapon".into(),
            identified: false,
            known_currency: None,
            known_supply: None,
            asset: None,
        };
        let mut second = item.clone();
        second.id = ItemTarget::from_digest([81; 32]);
        second.name = "silver sword".into();
        view.inventory = vec![item.clone(), second.clone()];
        let slot = equipment_slot("weapon", "in hand", true);
        view.interactions = Some(InteractionView {
            identification: None,
            completed: vec![],
            curse_blocked: vec![],
            slots: vec![slot.clone(); 2],
            preparation: None,
            inventory: [item, second]
                .into_iter()
                .enumerate()
                .map(|(socket, item)| ItemInteractionView {
                    item: item.id,
                    slot: Some(slot.clone()),
                    equipped_slot: Some(socket as u16),
                    known_projectile_bonus: None,
                    known_equipment: None,
                    known_cursed: None,
                    known_protected: None,
                    drinkable: false,
                    edible: false,
                    readable: false,
                    zappable: false,
                    device_activation: None,
                    charges: None,
                })
                .collect(),
        });
        view
    }
    #[test]
    fn choices_use_only_owned_observed_attack_sockets_and_keep_exact_identity() {
        let mut view = view();
        let selected = selection_from_text(&view, "silver sword").unwrap();
        assert_eq!(
            selected,
            WeaponSelection::Equipped {
                item: view.inventory[1].id,
                socket: 1
            }
        );
        assert!(selection_from_text(&view, "sword").is_err());
        assert_eq!(
            selection_from_text(&view, &format!("#{}", view.inventory[1].id)).unwrap(),
            selected
        );
        view.inventory.reverse();
        assert!(validate(&view, selected).is_ok());
        view.interactions.as_mut().unwrap().inventory[1].equipped_slot = Some(0);
        assert!(
            validate(&view, selected).is_err(),
            "never rebind an exact item to a new socket"
        );
        view.interactions.as_mut().unwrap().inventory[1].equipped_slot = None;
        assert!(selection_from_text(&view, "silver sword").is_err());
        view.interactions.as_mut().unwrap().slots[0].requires_attack = false;
        assert!(choices(&view).is_empty());
        assert!(validate(&view, WeaponSelection::Default).is_ok());
    }
    #[test]
    fn explicit_invocations_require_server_permission_and_current_owned_equipment() {
        let mut view = view();
        let target = view.self_target;
        let ability = &mut view
            .combat
            .as_mut()
            .unwrap()
            .own_stats
            .as_mut()
            .unwrap()
            .abilities[0];
        ability.reach = AbilityReach::SelfOnly;
        ability.target_kind = AbilityTargetKind::SelfOnly;
        ability.weapon_selection = true;
        let id = ability.id.clone();
        let selected = selection(&view, view.inventory[1].id).unwrap();
        let invocation = AbilityInvocation {
            aim: AbilityAim::ActorTarget { target },
            weapon: selected,
        };
        assert!(crate::abilities::invocation_action(
            &view,
            &id,
            invocation,
            SupplySelection::Carried
        )
        .is_ok());
        view.combat
            .as_mut()
            .unwrap()
            .own_stats
            .as_mut()
            .unwrap()
            .abilities[0]
            .weapon_selection = false;
        assert_eq!(
            crate::abilities::invocation_action(&view, &id, invocation, SupplySelection::Carried),
            Err("That ability does not select a weapon.".into())
        );
        assert!(crate::abilities::invocation_action(
            &view,
            &id,
            AbilityInvocation {
                weapon: WeaponSelection::Default,
                ..invocation
            },
            SupplySelection::Carried
        )
        .is_ok());
        view.combat
            .as_mut()
            .unwrap()
            .own_stats
            .as_mut()
            .unwrap()
            .abilities[0]
            .weapon_selection = true;
        view.inventory.pop();
        assert!(crate::abilities::invocation_action(
            &view,
            &id,
            invocation,
            SupplySelection::Carried
        )
        .is_err());
        view.interactions = None;
        assert!(choices(&view).is_empty());
    }
    #[test]
    fn weapon_clause_is_optional_closed_and_separate_from_supplies() {
        assert_eq!(split_clause("goblin").unwrap(), ("goblin", None));
        assert_eq!(
            split_clause("goblin with bronze sword").unwrap(),
            ("goblin", Some("bronze sword"))
        );
        assert_eq!(
            split_clause("with bronze sword").unwrap(),
            ("", Some("bronze sword"))
        );
        for invalid in ["goblin with", "with", "goblin with sword with axe"] {
            assert!(split_clause(invalid).is_err());
        }
    }
}
