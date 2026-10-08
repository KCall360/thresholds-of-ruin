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
    if let Some(combat) = &after.combat {
        if before.tick != after.tick || before.combat != after.combat {
            lines.extend(
                combat
                    .events
                    .iter()
                    .map(|event| combat_event(event, before, after)),
            );
            if before.combat.as_ref().is_none_or(|c| {
                c.hp != combat.hp || c.victory != combat.victory || c.dead != combat.dead
            }) {
                lines.push(combat_status(combat));
            }
        }
    }
    lines
}

/// A disclosed actor's name from either view, for an actor that may have
/// just died or moved out of sight.
fn actor_name(actor: ActorTarget, before: &Observation, after: &Observation) -> String {
    after
        .visible_actors
        .iter()
        .chain(&before.visible_actors)
        .find(|a| a.id == actor && !a.name.trim().is_empty())
        .map_or_else(|| "figure".into(), |a| label(&a.name, "figure"))
}

/// One line for a combat event, written from the observer's point of view.
pub fn combat_event(event: &CombatEventView, before: &Observation, after: &Observation) -> String {
    let observer = after.self_target;
    let subject = |actor: Option<ActorTarget>| match actor {
        Some(id) if id == observer => "You".to_owned(),
        Some(id) => format!("The {}", actor_name(id, before, after)),
        None => "Something".into(),
    };
    let object = |actor: Option<ActorTarget>| match actor {
        Some(id) if id == observer => "you".to_owned(),
        Some(id) => format!("the {}", actor_name(id, before, after)),
        None => "something".into(),
    };
    match event {
        CombatEventView::Attack {
            attacker,
            target,
            outcome,
        } => {
            let (subject, object) = (subject(*attacker), object(*target));
            match outcome {
                AttackOutcome::Miss => format!("{subject} missed {object}."),
                AttackOutcome::NoInjury => {
                    format!("{subject} struck {object}, but caused no injury.")
                }
                AttackOutcome::Hit => format!("{subject} struck {object}."),
            }
        }
        CombatEventView::Interrupted { .. } => "Your attack was interrupted.".into(),
        CombatEventView::Died { actor } if *actor == observer => "You died.".into(),
        CombatEventView::Died { actor } => format!("{} died.", subject(Some(*actor))),
    }
}

/// The objective as a sentence.
pub fn objective(kind: ObjectiveKind) -> &'static str {
    match kind {
        ObjectiveKind::RetrieveAndReturn => "Retrieve the objective item and return to the exit.",
        ObjectiveKind::ReachExit => "Reach the exit.",
    }
}

/// An injury level as an adjective phrase.
pub fn injury(injury: Injury) -> &'static str {
    match injury {
        Injury::Healthy => "healthy",
        Injury::Wounded => "wounded",
        Injury::BadlyWounded => "badly wounded",
        Injury::NearDeath => "near death",
    }
}

pub fn combat_status(c: &CombatView) -> String {
    let mut text = format!("HP {}/{}", c.hp, c.max_hp);
    if c.dead {
        text.push_str(" — You died. This run has ended.");
    } else if c.victory {
        text.push_str(if c.terminal {
            " — Victory! This run has ended."
        } else {
            " — Victory! You may continue exploring."
        });
    } else if let Some(remaining) = c.preparation_remaining {
        text.push_str(&format!(
            " — Attack preparation: {remaining} ticks remaining{}.",
            if c.preparation_active {
                ""
            } else {
                " (interrupted)"
            }
        ));
    } else if c.recovery_remaining > 0 {
        text.push_str(&format!(" — Recovering: {} ticks.", c.recovery_remaining));
    }
    text
}

fn describe_changes(
    before: &Observation,
    after: &Observation,
    acted_door: Option<DoorTarget>,
) -> Vec<String> {
    let actors = |view: &Observation| -> BTreeMap<ActorTarget, String> {
        view.visible_actors
            .iter()
            .filter(|a| a.id != view.self_target)
            .map(|a| (a.id, label(&a.name, "figure")))
            .collect()
    };
    let old = actors(before);
    let new = actors(after);
    let mut lines = Vec::new();
    if after.tick != before.tick {
        if let Some(motion) = &after.motion {
            if motion.displaced {
                lines.push("You move involuntarily.".into());
            }
            if motion.impacted {
                lines.push("You collide with an obstruction.".into());
            }
        }
    }
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
    let doors = |view: &Observation| -> BTreeMap<DoorTarget, (bool, String)> {
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
    if before.ready != after.ready && after.combat.as_ref().is_none_or(|c| !c.terminal) {
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
        Event::ItemStarted { action } => {
            let (verb, item) = match action {
                tor_protocol::Action::Equip { item, .. } => ("equip", item),
                tor_protocol::Action::Unequip { item } => ("remove", item),
                tor_protocol::Action::Drink { item } => ("drink", item),
                _ => return "You begin preparing.".into(),
            };
            let name = view
                .inventory
                .iter()
                .find(|candidate| candidate.id == *item)
                .map_or_else(|| "item".into(), |candidate| label(&candidate.name, "item"));
            format!("You begin to {verb} the {name}.")
        }

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
        Event::PreparationPaused => "Your preparation is paused until you act again.".into(),
        Event::AttackStarted { .. } => "You prepare to attack.".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn observation() -> Observation {
        Observation {
            interactions: None,
            combat: None,
            motion: None,
            places: Vec::new(),
            actor: ActorId(1),
            self_target: tor_protocol::ActorTarget::from_digest([1; 32]),
            tick: 0,
            position: Position { x: 0, y: 0, z: 0 },
            visible_cells: Vec::new(),
            ground_items: Vec::new(),
            inventory: Vec::new(),
            visible_actors: Vec::new(),
            ready: true,
        }
    }

    #[test]
    fn sightings_are_deduplicated_and_never_claim_hidden_actions() {
        let before = observation();
        let mut after = before.clone();
        let actor = ActorView {
            asset: None,
            id: tor_protocol::ActorTarget::from_digest([2; 32]),
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
        after.visible_actors[0].id = after.self_target;
        after.visible_actors.pop();
        assert!(changes(&before, &after).is_empty());
    }

    #[test]
    fn item_preparation_names_only_disclosed_items_and_reports_no_effect_early() {
        let mut view = observation();
        let item = tor_protocol::ItemTarget::from_digest([3; 32]);
        view.inventory.push(tor_protocol::ItemView {
            class: tor_protocol::ItemClass::Potion,
            quantity: 2,
            appearance: "red potion".into(),
            identified: false,
            description: String::new(),
            id: item,
            name: "red potion".into(),
            asset: None,
        });
        let event = Event::ItemStarted {
            action: tor_protocol::Action::Drink { item },
        };
        assert_eq!(action(&event, &view), "You begin to drink the red potion.");
        view.inventory.clear();
        assert_eq!(action(&event, &view), "You begin to drink the item.");
    }

    #[test]
    fn doors_require_consecutive_sight_and_do_not_name_a_cause() {
        let mut before = observation();
        before.visible_cells.push(CellView {
            key: "seen".into(),
            position: Position { x: 1, y: 0, z: 0 },
            wall: false,
            material: "stone".into(),
            place_hint: false,
            stairs_up: false,
            stairs_down: false,
            asset: None,
            door: Some(DoorView {
                id: tor_protocol::DoorTarget::from_digest([3; 32]),
                name: "iron gate".into(),
                description: String::new(),
                open: false,
                reachable: true,
                approaches: Vec::new(),
                asset: None,
            }),
        });
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
                    door: tor_protocol::DoorTarget::from_digest([3; 32]),
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
                    item: tor_protocol::ItemTarget::from_digest([9; 32]),
                    result: tor_protocol::ItemTarget::from_digest([9; 32]),
                    quantity: 1
                },
                &view
            ),
            "You pick up the item."
        );
        view.inventory.push(ItemView {
            class: Default::default(),
            asset: None,
            quantity: 1,
            appearance: String::new(),
            identified: true,
            id: tor_protocol::ItemTarget::from_digest([9; 32]),
            name: "copper\ntoken".into(),
            description: String::new(),
        });
        assert_eq!(
            action(
                &Event::Taken {
                    item: tor_protocol::ItemTarget::from_digest([9; 32]),
                    result: tor_protocol::ItemTarget::from_digest([9; 32]),
                    quantity: 1
                },
                &view
            ),
            "You pick up the coppertoken."
        );
        assert_eq!(
            action(
                &Event::DoorChanged {
                    door: tor_protocol::DoorTarget::from_digest([77; 32]),
                    open: false
                },
                &view
            ),
            "You close the door."
        );
    }

    #[test]
    fn combat_lines_are_written_from_events_and_disclosed_names() {
        let mut view = observation();
        view.visible_actors.push(ActorView {
            asset: None,
            id: tor_protocol::ActorTarget::from_digest([2; 32]),
            name: "ruin scout".into(),
            description: String::new(),
            position: Position { x: 1, y: 0, z: 0 },
        });
        let after = observation();
        let line = |event| combat_event(&event, &view, &after);
        assert_eq!(
            line(CombatEventView::Attack {
                attacker: Some(tor_protocol::ActorTarget::from_digest([1; 32])),
                target: Some(tor_protocol::ActorTarget::from_digest([2; 32])),
                outcome: AttackOutcome::Hit,
            }),
            "You struck the ruin scout."
        );
        assert_eq!(
            line(CombatEventView::Attack {
                attacker: None,
                target: Some(tor_protocol::ActorTarget::from_digest([1; 32])),
                outcome: AttackOutcome::Miss,
            }),
            "Something missed you."
        );
        assert_eq!(
            line(CombatEventView::Died {
                actor: tor_protocol::ActorTarget::from_digest([2; 32])
            }),
            "The ruin scout died."
        );
        assert_eq!(objective(ObjectiveKind::ReachExit), "Reach the exit.");
        assert_eq!(injury(Injury::BadlyWounded), "badly wounded");
    }

    #[test]
    fn readiness_changes_are_announced_without_inventing_an_unseen_action() {
        let before = observation();
        let mut waiting = before.clone();
        waiting.ready = false;
        assert_eq!(changes(&before, &waiting), ["You must wait."]);
        assert_eq!(changes(&waiting, &before), ["You can act again."]);
    }
    #[test]
    fn terminal_outcomes_do_not_instruct_the_player_to_wait() {
        let before = observation();
        for dead in [false, true] {
            let mut after = before.clone();
            after.ready = false;
            after.combat = Some(CombatView {
                hp: if dead { 0 } else { 30 },
                max_hp: 30,
                preparation_remaining: None,
                preparation_active: false,
                recovery_remaining: 0,
                actors: vec![],
                events: vec![],
                objective: None,
                exit: None,
                victory: !dead,
                dead,
                terminal: true,
            });
            let text = super::observation(&before, &after, None).join(" ");
            assert!(!text.contains("must wait"));
            assert!(text.contains(if dead { "You died" } else { "Victory" }));
        }
    }
}
