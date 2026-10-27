//! What the pack panel offers, planned from the disclosed view alone. Drawing
//! and input both follow this plan, so a button and its number key agree.
use crate::interact::PanelButton;
use crate::net::ProjectilePrompt;
use tor_client_3d_model::View;
use tor_client_common::items::ItemOperation;

/// Number keys reach the first actions in order: 1 to 9, then 0.
pub(crate) const HOTKEYS: usize = 10;

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Line {
    Heading(String),
    Note(String),
    Row {
        label: String,
        detail: Option<String>,
        actions: Vec<(&'static str, PanelButton)>,
    },
}

fn row(label: impl Into<String>, actions: Vec<(&'static str, PanelButton)>) -> Line {
    Line::Row {
        label: label.into(),
        detail: None,
        actions,
    }
}

pub(crate) fn plan(view: &View, projectile: Option<&ProjectilePrompt>) -> Vec<Line> {
    let mut lines = Vec::new();
    if let Some(prompt) = projectile {
        lines.push(Line::Heading(prompt.title.clone()));
        for (index, text) in prompt.rows.iter().enumerate() {
            lines.push(if prompt.choosing {
                row(
                    text.clone(),
                    vec![("Choose", PanelButton::ProjectileChoose(index))],
                )
            } else {
                Line::Note(text.clone())
            });
        }
        let mut paging = Vec::new();
        if prompt.previous {
            paging.push(("Previous", PanelButton::ProjectilePrevious));
        }
        if prompt.next {
            paging.push(("Next", PanelButton::ProjectileNext));
        }
        paging.push(("Cancel", PanelButton::ProjectileCancel));
        lines.push(row("", paging));
    }
    if let Some(choices) = &view.identify {
        lines.push(Line::Heading("Identify which item?".into()));
        for item in view.inventory.iter().filter(|i| choices.contains(&i.id)) {
            lines.push(row(
                item.name.clone(),
                vec![("Identify", PanelButton::Identify(Some(item.id)))],
            ));
        }
        if choices.is_empty() {
            lines.push(Line::Note("Nothing you carry can be identified.".into()));
        }
        lines.push(row(
            "Leave it",
            vec![("Decline", PanelButton::Identify(None))],
        ));
    }
    if !view.holds.is_empty() {
        lines.push(Line::Heading("Held".into()));
        for hold in &view.holds {
            lines.push(row(
                hold.label.clone(),
                vec![("Escape", PanelButton::Escape(hold.target))],
            ));
        }
    }
    if !view.abilities.is_empty() {
        lines.push(Line::Heading("Abilities".into()));
        for (index, ability) in view.abilities.iter().enumerate() {
            lines.push(row(
                ability.label.clone(),
                vec![("Use", PanelButton::Ability(index))],
            ));
        }
    }
    lines.push(Line::Heading("Pack".into()));
    if view.inventory.is_empty() {
        lines.push(Line::Note("You carry nothing.".into()));
    }
    for item in &view.inventory {
        let mut label = item.name.clone();
        if item.quantity > 1 {
            label.push_str(&format!(" x{}", item.quantity));
        }
        let mut actions = Vec::new();
        if let Some(light) = &item.portable_light {
            label.push_str(&format!(
                " [{}; fuel: {} ticks]",
                if light.lit { "lit" } else { "unlit" },
                light.remaining
            ));
            if light.ignitable {
                actions.push(("Ignite", PanelButton::Use(item.id, ItemOperation::Ignite)));
            }
            if light.extinguishable {
                actions.push((
                    "Extinguish",
                    PanelButton::Use(item.id, ItemOperation::Extinguish),
                ));
            }
        }
        if item.equipped.is_some() {
            actions.push(("Remove", PanelButton::Use(item.id, ItemOperation::Unequip)));
        } else if item.can_equip {
            actions.push(("Equip", PanelButton::Use(item.id, ItemOperation::Equip)));
        }
        if item.edible {
            actions.push(("Eat", PanelButton::Use(item.id, ItemOperation::Eat)));
        }
        if item.drinkable {
            actions.push(("Drink", PanelButton::Use(item.id, ItemOperation::Drink)));
        }
        if item.readable {
            actions.push(("Read", PanelButton::Use(item.id, ItemOperation::Read)));
        }
        match item.device {
            Some(tor_protocol::DeviceActivation::SelfTarget) => {
                actions.push(("Zap", PanelButton::Use(item.id, ItemOperation::Zap)));
            }
            Some(tor_protocol::DeviceActivation::Direction) => {
                actions.push(("Zap", PanelButton::Aim(item.id)));
            }
            Some(tor_protocol::DeviceActivation::DirectionOrSelf) => {
                actions.push(("Zap", PanelButton::Aim(item.id)));
                actions.push(("Zap self", PanelButton::ZapSelf(item.id)));
            }
            None => {}
        }
        if item.equipped.is_none() || item.portable_light.is_some() {
            actions.push(("Drop", PanelButton::Drop(item.id)));
        }
        lines.push(Line::Row {
            label,
            detail: item.equipped.clone(),
            actions,
        });
    }
    let reachable: Vec<_> = view.items.iter().filter(|i| i.reachable).collect();
    if !reachable.is_empty() {
        lines.push(Line::Heading("Within reach".into()));
        for item in reachable {
            let label = if item.quantity > 1 {
                format!("{} x{}", item.name, item.quantity)
            } else {
                item.name.clone()
            };
            lines.push(row(label, vec![("Take", PanelButton::Take(item.id))]));
        }
    }
    lines.push(Line::Note(
        "Number keys pick the first actions. I, Z or Esc: close".into(),
    ));
    lines
}

/// Every action in plan order, labelled "row: action".
pub(crate) fn actions(lines: &[Line]) -> Vec<(String, PanelButton)> {
    lines
        .iter()
        .flat_map(|line| match line {
            Line::Row { label, actions, .. } => actions
                .iter()
                .map(|(text, button)| (format!("{label}: {text}"), *button))
                .collect(),
            _ => Vec::new(),
        })
        .collect()
}

/// The number key shown on the action at this position, if it has one.
pub(crate) fn hotkey(position: usize) -> Option<char> {
    (position < HOTKEYS).then(|| char::from_digit(((position + 1) % 10) as u32, 10).unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tor_client_3d_model::Carried;
    use tor_protocol::{DeviceActivation, ItemClass, ItemTarget};

    fn carried(id: u8, name: &str) -> Carried {
        Carried {
            model: tor_client_3d_model::ItemModel::Placeholder,
            id: ItemTarget::from_digest([id; 32]),
            name: name.into(),
            class: ItemClass::Misc,
            quantity: 1,
            description: String::new(),
            equipped: None,
            can_equip: false,
            drinkable: false,
            portable_light: None,
            edible: false,
            readable: false,
            device: None,
        }
    }

    #[test]
    fn a_waiting_choice_comes_first_and_number_keys_follow_plan_order() {
        let mut view = crate::test_support::view(0);
        let scroll = Carried {
            readable: true,
            ..carried(1, "rune scroll")
        };
        let wand = Carried {
            device: Some(DeviceActivation::Direction),
            ..carried(2, "oak wand")
        };
        view.inventory = vec![scroll.clone(), wand.clone()];
        let listed = actions(&plan(&view, None));
        let labels: Vec<_> = listed.iter().map(|(label, _)| label.as_str()).collect();
        assert_eq!(
            labels,
            [
                "rune scroll: Read",
                "rune scroll: Drop",
                "oak wand: Zap",
                "oak wand: Drop"
            ]
        );
        assert_eq!(
            listed[2].1,
            PanelButton::Aim(wand.id),
            "a directional zap aims first"
        );
        view.identify = Some(vec![wand.id]);
        let listed = actions(&plan(&view, None));
        assert_eq!(listed[0].0, "oak wand: Identify");
        view.identify = None;
        view.inventory = vec![Carried {
            device: Some(DeviceActivation::DirectionOrSelf),
            ..carried(3, "bone wand")
        }];
        let dual: Vec<_> = actions(&plan(&view, None))
            .into_iter()
            .map(|(_, button)| button)
            .collect();
        let bone = view.inventory[0].id;
        assert_eq!(
            dual[..2],
            [PanelButton::Aim(bone), PanelButton::ZapSelf(bone)],
            "a wand that may target either way offers both"
        );
        assert_eq!(listed[1].0, "Leave it: Decline");
        assert_eq!(
            (hotkey(0), hotkey(8), hotkey(9), hotkey(10)),
            (Some('1'), Some('9'), Some('0'), None)
        );
    }
}
