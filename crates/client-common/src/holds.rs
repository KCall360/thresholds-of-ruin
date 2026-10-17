//! Selection and presentation of the observer's disclosed incoming holds.
use serde::Serialize;
use tor_protocol::{HoldTarget, Observation};

/// A choice freezes its exact opaque identity, never just its current row number.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct HoldChoice {
    pub target: HoldTarget,
    pub label: String,
}

pub fn hold_choices(observation: &Observation) -> Vec<HoldChoice> {
    observation
        .linked_holds
        .iter()
        .map(|hold| {
            let label = hold
                .holder
                .and_then(|holder| {
                    observation
                        .visible_actors
                        .iter()
                        .find(|actor| actor.id == holder)
                })
                .map(|actor| {
                    actor
                        .name
                        .chars()
                        .filter(|c| !c.is_control())
                        .collect::<String>()
                })
                .filter(|name| !name.trim().is_empty())
                .unwrap_or_else(|| "Unseen holder".into());
            HoldChoice {
                target: hold.target,
                label,
            }
        })
        .collect()
}

pub fn render_holds(observation: &Observation) -> Vec<String> {
    hold_choices(observation)
        .iter()
        .enumerate()
        .map(|(index, choice)| format!("{}. {} (#{})", index + 1, choice.label, choice.target))
        .collect()
}

/// Empty selects a sole offer; all other selectors are explicit current choices.
pub fn select_hold(observation: &Observation, selector: &str) -> Result<HoldTarget, String> {
    let choices = hold_choices(observation);
    let selector = selector.trim();
    let unavailable = || "That hold is no longer available.".to_owned();
    if choices.is_empty() {
        return Err("You are not held.".into());
    }
    if selector.is_empty() {
        return match choices.as_slice() {
            [choice] => Ok(choice.target),
            _ => Err(format!(
                "Choose a hold with escape hold <number|#token|name>.\n{}",
                render_holds(observation).join("\n")
            )),
        };
    }
    if let Some(token) = selector.strip_prefix('#') {
        let target: HoldTarget = token.parse().map_err(|_| unavailable())?;
        return choices
            .iter()
            .find(|choice| choice.target == target)
            .map(|choice| choice.target)
            .ok_or_else(unavailable);
    }
    if selector.bytes().all(|byte| byte.is_ascii_digit()) {
        return selector
            .parse::<usize>()
            .ok()
            .and_then(|index| index.checked_sub(1))
            .and_then(|index| choices.get(index))
            .map(|choice| choice.target)
            .ok_or_else(unavailable);
    }
    let query = selector.to_lowercase();
    let matches: Vec<_> = choices
        .iter()
        .filter(|choice| choice.label.to_lowercase().contains(&query))
        .collect();
    match matches.as_slice() {
        [choice] => Ok(choice.target),
        [] => Err(unavailable()),
        _ => Err(format!(
            "That name matches more than one hold. Choose a number or #token.\n{}",
            render_holds(observation).join("\n")
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tor_protocol::{ActorTarget, ActorView, LinkedHold, Position};

    #[test]
    fn sole_number_token_and_visible_name_select_the_exact_hold() {
        let mut view = crate::test_support::observation();
        let first = HoldTarget::from_digest([1; 32]);
        let second = HoldTarget::from_digest([2; 32]);
        view.linked_holds = vec![LinkedHold {
            target: first,
            holder: None,
        }];
        assert_eq!(select_hold(&view, "").unwrap(), first);
        assert_eq!(select_hold(&view, "1").unwrap(), first);
        assert_eq!(select_hold(&view, &format!("#{first}")).unwrap(), first);
        let holder = ActorTarget::from_digest([4; 32]);
        view.visible_actors.push(ActorView {
            id: holder,
            name: "Fly\ntrap".into(),
            description: String::new(),
            position: Position { x: 1, y: 0, z: 0 },
            asset: None,
        });
        view.linked_holds.push(LinkedHold {
            target: second,
            holder: Some(holder),
        });
        assert_eq!(select_hold(&view, "2").unwrap(), second);
        assert_eq!(select_hold(&view, "flytrap").unwrap(), second);
        assert!(select_hold(&view, "")
            .unwrap_err()
            .contains("Choose a hold"));
        assert_eq!(hold_choices(&view)[1].label, "Flytrap");
        for invalid in ["0", "3", "9999999999999999999999999999", "#bad", "unknown"] {
            assert!(select_hold(&view, invalid).is_err(), "{invalid}");
        }
        view.linked_holds.remove(0);
        assert!(select_hold(&view, &format!("#{first}")).is_err());
    }

    #[test]
    fn only_current_visible_names_are_used_and_ambiguity_requires_a_choice() {
        let mut view = crate::test_support::observation();
        view.linked_holds = [1, 2]
            .map(|id| LinkedHold {
                target: HoldTarget::from_digest([id; 32]),
                holder: Some(ActorTarget::from_digest([9; 32])),
            })
            .to_vec();
        assert!(hold_choices(&view)
            .iter()
            .all(|choice| choice.label == "Unseen holder"));
        assert!(select_hold(&view, "Unseen")
            .unwrap_err()
            .contains("more than one"));
        view.linked_holds.clear();
        assert_eq!(select_hold(&view, "").unwrap_err(), "You are not held.");
    }
}
