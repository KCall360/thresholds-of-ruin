//! Ability affordances from the attached actor's disclosed build. These are
//! suggestions, never authority: the server rechecks grants and targets, and
//! reserves resources only when preparation actually starts.
use tor_protocol::{Ability, Action, ActorTarget, Observation, Technique};

pub fn choices(view: &Observation) -> Vec<Ability> {
    view.combat
        .as_ref()
        .and_then(|combat| combat.own_stats.as_ref())
        .map(|stats| {
            stats
                .abilities
                .iter()
                .filter_map(|technique| match technique {
                    Technique::BasicMelee => None,
                    Technique::PowerStrike => Some(Ability::PowerStrike),
                    Technique::MagicBolt => Some(Ability::MagicBolt),
                    Technique::Fear => Some(Ability::Fear),
                })
                .collect()
        })
        .unwrap_or_default()
}

pub fn action(view: &Observation, ability: Ability, target: ActorTarget) -> Result<Action, String> {
    if !choices(view).contains(&ability) {
        return Err(format!(
            "You don't have {}.",
            crate::narration::ability_name(ability)
        ));
    }
    if target == view.self_target || !view.visible_actors.iter().any(|actor| actor.id == target) {
        return Err("That creature is not a visible target.".into());
    }
    // Funding and physical reach stay authoritative. In particular, a retained
    // preparation may already own a hold, and queueing never pays a cost.
    Ok(Action::UseAbility { ability, target })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn choices_and_actions_use_only_personal_grants_and_visible_nonself_targets() {
        let target = ActorTarget::from_digest([2; 32]);
        let mut view: Observation = serde_json::from_value(serde_json::json!({
            "actor": "1", "self_target": ActorTarget::from_digest([1; 32]), "tick": "0",
            "position": {"x": 0, "y": 0, "z": 0}, "ready": true,
            "places": [], "visible_cells": [], "ground_items": [], "inventory": [],
            "visible_actors": [{"id": target, "name": "goblin", "description": "", "position": {"x": 6, "y": 0, "z": 0}}],
            "combat": {"hp": 1, "max_hp": 1, "preparation_remaining": null, "preparation_active": false,
                "recovery_remaining": "0", "actors": [], "events": [], "objective": null,
                "victory": false, "dead": false, "terminal": false,
                "own_stats": {"kind": "humanoid", "subtypes": [], "hit_dice": ["warrior"],
                    "attributes": {"strength": 1, "speed": 1, "intellect": 1, "willpower": 1, "awareness": 1, "presence": 1},
                    "skills": [], "defenses": {"physical": 12, "cognitive": 12, "spiritual": 12},
                    "binding": "intellect", "resources": [], "active_talents": [], "dormant_talents": [],
                    "abilities": ["basic_melee", "power_strike", "magic_bolt", "fear"]}
            }
        })).unwrap();
        let before = view.clone();
        assert_eq!(
            choices(&view),
            [Ability::PowerStrike, Ability::MagicBolt, Ability::Fear]
        );
        for ability in choices(&view) {
            assert_eq!(
                action(&view, ability, target),
                Ok(Action::UseAbility { ability, target })
            );
            assert!(action(&view, ability, view.self_target).is_err());
            assert!(action(&view, ability, ActorTarget::from_digest([9; 32])).is_err());
        }
        assert_eq!(view, before);
        view.combat
            .as_mut()
            .unwrap()
            .own_stats
            .as_mut()
            .unwrap()
            .abilities = vec![Technique::BasicMelee];
        assert!(choices(&view).is_empty());
        assert_eq!(
            action(&view, Ability::Fear, target),
            Err("You don't have fear.".into())
        );
        view.combat = None;
        assert!(choices(&view).is_empty());
    }
}
