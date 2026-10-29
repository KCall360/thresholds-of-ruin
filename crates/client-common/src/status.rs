//! The compact status every client can show, from disclosed state only:
//! attributes and place, then health, resource pools, gold, hunger, time,
//! holds and the character's current combat state.
use crate::ClientState;
use tor_protocol::*;

fn place_name(o: &Observation) -> Option<&str> {
    o.places
        .iter()
        .filter_map(|place| {
            o.visible_cells
                .iter()
                .filter(|c| c.key == place.key)
                .map(|c| {
                    let d = c.position;
                    (d.x - o.position.x).pow(2) + (d.y - o.position.y).pow(2)
                })
                .min()
                .map(|distance| (distance, place.name.as_str()))
        })
        .min()
        .map(|(_, name)| name)
}

fn sequence_label(combat: &CombatView) -> Option<String> {
    let sequence = combat.sequence.as_ref()?;
    Some(format!(
        "{}:{}/{} Next:{} Cancel:{}",
        if combat.preparation_active {
            "Sequence"
        } else {
            "Sequence paused"
        },
        sequence.completed,
        sequence.total,
        sequence.next_strike_remaining,
        sequence.cancel_recovery_remaining
    ))
}

/// Two rows of status fields separated by two spaces. Client activities,
/// such as a run, follow travel and precede the wizard marker.
pub fn lines(state: &ClientState, activities: &[&str]) -> [String; 2] {
    let o = &state.state().observation;
    let me = o
        .visible_actors
        .iter()
        .find(|a| a.id == o.self_target && !a.name.trim().is_empty())
        .map(|a| a.name.as_str());
    let mut first = Vec::new();
    if let Some(stats) = o
        .combat
        .as_ref()
        .and_then(|combat| combat.own_stats.as_ref())
    {
        let a = &stats.attributes;
        first.push(format!(
            "Str:{} Spd:{} Int:{} Wil:{} Awr:{} Pre:{} HD:{}",
            a.strength,
            a.speed,
            a.intellect,
            a.willpower,
            a.awareness,
            a.presence,
            stats.hit_dice.total,
        ));
    }
    if let Some(place) = place_name(o).or(me) {
        first.push(place.to_owned());
    }
    let mut first = first.join("  ");
    if let Some(objective) = o.combat.as_ref().and_then(|c| c.objective) {
        if !first.is_empty() {
            first.push_str("   ");
        }
        first.push_str(crate::narration::objective(objective));
    }
    let mut second = Vec::new();
    if let Some(combat) = &o.combat {
        second.push(format!("HP:{}({})", combat.hp, combat.max_hp));
        if let Some(stats) = &combat.own_stats {
            for resource in &stats.resources {
                let label = match resource.resource.id.as_str() {
                    "stamina" => "Stm",
                    "focus" => "Foc",
                    "mana" => "Mana",
                    _ => resource.resource.name.as_str(),
                };
                let mut pool = format!("{label}:{}/{}", resource.balance, resource.maximum);
                if resource.reserved > 0 {
                    pool.push_str(&format!("[-{}]", resource.reserved));
                }
                second.push(pool);
            }
        }
    }
    // Currency is a disclosed typed capability, never inferred from appearance.
    // Sum in u128: the bounded inventory can contain several full u64 stacks.
    let gold: u128 = o
        .inventory
        .iter()
        .filter(|item| item.known_currency == Some(CurrencyKind::Gold))
        .map(|item| u128::from(item.quantity))
        .sum();
    second.push(format!("Gold:{gold}"));
    if let Some(NutritionView::Required { hunger, .. }) = o
        .combat
        .as_ref()
        .and_then(|combat| combat.own_stats.as_ref())
        .map(|stats| stats.nutrition)
    {
        second.push(crate::stats::hunger_label(hunger).into());
    }
    second.push(format!("T:{}", o.tick));
    if !o.linked_holds.is_empty() {
        second.push(format!("Held:{}", o.linked_holds.len()));
    }
    if let Some(combat) = &o.combat {
        if combat.dead {
            second.push("Dead".into());
        } else if combat.victory {
            second.push("Victorious".into());
            if !combat.terminal {
                second.extend(sequence_label(combat));
            }
        } else if let Some(sequence) = sequence_label(combat) {
            second.push(sequence);
        } else if combat.preparation_remaining.is_some() {
            second.push(
                if combat.preparation_active {
                    "Attacking"
                } else {
                    "Attack paused"
                }
                .into(),
            );
        } else if combat.recovery_remaining > 0 {
            second.push("Recovering".into());
        }
    }
    if state
        .travel()
        .is_some_and(|t| t.phase == TravelPhase::Active)
    {
        second.push("Travelling".into());
    }
    second.extend(activities.iter().map(|activity| (*activity).to_string()));
    if state.state().wizard_game {
        second.push("Wizard".into());
    }
    [first, second.join("  ")]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(observation: Observation, wizard: bool) -> ClientState {
        let snapshot: Snapshot = serde_json::from_value(serde_json::json!({
            "readiness": {"revision": "0", "admission": true, "resume": [], "cancel": []},
            "context": StreamContext { stream: StreamId("status".into()), epoch: 0 },
            "actor": observation.actor, "branch": "play",
            "cursor": {"sequence": "0", "tick": observation.tick.to_string()},
            "has_control": true, "intentions": [], "history": {"entries": [], "older_before": null},
            "state": {"snapshot": "status-picture", "wizard_game": wizard, "revision": "0", "observation": observation},
        }))
        .unwrap();
        ClientState::from_snapshot(snapshot).unwrap()
    }

    #[test]
    fn rows_show_health_gold_and_time_with_activities_before_the_wizard_marker() {
        let mut observation = crate::test_support::observation();
        observation.tick = 42;
        let [first, second] = lines(&state(observation.clone(), false), &["Running"]);
        assert!(first.starts_with("Str:"), "{first}");
        assert_eq!(second, "HP:12(12)  Gold:0  T:42  Running");
        let [_, second] = lines(&state(observation, true), &["Running"]);
        assert_eq!(second, "HP:12(12)  Gold:0  T:42  Running  Wizard");
    }
}
