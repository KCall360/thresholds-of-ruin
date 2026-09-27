//! Presentation facts derived exclusively from consecutive disclosed views.
use std::collections::BTreeMap;
use tor_protocol::*;

fn label(name: &str, fallback: &str) -> String {
    let name: String = name.chars().filter(|c| !c.is_control()).collect();
    if name.trim().is_empty() {
        fallback.into()
    } else {
        name
    }
}

/// Sight transitions describe knowledge, never movement, death, or hidden causes.
/// Duplicate portal views count once; self sightings never count as discoveries.
pub fn changes(before: &Observation, after: &Observation) -> Vec<String> {
    describe_changes(before, after, None)
}

/// Combine action results and sight changes without repeating a door action.
pub fn observation(
    before: &Observation,
    after: &Observation,
    event: Option<&Event>,
) -> Vec<String> {
    let door = match event {
        Some(Event::DoorChanged { door, .. }) => Some(*door),
        _ => None,
    };
    let mut lines = describe_changes(before, after, door);
    if let Some(event) = event {
        lines.insert(0, action(event, after));
    }
    lines
}

fn describe_changes(
    before: &Observation,
    after: &Observation,
    acted_door: Option<u64>,
) -> Vec<String> {
    let actors = |view: &Observation| -> BTreeMap<ActorId, String> {
        view.visible_actors
            .iter()
            .filter(|a| a.id != view.actor)
            .map(|a| (a.id, label(&a.name, "figure")))
            .collect()
    };
    let old = actors(before);
    let new = actors(after);
    let mut lines = Vec::new();
    for (id, name) in &new {
        if !old.contains_key(id) {
            let article = if name.to_lowercase().starts_with(['a', 'e', 'i', 'o', 'u']) {
                "an"
            } else {
                "a"
            };
            lines.push(format!("You notice {article} {name}."));
        }
    }
    for (id, name) in &old {
        if !new.contains_key(id) {
            lines.push(format!("You can no longer see the {name}."));
        }
    }
    let doors = |view: &Observation| -> BTreeMap<u64, (bool, String)> {
        view.visible_cells
            .iter()
            .filter_map(|c| c.door.as_ref())
            .map(|d| (d.id, (d.open, label(&d.name, "door"))))
            .collect()
    };
    let old = doors(before);
    for (id, (open, name)) in doors(after) {
        if Some(id) != acted_door && old.get(&id).is_some_and(|(was_open, _)| *was_open != open) {
            lines.push(format!(
                "The {name} is now {}.",
                if open { "open" } else { "closed" }
            ));
        }
    }
    if before.ready != after.ready {
        lines.push(
            if after.ready {
                "You can act again."
            } else {
                "You must wait."
            }
            .into(),
        );
    }
    lines
}

/// Ordinary action narration resolves names only from the resulting disclosed view.
pub fn action(event: &Event, view: &Observation) -> String {
    match event {
        Event::Moved { direction } => format!(
            "You move {}.",
            match direction {
                Direction::North => "north",
                Direction::East => "east",
                Direction::South => "south",
                Direction::West => "west",
                Direction::NorthEast => "northeast",
                Direction::SouthEast => "southeast",
                Direction::SouthWest => "southwest",
                Direction::NorthWest => "northwest",
                Direction::Up => "up",
                Direction::Down => "down",
            }
        ),
        Event::Taken {
            result, quantity, ..
        } => view.inventory.iter().find(|i| i.id == *result).map_or_else(
            || "You pick up the item.".into(),
            |i| {
                if *quantity == 1 {
                    format!("You pick up the {}.", label(&i.name, "item"))
                } else {
                    format!("You pick up {} x {}.", quantity, label(&i.name, "item"))
                }
            },
        ),
        Event::Dropped {
            result, quantity, ..
        } => view
            .ground_items
            .iter()
            .find(|i| i.item.id == *result)
            .map_or_else(
                || "You drop the item.".into(),
                |i| format!("You drop {} x {}.", quantity, label(&i.item.name, "item")),
            ),
        Event::DoorChanged { door, open } => {
            let name = view
                .visible_cells
                .iter()
                .filter_map(|c| c.door.as_ref())
                .find(|d| d.id == *door)
                .map_or_else(|| "door".into(), |d| label(&d.name, "door"));
            format!("You {} the {name}.", if *open { "open" } else { "close" })
        }
        Event::Waited => "Time passes.".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn observation() -> Observation {
        serde_json::from_value(serde_json::json!({
            "actor":1,"tick":0,"position":{"x":0,"y":0,"z":0},
            "visible_cells":[],"ground_items":[],"inventory":[],
            "visible_actors":[],"ready":true,"places":[]
        }))
        .unwrap()
    }

    #[test]
    fn sightings_are_deduplicated_and_never_claim_hidden_actions() {
        let before = observation();
        let mut after = before.clone();
        let actor = ActorView {
            id: ActorId(2),
            name: "figure".into(),
            description: String::new(),
            position: Position { x: 1, y: 0, z: 0 },
        };
        after.visible_actors = vec![actor.clone(), actor];
        assert_eq!(changes(&before, &after), ["You notice a figure."]);
        assert!(changes(&after, &after).is_empty());
        assert_eq!(
            changes(&after, &before),
            ["You can no longer see the figure."]
        );
        after.visible_actors[0].id = ActorId(1);
        after.visible_actors.pop();
        assert!(changes(&before, &after).is_empty());
    }

    #[test]
    fn doors_require_consecutive_sight_and_do_not_name_a_cause() {
        let mut before = observation();
        before.visible_cells.push(
            serde_json::from_value(serde_json::json!({
                "key":"seen","position":{"x":1,"y":0,"z":0},"wall":false,
                "floor":null,"ceiling":null,"material":"stone","place_hint":false,
                "stairs_up":false,"stairs_down":false,
                "door":{"id":3,"name":"iron gate","description":"","open":false,
                    "reachable":true,"approaches":[]}
            }))
            .unwrap(),
        );
        let mut after = before.clone();
        after.visible_cells[0].door.as_mut().unwrap().open = true;
        assert_eq!(changes(&before, &after), ["The iron gate is now open."]);
        assert!(changes(&observation(), &after).is_empty());
        assert!(changes(&after, &observation()).is_empty());
        assert_eq!(
            super::observation(
                &before,
                &after,
                Some(&Event::DoorChanged {
                    door: 3,
                    open: true
                })
            ),
            ["You open the iron gate."]
        );
    }

    #[test]
    fn action_names_are_disclosed_and_control_characters_are_removed() {
        let mut view = observation();
        assert_eq!(
            action(
                &Event::Taken {
                    item: 9,
                    result: 9,
                    quantity: 1
                },
                &view
            ),
            "You pick up the item."
        );
        view.inventory.push(ItemView {
            quantity: 1,
            appearance: String::new(),
            identified: true,
            id: 9,
            name: "copper\ntoken".into(),
            description: String::new(),
        });
        assert_eq!(
            action(
                &Event::Taken {
                    item: 9,
                    result: 9,
                    quantity: 1
                },
                &view
            ),
            "You pick up the coppertoken."
        );
        assert_eq!(
            action(
                &Event::DoorChanged {
                    door: 77,
                    open: false
                },
                &view
            ),
            "You close the door."
        );
    }

    #[test]
    fn readiness_changes_are_announced_without_inventing_an_unseen_action() {
        let before = observation();
        let mut waiting = before.clone();
        waiting.ready = false;
        assert_eq!(changes(&before, &waiting), ["You must wait."]);
        assert_eq!(changes(&waiting, &before), ["You can act again."]);
    }
}
