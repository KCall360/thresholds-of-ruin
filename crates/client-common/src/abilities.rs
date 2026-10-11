//! Dynamic ability affordances from the server-disclosed personal catalog.
use tor_protocol::{AbilityTargetKind, AbilityView, Action, ActorTarget, Observation};
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
    let ability = available(view)
        .iter()
        .find(|ability| ability.id == id)
        .ok_or_else(|| format!("You don't have {id}."))?;
    let valid = match ability.target_kind {
        AbilityTargetKind::SelfOnly => target == view.self_target,
        AbilityTargetKind::Actor => {
            target != view.self_target && view.visible_actors.iter().any(|actor| actor.id == target)
        }
    };
    if !valid {
        return Err("That creature is not a valid visible target.".into());
    }
    Ok(Action::UseAbility {
        id: id.into(),
        target,
    })
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
                    target: view.visible_actors[0].id
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
