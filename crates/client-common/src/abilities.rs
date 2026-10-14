//! Dynamic ability affordances from the server-disclosed personal catalog.
use tor_protocol::{
    AbilityInvocation, AbilityTargetKind, AbilityView, Action, ActorTarget, ItemTarget,
    Observation, SelectedSupply, SupplyAccess, SupplySelection, MAX_SUPPLY_FRAGMENTS,
};
pub fn available(view: &Observation) -> &[AbilityView] {
    view.combat
        .as_ref()
        .and_then(|combat| combat.own_stats.as_ref())
        .map_or(&[], |stats| &stats.abilities)
}
pub fn choices(view: &Observation) -> Vec<AbilityView> {
    available(view)
        .iter()
        .filter(|ability| !ability.basic_attack)
        .cloned()
        .collect()
}
pub fn basic_action(view: &Observation, target: ActorTarget) -> Result<Action, String> {
    let ability = available(view)
        .iter()
        .find(|a| a.basic_attack)
        .ok_or("You don't have a basic attack.")?;
    action(view, &ability.id, target)
}
pub fn action(view: &Observation, id: &str, target: ActorTarget) -> Result<Action, String> {
    action_with_supplies(view, id, target, SupplySelection::Carried)
}
pub fn action_with_supplies(
    view: &Observation,
    id: &str,
    target: ActorTarget,
    supplies: SupplySelection,
) -> Result<Action, String> {
    invocation_action(
        view,
        id,
        AbilityInvocation::ActorTarget { target },
        supplies,
    )
}
pub fn invocation_action(
    view: &Observation,
    id: &str,
    invocation: AbilityInvocation,
    supplies: SupplySelection,
) -> Result<Action, String> {
    let ability = available(view)
        .iter()
        .find(|ability| ability.id == id)
        .ok_or_else(|| format!("You don't have {id}."))?;
    match invocation {
        AbilityInvocation::ActorTarget { target } if !target_eligible(view, ability, target) => {
            return Err("That creature is not a valid visible target.".into());
        }
        AbilityInvocation::SelfStep { .. }
            if ability.target_kind != AbilityTargetKind::SelfStep =>
        {
            return Err("That ability does not select a step.".into());
        }
        _ => {}
    }
    if !supplies.valid() {
        return Err("Choose valid exact supplies.".into());
    }
    if let SupplySelection::Explicit { items } = &supplies {
        let selected = selection_from_items(
            view,
            ability,
            &items.iter().map(|row| row.item).collect::<Vec<_>>(),
        )?;
        if selected != supplies {
            return Err("Those supplies do not match the ability's costs.".into());
        }
    }
    Ok(Action::UseAbility {
        id: id.into(),
        invocation,
        supplies,
    })
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct SupplyCandidate {
    pub cost_ordinal: u8,
    pub item: ItemTarget,
    pub name: String,
    pub quantity: u64,
    pub carried: bool,
}
/// Only observed capability and contact facts are eligible; physical quantities
/// do not reveal private holds or promise that admission will succeed.
pub fn supply_candidates(view: &Observation, ability: &AbilityView) -> Vec<SupplyCandidate> {
    supply_candidates_for_costs(view, &ability.supply_costs)
}
/// Shared finite-supply selection also serves ordinary respec, without
/// fabricating an ability or reconstructing undisclosed held quantities.
pub fn supply_candidates_for_costs(
    view: &Observation,
    costs: &[tor_protocol::AbilitySupplyCost],
) -> Vec<SupplyCandidate> {
    let mut seen = std::collections::BTreeSet::new();
    let mut values = Vec::new();
    for (item, carried) in view.inventory.iter().map(|item| (item, true)).chain(
        view.ground_items
            .iter()
            .filter(|ground| ground.supply_reachable)
            .map(|ground| (&ground.item, false)),
    ) {
        let Some(supply) = item.known_supply.as_ref().filter(|_| item.identified) else {
            continue;
        };
        if !seen.insert(item.id) {
            continue;
        }
        if let Some((ordinal, _)) = costs.iter().enumerate().find(|(_, cost)| {
            cost.supply == supply.id
                && (carried || cost.access == SupplyAccess::CarriedOrGroundTouch)
        }) {
            values.push(SupplyCandidate {
                cost_ordinal: ordinal as u8,
                item: item.id,
                name: item.name.clone(),
                quantity: item.quantity,
                carried,
            });
        }
    }
    values.sort_by_key(|row| (row.cost_ordinal, row.item));
    values
}
pub fn selection_from_items(
    view: &Observation,
    ability: &AbilityView,
    items: &[ItemTarget],
) -> Result<SupplySelection, String> {
    selection_from_items_for_costs(view, &ability.supply_costs, items)
}
pub fn selection_from_items_for_costs(
    view: &Observation,
    costs: &[tor_protocol::AbilitySupplyCost],
    items: &[ItemTarget],
) -> Result<SupplySelection, String> {
    if items.is_empty() || items.len() > MAX_SUPPLY_FRAGMENTS {
        return Err(format!(
            "Choose between 1 and {MAX_SUPPLY_FRAGMENTS} exact supplies."
        ));
    }
    let candidates = supply_candidates_for_costs(view, costs);
    let mut totals = vec![0_u64; costs.len()];
    let mut counts = vec![0_u64; costs.len()];
    let mut seen = std::collections::BTreeSet::new();
    let mut rows = Vec::with_capacity(items.len());
    for item in items {
        if !seen.insert(*item) {
            return Err("Choose each supply only once.".into());
        }
        let candidate = candidates
            .iter()
            .find(|row| row.item == *item)
            .ok_or("That supply is not a known accessible source for this ability.")?;
        totals[usize::from(candidate.cost_ordinal)] =
            totals[usize::from(candidate.cost_ordinal)].saturating_add(candidate.quantity);
        counts[usize::from(candidate.cost_ordinal)] += 1;
        rows.push(SelectedSupply {
            cost_ordinal: candidate.cost_ordinal,
            item: *item,
        });
    }
    if costs
        .iter()
        .zip(totals)
        .any(|(cost, quantity)| quantity < cost.quantity)
    {
        return Err("Choose enough physical supplies for every cost.".into());
    }
    // Each accepted exact source contributes at least one unit. Do not infer
    // further surplus from physical stack sizes: private holds may reduce them.
    if costs
        .iter()
        .zip(counts)
        .any(|(cost, count)| count > cost.quantity)
    {
        return Err("Choose no more exact sources than the required supply units.".into());
    }
    rows.sort();
    Ok(SupplySelection::Explicit { items: rows })
}
/// Scripted and adventure inputs share a small optional exact-source clause.
pub fn split_supply_clause(text: &str) -> Result<(&str, Option<&str>), String> {
    let lower = text.to_ascii_lowercase();
    let split = if lower.starts_with("using ") {
        Some(("", &text[6..]))
    } else {
        lower
            .find(" using ")
            .map(|index| (&text[..index], &text[index + 7..]))
    };
    match split {
        Some((target, supplies)) if !supplies.trim().is_empty() => {
            Ok((target.trim(), Some(supplies.trim())))
        }
        Some(_) => Err("Name the supplies after 'using'.".into()),
        None if lower.trim() == "using" || lower.trim_end().ends_with(" using") => {
            Err("Name the supplies after 'using'.".into())
        }
        None => Ok((text.trim(), None)),
    }
}
pub fn selection_from_text(
    view: &Observation,
    ability: &AbilityView,
    text: &str,
) -> Result<SupplySelection, String> {
    selection_from_text_for_costs(view, &ability.supply_costs, text)
}
pub fn selection_from_text_for_costs(
    view: &Observation,
    costs: &[tor_protocol::AbilitySupplyCost],
    text: &str,
) -> Result<SupplySelection, String> {
    if text.trim().eq_ignore_ascii_case("carried") {
        return Ok(SupplySelection::Carried);
    }
    let candidates = supply_candidates_for_costs(view, costs);
    let mut selected = Vec::new();
    for phrase in text.split(',') {
        let phrase = phrase.trim();
        if phrase.is_empty() {
            return Err("Name each exact supply, separated by commas.".into());
        }
        let opaque = phrase
            .strip_prefix('#')
            .and_then(|s| s.parse::<ItemTarget>().ok());
        let exact: Vec<_> = candidates
            .iter()
            .filter(|row| row.name.eq_ignore_ascii_case(phrase) || opaque == Some(row.item))
            .collect();
        let found = if exact.is_empty() {
            candidates
                .iter()
                .filter(|row| row.name.to_lowercase().contains(&phrase.to_lowercase()))
                .collect()
        } else {
            exact
        };
        match found.as_slice() {
            [row] => selected.push(row.item),
            [] => return Err("No matching known accessible supply is visible.".into()),
            _ => return Err("Which supply? Use its full name or opaque #item handle.".into()),
        }
    }
    selection_from_items_for_costs(view, costs, &selected)
}
pub fn ability_label(ability: &AbilityView) -> String {
    let mut parts = Vec::new();
    if ability.cooldown_remaining > 0 {
        parts.push(format!(
            "cooldown: {} active ticks",
            ability.cooldown_remaining
        ));
    }
    if !ability.supply_costs.is_empty() {
        parts.push(format!(
            "needs {}",
            ability
                .supply_costs
                .iter()
                .map(|cost| format!("{} {}", cost.quantity, cost.name))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if parts.is_empty() {
        ability.name.clone()
    } else {
        format!("{} ({})", ability.name, parts.join("; "))
    }
}
/// Uses only the server's disclosed target relationship. Reach remains authoritative.
pub fn target_eligible(view: &Observation, ability: &AbilityView, target: ActorTarget) -> bool {
    let visible = || view.visible_actors.iter().any(|actor| actor.id == target);
    match ability.target_kind {
        AbilityTargetKind::SelfStep => false,
        AbilityTargetKind::SelfOnly => target == view.self_target,
        AbilityTargetKind::Actor => target != view.self_target && visible(),
        AbilityTargetKind::SelfOrAlly => {
            target == view.self_target
                || (visible()
                    && view.combat.as_ref().is_some_and(|combat| {
                        combat
                            .actors
                            .iter()
                            .any(|actor| actor.actor == target && actor.allied && !actor.hostile)
                    }))
        }
    }
}
/// Match a disclosed name or ID at the start of input, preferring the longest
/// match and refusing ambiguous names. No game-specific vocabulary lives here.
pub fn match_command<'a, 'b>(
    view: &'a Observation,
    line: &'b str,
) -> Result<Option<(&'a AbilityView, &'b str)>, String> {
    let line = line.trim();
    let mut found = Vec::new();
    let boundaries: Vec<_> = line
        .char_indices()
        .filter(|(_, c)| c.is_whitespace())
        .map(|(index, _)| index)
        .chain(std::iter::once(line.len()))
        .collect();
    for ability in available(view).iter().filter(|a| !a.basic_attack) {
        let aliases = [
            ability.name.to_lowercase(),
            ability.id.to_lowercase(),
            ability.id.replace('_', " ").to_lowercase(),
            ability.name.to_lowercase().replace(' ', ""),
        ];
        let length = boundaries
            .iter()
            .copied()
            .filter(|&end| {
                aliases
                    .iter()
                    .any(|alias| line[..end].to_lowercase() == *alias)
            })
            .max();
        if let Some(length) = length {
            found.push((length, ability));
        }
    }
    found.sort_by_key(|(length, _)| std::cmp::Reverse(*length));
    match found.as_slice() {
        [] => Ok(None),
        [(length, ability), rest @ ..] => {
            if rest.iter().any(|(other, _)| other == length) {
                return Err("That ability name is ambiguous; use its ID.".into());
            }
            Ok(Some((ability, line.get(*length..).unwrap_or("").trim())))
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn arbitrary_disclosed_ability_matches_name_id_and_compact_name() {
        let mut view = crate::test_support::observation();
        let mut ability = crate::test_support::ability("shimmer_wave");
        ability.name = "Shimmer Wave".into();
        view.combat
            .as_mut()
            .unwrap()
            .own_stats
            .as_mut()
            .unwrap()
            .abilities
            .push(ability.clone());
        let before = view.clone();
        for line in [
            "shimmer wave sentinel",
            "shimmer_wave sentinel",
            "shimmerwave sentinel",
        ] {
            let (selected, target) = match_command(&view, line).unwrap().unwrap();
            assert_eq!(selected.id, "shimmer_wave");
            assert_eq!(target, "sentinel");
            assert_eq!(
                action(&view, &selected.id, view.visible_actors[0].id),
                Ok(Action::UseAbility {
                    id: "shimmer_wave".into(),
                    invocation: tor_protocol::AbilityInvocation::ActorTarget {
                        target: view.visible_actors[0].id
                    },
                    supplies: Default::default(),
                })
            );
        }
        assert_eq!(view, before);
        assert!(action(&view, "missing", view.visible_actors[0].id).is_err());
    }
    #[test]
    fn self_only_and_visible_actor_targets_are_distinct() {
        let mut view = crate::test_support::observation();
        let mut ability = crate::test_support::ability("mend");
        ability.reach = tor_protocol::AbilityReach::SelfOnly;
        ability.target_kind = AbilityTargetKind::SelfOnly;
        view.combat
            .as_mut()
            .unwrap()
            .own_stats
            .as_mut()
            .unwrap()
            .abilities
            .push(ability);
        assert!(action(&view, "mend", view.self_target).is_ok());
        assert!(action(&view, "mend", view.visible_actors[0].id).is_err());
        assert!(basic_action(&view, view.self_target).is_err());
    }
    #[test]
    fn phase5_self_or_ally_requires_an_explicit_disclosed_relation() {
        let mut view = crate::test_support::observation();
        let mut ability = crate::test_support::ability("mend");
        ability.reach = tor_protocol::AbilityReach::SelfOrAlly {
            range: tor_protocol::AbilityAllyRange::Touch,
        };
        ability.target_kind = AbilityTargetKind::SelfOrAlly;
        view.combat
            .as_mut()
            .unwrap()
            .own_stats
            .as_mut()
            .unwrap()
            .abilities
            .push(ability);
        let target = view.visible_actors[0].id;
        assert!(action(&view, "mend", view.self_target).is_ok());
        assert!(action(&view, "mend", target).is_err());
        view.combat
            .as_mut()
            .unwrap()
            .actors
            .push(tor_protocol::CombatActorView {
                actor: target,
                hostile: false,
                allied: false,
                injury: tor_protocol::Injury::Healthy,
            });
        assert!(action(&view, "mend", target).is_err());
        view.combat.as_mut().unwrap().actors[0].allied = true;
        assert!(action(&view, "mend", target).is_ok());
        view.visible_actors.clear();
        assert!(action(&view, "mend", target).is_err());
    }
    #[test]
    fn unicode_names_preserve_target_boundaries_and_ambiguous_names_require_id() {
        let mut view = crate::test_support::observation();
        let mut ability = crate::test_support::ability("glow");
        ability.name = "İşık".into();
        view.combat
            .as_mut()
            .unwrap()
            .own_stats
            .as_mut()
            .unwrap()
            .abilities
            .push(ability.clone());
        assert_eq!(
            match_command(&view, "İşık sentinel").unwrap().unwrap().1,
            "sentinel"
        );
        ability.id = "other_glow".into();
        view.combat
            .as_mut()
            .unwrap()
            .own_stats
            .as_mut()
            .unwrap()
            .abilities
            .push(ability);
        assert!(match_command(&view, "İşık sentinel").is_err());
        assert_eq!(
            match_command(&view, "glow sentinel").unwrap().unwrap().0.id,
            "glow"
        );
    }
}

#[cfg(test)]
mod supply_tests {
    use super::*;
    use tor_protocol::*;

    fn fixture() -> Observation {
        let mut view = crate::test_support::observation();
        let mut ability = crate::test_support::ability("mend");
        ability.reach = AbilityReach::SelfOnly;
        ability.target_kind = AbilityTargetKind::SelfOnly;
        ability.cooldown_remaining = 25;
        ability.supply_costs = vec![AbilitySupplyCost {
            supply: "bandage".into(),
            name: "Bandage".into(),
            quantity: 1,
            access: SupplyAccess::CarriedOrGroundTouch,
        }];
        view.combat
            .as_mut()
            .unwrap()
            .own_stats
            .as_mut()
            .unwrap()
            .abilities
            .push(ability);
        view.ground_items.push(GroundItemView {
            reachable: false,
            supply_reachable: true,
            position: Position { x: 1, y: 0, z: 0 },
            item: ItemView {
                id: ItemTarget::from_digest([3; 32]),
                name: "clean bandage".into(),
                description: String::new(),
                class: ItemClass::Misc,
                quantity: 1,
                appearance: "bandage".into(),
                identified: true,
                asset: None,
                known_supply: Some(SupplyView {
                    id: "bandage".into(),
                    name: "Bandage".into(),
                }),
            },
        });
        view
    }
    #[test]
    fn phase5_exact_ground_selection_uses_disclosed_touch_and_no_private_availability() {
        let mut view = fixture();
        let ability = choices(&view).pop().unwrap();
        let target = view.ground_items[0].item.id;
        let before = view.clone();
        assert!(matches!(
            action(&view, "mend", view.self_target).unwrap(),
            Action::UseAbility {
                supplies: SupplySelection::Carried,
                ..
            }
        ));
        let candidates = supply_candidates(&view, &ability);
        assert_eq!(candidates.len(), 1);
        assert!(!candidates[0].carried);
        let selection = selection_from_items(&view, &ability, &[target]).unwrap();
        assert_eq!(
            selection,
            SupplySelection::Explicit {
                items: vec![SelectedSupply {
                    cost_ordinal: 0,
                    item: target
                }]
            }
        );
        assert!(action_with_supplies(&view, "mend", view.self_target, selection).is_ok());
        assert_eq!(
            view, before,
            "cooldown and quantities are never spent locally"
        );
        view.ground_items[0].supply_reachable = false;
        assert!(supply_candidates(&view, &ability).is_empty());
        assert!(selection_from_items(&view, &ability, &[target]).is_err());
        view.ground_items[0].supply_reachable = true;
        view.ground_items[0].item.known_supply = None;
        assert!(supply_candidates(&view, &ability).is_empty());
    }
    #[test]
    fn phase5_supply_selection_rejects_duplicate_hidden_wrong_cost_and_missing_fragments() {
        let view = fixture();
        let ability = choices(&view).pop().unwrap();
        let target = view.ground_items[0].item.id;
        assert!(selection_from_items(&view, &ability, &[]).is_err());
        assert!(selection_from_items(&view, &ability, &[target, target]).is_err());
        assert!(
            selection_from_items(&view, &ability, &[ItemTarget::from_digest([9; 32])]).is_err()
        );
        let mut expensive = ability.clone();
        expensive.supply_costs[0].quantity = 2;
        assert!(selection_from_items(&view, &expensive, &[target]).is_err());
        let bad = SupplySelection::Explicit {
            items: vec![SelectedSupply {
                cost_ordinal: 1,
                item: target,
            }],
        };
        assert!(action_with_supplies(&view, "mend", view.self_target, bad).is_err());
        let mut carried = ability;
        carried.supply_costs[0].access = SupplyAccess::CarriedOnly;
        assert!(supply_candidates(&view, &carried).is_empty());
    }
    #[test]
    fn phase5_supply_text_uses_disclosed_names_or_opaque_handles_only() {
        let view = fixture();
        let ability = choices(&view).pop().unwrap();
        let item = view.ground_items[0].item.id;
        let expected = selection_from_items(&view, &ability, &[item]).unwrap();
        assert_eq!(
            selection_from_text(&view, &ability, "clean bandage").unwrap(),
            expected
        );
        assert_eq!(
            selection_from_text(&view, &ability, &format!("#{item}")).unwrap(),
            expected
        );
        assert_eq!(
            selection_from_text(&view, &ability, "carried").unwrap(),
            SupplySelection::Carried
        );
        assert!(selection_from_text(&view, &ability, "#3").is_err());
        assert_eq!(
            split_supply_clause("me using clean bandage").unwrap(),
            ("me", Some("clean bandage"))
        );
        assert!(split_supply_clause("me using ").is_err());
        assert!(ability_label(&ability).contains("25 active ticks"));
        assert!(ability_label(&ability).contains("1 Bandage"));
    }
    #[test]
    fn phase5_fragmented_costs_keep_exact_sources_and_canonical_ordinals() {
        let mut view = fixture();
        let mut ability = choices(&view).pop().unwrap();
        ability.supply_costs[0].quantity = 2;
        ability.supply_costs.push(AbilitySupplyCost {
            supply: "ration".into(),
            name: "Ration".into(),
            quantity: 1,
            access: SupplyAccess::CarriedOnly,
        });
        let ground = view.ground_items[0].item.id;
        let mut carried = view.ground_items[0].item.clone();
        carried.id = ItemTarget::from_digest([4; 32]);
        view.inventory.push(carried.clone());
        let bandage = carried.id;
        carried.id = ItemTarget::from_digest([5; 32]);
        carried.name = "travel ration".into();
        carried.known_supply = Some(SupplyView {
            id: "ration".into(),
            name: "Ration".into(),
        });
        let ration = carried.id;
        view.inventory.push(carried);
        // A second projected occurrence is not another physical stack.
        view.ground_items.push(view.ground_items[0].clone());
        assert_eq!(supply_candidates(&view, &ability).len(), 3);
        assert_eq!(
            selection_from_items(&view, &ability, &[ration, bandage, ground]).unwrap(),
            SupplySelection::Explicit {
                items: vec![
                    SelectedSupply {
                        cost_ordinal: 0,
                        item: ground
                    },
                    SelectedSupply {
                        cost_ordinal: 0,
                        item: bandage
                    },
                    SelectedSupply {
                        cost_ordinal: 1,
                        item: ration
                    }
                ]
            }
        );
    }

    #[test]
    fn phase5_supply_selection_rejects_more_rows_than_required_units() {
        let mut view = fixture();
        let ability = choices(&view).pop().unwrap();
        let first = view.ground_items[0].item.id;
        let mut second = view.ground_items[0].item.clone();
        second.id = ItemTarget::from_digest([4; 32]);
        let second_id = second.id;
        view.inventory.push(second);
        // Every explicit row must contribute at least one unit, regardless of
        // hidden holds or the backend's private exact-item ordering.
        assert!(selection_from_items(&view, &ability, &[first, second_id]).is_err());

        let mut two_units = ability;
        two_units.supply_costs[0].quantity = 2;
        view.ground_items[0].item.quantity = 3;
        view.inventory.last_mut().unwrap().quantity = 3;
        // Physical quantities cannot tell us whether either stack is partly
        // held. This possible two-row allocation remains server-authoritative.
        assert!(selection_from_items(&view, &two_units, &[first, second_id]).is_ok());
    }
    // Append to the existing client-common abilities tests module.
    #[test]
    fn typed_self_step_selection_uses_disclosed_metadata_and_shared_exact_supplies() {
        let mut view = crate::test_support::observation();
        let mut ability = crate::test_support::ability("unrelated_content_name");
        ability.reach = tor_protocol::AbilityReach::SelfOnly;
        ability.target_kind = AbilityTargetKind::SelfStep;
        view.combat
            .as_mut()
            .unwrap()
            .own_stats
            .as_mut()
            .unwrap()
            .abilities
            .push(ability.clone());
        let before = view.clone();
        for direction in [
            tor_protocol::Direction::North,
            tor_protocol::Direction::East,
            tor_protocol::Direction::South,
            tor_protocol::Direction::West,
            tor_protocol::Direction::NorthEast,
            tor_protocol::Direction::SouthEast,
            tor_protocol::Direction::SouthWest,
            tor_protocol::Direction::NorthWest,
            tor_protocol::Direction::Up,
            tor_protocol::Direction::Down,
        ] {
            let invocation = AbilityInvocation::SelfStep { direction };
            assert_eq!(
                invocation_action(&view, &ability.id, invocation, SupplySelection::Carried)
                    .unwrap(),
                Action::UseAbility {
                    id: ability.id.clone(),
                    invocation,
                    supplies: SupplySelection::Carried
                }
            );
        }
        assert!(action(&view, &ability.id, view.self_target).is_err());
        assert!(invocation_action(
            &view,
            "missing",
            AbilityInvocation::SelfStep {
                direction: tor_protocol::Direction::North
            },
            SupplySelection::Carried
        )
        .is_err());
        assert_eq!(view, before);
        let mut supplied = fixture();
        let supplied_ability = supplied
            .combat
            .as_mut()
            .unwrap()
            .own_stats
            .as_mut()
            .unwrap()
            .abilities
            .last_mut()
            .unwrap();
        supplied_ability.target_kind = AbilityTargetKind::SelfStep;
        let id = supplied_ability.id.clone();
        let supply = supplied.ground_items.last().unwrap().item.id;
        let selection =
            selection_from_items(&supplied, available(&supplied).last().unwrap(), &[supply])
                .unwrap();
        assert!(invocation_action(
            &supplied,
            &id,
            AbilityInvocation::SelfStep {
                direction: tor_protocol::Direction::West
            },
            selection
        )
        .is_ok());
    }
}
