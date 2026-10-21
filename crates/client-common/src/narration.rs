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

/// What a narrated line reports, so a frontend can choose which lines to show
/// and merge related ones. The text stays the same whichever it picks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Topic {
    /// The observer's own action, as its history event records it.
    Own(Event),
    /// Another actor came into sight.
    Noticed(ActorTarget),
    /// Another actor left sight.
    LostSight(ActorTarget),
    /// A door changed state without the observer acting on it.
    Door,
    /// A newly disclosed entrance, trap presence, or trap family.
    Discovery,
    /// Involuntary motion or a collision.
    Motion,
    /// The observer finished timed item work.
    Finished,
    /// A disclosed combat event.
    Combat(CombatEventView),
    /// A newly paid historical observation, owned by this observer.
    PublicPlan,
    /// Hit points with preparation, recovery, victory or death.
    Status,
    /// Readiness changed: the observer can act again, or must wait.
    Pacing,
}

/// One narrated sentence and what it reports.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    pub topic: Topic,
    pub text: String,
    /// Optional compact presentation for an observer-owned basic attack.
    /// The ordinary text and underlying event remain unchanged.
    pub basic_attack: Option<BasicAttack>,
}

/// Basic-attack facts derived from the observer's disclosed ability catalog,
/// never from an ability's name or an undisclosed opponent capability.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BasicAttack {
    Preparing,
    Resolved {
        /// An already-disclosed object phrase, such as "the ruin scout".
        target: String,
        outcome: AbilityOutcome,
    },
}

impl Line {
    fn new(topic: Topic, text: impl Into<String>) -> Self {
        Self {
            topic,
            text: text.into(),
            basic_attack: None,
        }
    }

    fn with_basic_attack(mut self, before: &Observation, after: &Observation) -> Self {
        // Prefer a current disclosed capability over an older one. The prior
        // view still covers a strike that removes a grant or ends the run.
        let basic = |ability: &ContentRef| {
            crate::abilities::available(after)
                .iter()
                .chain(crate::abilities::available(before))
                .find(|candidate| candidate.id == ability.id)
                .is_some_and(|candidate| candidate.basic_attack)
        };
        self.basic_attack = match &self.topic {
            Topic::Own(Event::AbilityStarted { ability, .. }) if basic(ability) => {
                Some(BasicAttack::Preparing)
            }
            Topic::Combat(CombatEventView::Ability {
                caster: Some(caster),
                target,
                ability,
                outcome,
                ..
            }) if *caster == after.self_target && basic(ability) => {
                let target = match target {
                    Some(actor) if *actor == after.self_target => "yourself".into(),
                    Some(actor) => format!("the {}", actor_name(*actor, before, after)),
                    None => "something".into(),
                };
                Some(BasicAttack::Resolved {
                    target,
                    outcome: *outcome,
                })
            }
            _ => None,
        };
        self
    }
}

/// Sight transitions describe knowledge, never movement, death, or hidden causes.
/// Duplicate portal views count once; self sightings never count as discoveries.
pub fn changes(before: &Observation, after: &Observation) -> Vec<String> {
    describe_changes(before, after, None)
        .into_iter()
        .map(|line| line.text)
        .collect()
}

/// Narrate the observer's completed item work from disclosed inventories only.
pub fn completed_items(before: &Observation, after: &Observation) -> Vec<String> {
    let Some(interactions) = &after.interactions else {
        return Vec::new();
    };
    if before.tick == after.tick
        && before.interactions.as_ref().is_some_and(|old| {
            old.completed == interactions.completed
                && old.curse_blocked == interactions.curse_blocked
        })
    {
        return Vec::new();
    }
    interactions
        .completed
        .iter()
        .filter_map(|action| {
            let (verb, item) = match action {
                tor_protocol::Action::Equip { item, .. } => ("equipping", item),
                tor_protocol::Action::Unequip { item } => ("removing", item),
                tor_protocol::Action::Drink { item } => ("drinking", item),
                tor_protocol::Action::Eat { item } => ("eating", item),
                tor_protocol::Action::Read { item } => ("reading", item),
                tor_protocol::Action::Zap { item, .. } => ("zapping", item),
                _ => return None,
            };
            let name = before
                .inventory
                .iter()
                .chain(&after.inventory)
                .find(|candidate| candidate.id == *item)
                .map_or_else(|| "item".into(), |candidate| label(&candidate.name, "item"));
            Some(format!("You finish {verb} the {name}."))
        })
        .chain(interactions.curse_blocked.iter().map(|item| {
            let name = before
                .inventory
                .iter()
                .chain(&after.inventory)
                .find(|candidate| candidate.id == *item)
                .map_or_else(|| "item".into(), |candidate| label(&candidate.name, "item"));
            format!("You cannot remove the {name}; it is cursed.")
        }))
        .collect()
}

/// Combine action results and sight changes without repeating a door action.
pub fn observation(
    before: &Observation,
    after: &Observation,
    event: Option<&Event>,
) -> Vec<String> {
    observation_lines(before, after, event)
        .into_iter()
        .map(|line| line.text)
        .collect()
}

/// [`observation`], with what each line reports.
pub fn observation_lines(
    before: &Observation,
    after: &Observation,
    event: Option<&Event>,
) -> Vec<Line> {
    let door = match event {
        Some(Event::DoorChanged { door, .. }) => Some(*door),
        _ => None,
    };
    let mut lines = describe_changes(before, after, door);
    if let Some(event) = event {
        lines.insert(
            0,
            Line::new(Topic::Own(event.clone()), action(event, after))
                .with_basic_attack(before, after),
        );
    }
    lines.extend(
        completed_items(before, after)
            .into_iter()
            .map(|text| Line::new(Topic::Finished, text)),
    );
    if let Some(combat) = &after.combat {
        if let Some(text) = crate::public_plan::changed_line(
            before
                .combat
                .as_ref()
                .and_then(|value| value.public_plan.as_ref()),
            combat.public_plan.as_ref(),
        ) {
            lines.push(Line::new(Topic::PublicPlan, text));
        }
        if before.tick != after.tick || before.combat != after.combat {
            lines.extend(combat.events.iter().map(|event| {
                Line::new(
                    Topic::Combat(event.clone()),
                    combat_event(event, before, after),
                )
                .with_basic_attack(before, after)
            }));
            if before.combat.as_ref().is_none_or(|c| {
                c.hp != combat.hp
                    || c.victory != combat.victory
                    || c.dead != combat.dead
                    || c.sequence != combat.sequence
                    || (combat.sequence.is_some()
                        && c.preparation_active != combat.preparation_active)
            }) {
                lines.push(Line::new(Topic::Status, combat_status(combat)));
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
        CombatEventView::UnseenAbility {
            target, outcome, ..
        } => {
            let object = object(Some(*target));
            if *outcome == AbilityOutcome::Miss {
                format!("Something attacks {object}, but misses.")
            } else {
                format!("Something attacks {object}.")
            }
        }
        CombatEventView::AnonymousAbility {
            caster,
            target,
            outcome,
        } => {
            let (subject, object) = (subject(*caster), object(*target));
            match outcome {
                AbilityOutcome::Applied | AbilityOutcome::Unaffected => {
                    format!("{subject} used an unidentified effect on {object}.")
                }
                AbilityOutcome::Miss => {
                    format!("{subject} used an unidentified effect on {object}, but missed.")
                }
            }
        }
        CombatEventView::Ability {
            caster,
            target,
            ability,
            outcome,
            strike,
        } => {
            let (subject, object) = (subject(*caster), object(*target));
            let mut name = label(&ability.name, "ability");
            if let Some(ordinal) = strike.filter(|ordinal| *ordinal > 0) {
                name.push_str(&format!(" (strike {})", u32::from(ordinal) + 1));
            }
            match outcome {
                // This marker describes the primary, not every possible rider.
                // Uniform neutral wording does not disclose hidden rider state.
                AbilityOutcome::Applied | AbilityOutcome::Unaffected => {
                    format!("{subject} used {name} on {object}.")
                }
                AbilityOutcome::Miss => format!("{subject} used {name} on {object}, but missed."),
            }
        }
        CombatEventView::EscapeResolved { success, .. } => {
            if *success {
                "You escape one hold.".into()
            } else {
                "You fail to escape the hold.".into()
            }
        }
        CombatEventView::Condition { condition } => {
            format!("You are affected by {}.", condition.name)
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
        ObjectiveKind::RetrieveAndExit => {
            "Retrieve the objective item, then carry it up through the surface exit."
        }
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
        if !c.terminal {
            if let Some(sequence) = sequence_status(c) {
                text.push_str(" — ");
                text.push_str(&sequence);
            }
        }
    } else if let Some(sequence) = sequence_status(c) {
        text.push_str(" — ");
        text.push_str(&sequence);
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

/// Exact active-time progress and cancellation commitment for the observed actor.
/// This contains no action permissions; frontends offer controls from readiness.
pub fn sequence_status(c: &CombatView) -> Option<String> {
    let sequence = c.sequence.as_ref()?;
    let phase = if sequence.completed == 0 {
        format!(
            "Attack preparation: {} ticks remaining{}. {}-strike sequence.",
            sequence.next_strike_remaining,
            if c.preparation_active {
                ""
            } else {
                " (interrupted)"
            },
            sequence.total
        )
    } else if c.preparation_active {
        format!(
            "Strike {} of {} complete.",
            sequence.completed, sequence.total
        )
    } else {
        format!(
            "Sequence paused after strike {} of {}.",
            sequence.completed, sequence.total
        )
    };
    Some(format!(
        "{phase} Next strike in {} ticks. Cancel leaves {} ticks of recovery.",
        sequence.next_strike_remaining, sequence.cancel_recovery_remaining
    ))
}

pub fn discovery_changes(before: &Observation, after: &Observation) -> Vec<String> {
    let mut lines = Vec::new();
    let traps = |view: &Observation| -> BTreeMap<TrapTarget, Option<TrapFamily>> {
        view.visible_cells
            .iter()
            .filter_map(|cell| cell.trap.as_ref())
            .map(|trap| (trap.id, trap.family))
            .collect()
    };
    let old = traps(before);
    for (id, family) in traps(after) {
        match (old.get(&id), family) {
            (None, None) => lines.push("You notice a trap.".into()),
            (None | Some(None), Some(family)) => {
                lines.push(format!("You recognize a {}.", family.name()))
            }
            _ => {}
        }
    }
    let old: BTreeMap<_, _> = before
        .visible_cells
        .iter()
        .map(|cell| (&cell.key, cell))
        .collect();
    let mut doors = std::collections::BTreeSet::new();
    for cell in &after.visible_cells {
        if let Some(door) = &cell.door {
            if old
                .get(&cell.key)
                .is_some_and(|old| old.wall && old.door.is_none())
                && doors.insert(door.id)
            {
                lines.push("You notice a door.".into());
            }
        }
    }
    lines
}

fn describe_changes(
    before: &Observation,
    after: &Observation,
    acted_door: Option<DoorTarget>,
) -> Vec<Line> {
    let actors = |view: &Observation| -> BTreeMap<ActorTarget, String> {
        view.visible_actors
            .iter()
            .filter(|a| a.id != view.self_target)
            .map(|a| (a.id, label(&a.name, "figure")))
            .collect()
    };
    let old = actors(before);
    let new = actors(after);
    let mut lines: Vec<_> = discovery_changes(before, after)
        .into_iter()
        .map(|text| Line::new(Topic::Discovery, text))
        .collect();
    if after.tick != before.tick {
        if let Some(motion) = &after.motion {
            if motion.displaced {
                lines.push(Line::new(Topic::Motion, "You move involuntarily."));
            }
            if motion.impacted {
                lines.push(Line::new(Topic::Motion, "You collide with an obstruction."));
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
            lines.push(Line::new(
                Topic::Noticed(*id),
                format!("You notice {article} {name}."),
            ));
        }
    }
    for (id, name) in &old {
        if !new.contains_key(id) {
            lines.push(Line::new(
                Topic::LostSight(*id),
                format!("You can no longer see the {name}."),
            ));
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
            lines.push(Line::new(
                Topic::Door,
                format!(
                    "The {name} is now {}.",
                    if open { "open" } else { "closed" }
                ),
            ));
        }
    }
    if before.ready != after.ready && after.combat.as_ref().is_none_or(|c| !c.terminal) {
        lines.push(Line::new(
            Topic::Pacing,
            if after.ready {
                "You can act again."
            } else {
                "You must wait."
            },
        ));
    }
    lines
}

/// Ordinary action narration resolves names only from the resulting disclosed view.
pub fn action(event: &Event, view: &Observation) -> String {
    match event {
        Event::EscapeStarted { .. } => "You begin trying to escape a hold.".into(),
        Event::Escaped { item } => {
            let name = view
                .inventory
                .iter()
                .find(|candidate| candidate.id == *item)
                .map_or_else(
                    || "objective item".into(),
                    |candidate| label(&candidate.name, "objective item"),
                );
            format!("You leave the dungeon with the {name}.")
        }
        Event::ItemStarted { action } => {
            let (verb, item) = match action {
                tor_protocol::Action::Equip { item, .. } => ("equip", item),
                tor_protocol::Action::Unequip { item } => ("remove", item),
                tor_protocol::Action::Drink { item } => ("drink", item),
                tor_protocol::Action::Eat { item } => ("eat", item),
                tor_protocol::Action::Read { item } => ("read", item),
                tor_protocol::Action::Zap { item, .. } => ("zap", item),
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
        Event::Crumbled { quantity, .. } => {
            if *quantity == 1 {
                "The item crumbles as you try to pick it up.".into()
            } else {
                format!("{quantity} items crumble as you try to pick them up.")
            }
        }
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
        Event::AbilityStarted { ability, .. } => {
            format!("You prepare to use {}.", label(&ability.name, "ability"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn observation() -> Observation {
        Observation {
            self_appearance: None,
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
            linked_holds: Vec::new(),
            visible_actors: Vec::new(),
            ready: true,
        }
    }

    #[test]
    fn sightings_are_deduplicated_and_never_claim_hidden_actions() {
        let before = observation();
        let mut after = before.clone();
        let actor = ActorView {
            appearance_seed: None,
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
    fn paid_curse_failure_is_narrated_without_success_or_hidden_item_name() {
        let before = observation();
        let mut after = before.clone();
        after.tick += 300;
        after.interactions = Some(tor_protocol::InteractionView {
            identification: None,
            completed: vec![],
            curse_blocked: vec![tor_protocol::ItemTarget::from_digest([19; 32])],
            slots: vec![],
            preparation: None,
            inventory: vec![],
        });
        assert_eq!(
            completed_items(&before, &after),
            ["You cannot remove the item; it is cursed."]
        );
        assert!(completed_items(&after, &after).is_empty());
    }

    #[test]
    fn item_preparation_names_only_disclosed_items_and_reports_no_effect_early() {
        let mut view = observation();
        let item = tor_protocol::ItemTarget::from_digest([3; 32]);
        view.inventory.push(tor_protocol::ItemView {
            corpse: None,
            class: tor_protocol::ItemClass::Potion,
            quantity: 2,
            appearance: "red potion".into(),
            identified: false,
            description: String::new(),
            id: item,
            name: "red potion".into(),
            asset: None,
            known_currency: None,
            known_supply: None,
        });
        let event = Event::ItemStarted {
            action: tor_protocol::Action::Drink { item },
        };
        assert_eq!(action(&event, &view), "You begin to drink the red potion.");
        let carried = view.clone();
        view.inventory.clear();
        assert_eq!(action(&event, &view), "You begin to drink the item.");
        let before = view.clone();
        view.interactions = Some(tor_protocol::InteractionView {
            curse_blocked: vec![],
            identification: None,
            completed: vec![tor_protocol::Action::Drink { item }],
            slots: vec![],
            preparation: None,
            inventory: vec![],
        });
        view.tick = 100;
        assert_eq!(
            completed_items(&before, &view),
            ["You finish drinking the item."]
        );
        assert!(completed_items(&view, &view).is_empty());
        assert_eq!(
            completed_items(&carried, &view),
            ["You finish drinking the red potion."]
        );
    }

    #[test]
    fn doors_require_consecutive_sight_and_do_not_name_a_cause() {
        let mut before = observation();
        before.visible_cells.push(CellView {
            lit: true,
            key: "seen".into(),
            position: Position { x: 1, y: 0, z: 0 },
            wall: false,
            material: "stone".into(),
            place_hint: false,
            stairs_up: false,
            stairs_down: false,
            asset: None,
            trap: None,
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
    fn intentional_retrieval_exit_names_only_the_disclosed_carried_item() {
        assert_eq!(
            objective(ObjectiveKind::RetrieveAndExit),
            "Retrieve the objective item, then carry it up through the surface exit."
        );
        let mut view = observation();
        let item = ItemTarget::from_digest([7; 32]);
        let event = Event::Escaped { item };
        assert_eq!(
            action(&event, &view),
            "You leave the dungeon with the objective item."
        );
        view.inventory.push(ItemView {
            corpse: None,
            class: Default::default(),
            asset: None,
            quantity: 1,
            appearance: String::new(),
            identified: true,
            id: item,
            name: "Amulet\nof Yendor".into(),
            description: String::new(),
            known_currency: None,
            known_supply: None,
        });
        assert_eq!(
            action(&event, &view),
            "You leave the dungeon with the Amuletof Yendor."
        );
        view.inventory[0].id = ItemTarget::from_digest([8; 32]);
        assert_eq!(
            action(&event, &view),
            "You leave the dungeon with the objective item."
        );
    }

    #[test]
    fn crumbled_pickup_never_claims_inventory_transfer_or_invents_identity() {
        let view = observation();
        for (quantity, expected) in [
            (1, "The item crumbles as you try to pick it up."),
            (2, "2 items crumble as you try to pick them up."),
        ] {
            assert_eq!(
                action(
                    &Event::Crumbled {
                        item: tor_protocol::ItemTarget::from_digest([9; 32]),
                        quantity
                    },
                    &view
                ),
                expected
            );
        }
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
            corpse: None,
            class: Default::default(),
            asset: None,
            quantity: 1,
            appearance: String::new(),
            identified: true,
            id: tor_protocol::ItemTarget::from_digest([9; 32]),
            name: "copper\ntoken".into(),
            description: String::new(),
            known_currency: None,
            known_supply: None,
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
    fn ability_lines_describe_only_disclosed_participants_and_qualitative_results() {
        let view = observation();
        assert_eq!(
            action(
                &Event::AbilityStarted {
                    ability: crate::test_support::content("fear"),
                    target: view.self_target
                },
                &view
            ),
            "You prepare to use Fear."
        );
        assert_eq!(
            combat_event(
                &CombatEventView::Ability {
                    strike: None,
                    caster: None,
                    target: Some(view.self_target),
                    ability: crate::test_support::content("fear"),
                    outcome: AbilityOutcome::Applied,
                },
                &view,
                &view
            ),
            "Something used Fear on you."
        );
        assert_eq!(
            combat_event(
                &CombatEventView::Ability {
                    strike: None,
                    caster: Some(view.self_target),
                    target: None,
                    ability: crate::test_support::content("power_strike"),
                    outcome: AbilityOutcome::Unaffected,
                },
                &view,
                &view
            ),
            "You used Power Strike on something."
        );
        assert_eq!(
            combat_event(
                &CombatEventView::Ability {
                    strike: None,
                    caster: Some(view.self_target),
                    target: None,
                    ability: crate::test_support::content("magic_bolt"),
                    outcome: AbilityOutcome::Miss,
                },
                &view,
                &view
            ),
            "You used Magic Bolt on something, but missed."
        );
    }

    #[test]
    fn combat_lines_are_written_from_events_and_disclosed_names() {
        let mut view = observation();
        view.visible_actors.push(ActorView {
            appearance_seed: None,
            asset: None,
            id: tor_protocol::ActorTarget::from_digest([2; 32]),
            name: "ruin scout".into(),
            description: String::new(),
            position: Position { x: 1, y: 0, z: 0 },
        });
        let after = observation();
        let line = |event| combat_event(&event, &view, &after);
        assert_eq!(
            line(CombatEventView::Ability {
                strike: None,
                ability: crate::test_support::content("basic_melee"),
                caster: Some(tor_protocol::ActorTarget::from_digest([1; 32])),
                target: Some(tor_protocol::ActorTarget::from_digest([2; 32])),
                outcome: AbilityOutcome::Applied,
            }),
            "You used Basic Melee on the ruin scout."
        );
        assert_eq!(
            line(CombatEventView::Ability {
                strike: None,
                ability: crate::test_support::content("basic_melee"),
                caster: None,
                target: Some(tor_protocol::ActorTarget::from_digest([1; 32])),
                outcome: AbilityOutcome::Miss,
            }),
            "Something used Basic Melee on you, but missed."
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
                public_plan: None,
                sequence: None,
                own_stats: None,
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

    #[test]
    fn every_line_says_what_it_reports_and_matches_the_plain_text() {
        let scout = tor_protocol::ActorTarget::from_digest([2; 32]);
        let me = tor_protocol::ActorTarget::from_digest([1; 32]);
        let mut before = observation();
        before.ready = false;
        before.visible_actors.push(ActorView {
            appearance_seed: None,
            asset: None,
            id: scout,
            name: "ruin scout".into(),
            description: String::new(),
            position: Position { x: 1, y: 0, z: 0 },
        });
        let mut after = observation();
        let hit = CombatEventView::Ability {
            strike: None,
            ability: crate::test_support::content("basic_melee"),
            caster: Some(me),
            target: Some(scout),
            outcome: AbilityOutcome::Applied,
        };
        after.combat = Some(CombatView {
            public_plan: None,
            sequence: None,
            own_stats: None,
            hp: 28,
            max_hp: 30,
            preparation_remaining: None,
            preparation_active: false,
            recovery_remaining: 0,
            actors: vec![],
            events: vec![hit.clone(), CombatEventView::Died { actor: scout }],
            objective: None,
            exit: None,
            victory: false,
            dead: false,
            terminal: false,
        });
        let event = Event::Moved {
            direction: Direction::East,
        };
        let lines = observation_lines(&before, &after, Some(&event));
        let topics: Vec<_> = lines.iter().map(|line| line.topic.clone()).collect();
        assert_eq!(
            topics,
            [
                Topic::Own(event.clone()),
                Topic::LostSight(scout),
                Topic::Pacing,
                Topic::Combat(hit),
                Topic::Combat(CombatEventView::Died { actor: scout }),
                Topic::Status,
            ]
        );
        assert_eq!(
            lines
                .iter()
                .map(|line| line.text.clone())
                .collect::<Vec<_>>(),
            super::observation(&before, &after, Some(&event))
        );
        assert_eq!(lines[3].text, "You used Basic Melee on the ruin scout.");
    }

    #[test]
    fn compact_basic_attacks_require_the_current_disclosed_flag() {
        let mut before = crate::test_support::observation();
        let granted = &mut before
            .combat
            .as_mut()
            .unwrap()
            .own_stats
            .as_mut()
            .unwrap()
            .abilities[0];
        granted.id = "custom.attack".into();
        granted.name = "Disclosed attack".into();
        let ability = granted.content();
        let target = before.visible_actors[0].id;
        let start = Event::AbilityStarted {
            ability: ability.clone(),
            target,
        };
        let event = CombatEventView::Ability {
            caster: Some(before.self_target),
            target: Some(target),
            ability,
            outcome: AbilityOutcome::Applied,
            strike: None,
        };
        for (current_flag, expected) in [(true, true), (false, false)] {
            let mut after = before.clone();
            after.tick += 1;
            let combat = after.combat.as_mut().unwrap();
            combat.own_stats.as_mut().unwrap().abilities[0].basic_attack = current_flag;
            combat.events = vec![event.clone()];
            let lines = observation_lines(&before, &after, Some(&start));
            assert_eq!(lines.len(), 2);
            assert_eq!(lines[0].text, "You prepare to use Disclosed attack.");
            assert_eq!(lines[1].text, "You used Disclosed attack on the sentinel.");
            assert_eq!(
                lines[0].basic_attack,
                expected.then_some(BasicAttack::Preparing)
            );
            assert_eq!(
                lines[1].basic_attack,
                expected.then_some(BasicAttack::Resolved {
                    target: "the sentinel".into(),
                    outcome: AbilityOutcome::Applied,
                })
            );
        }
    }

    #[test]
    fn unaffected_primary_keeps_neutral_narration_with_visible_or_concealed_riders() {
        // The ordinary event deliberately contains no rider outcome. A fully
        // reduced primary must use the same wording for a visible push, a blocked
        // push, no rider, or a condition rider whose result is not disclosed.
        for (ability, displaced) in [
            ("dislocating_strike", true),
            ("dislocating_strike", false),
            ("basic_melee", false),
            ("rattling_blow", false),
        ] {
            let before = crate::test_support::observation();
            let mut after = before.clone();
            after.tick = 110;
            after.motion = Some(MotionView {
                velocity: [0, 0, 0],
                units_per_cell: 100,
                displaced,
                impacted: false,
            });
            if displaced {
                after.position.x = 1;
            }
            let event = CombatEventView::Ability {
                strike: None,
                caster: Some(ActorTarget::from_digest([2; 32])),
                target: Some(after.self_target),
                ability: crate::test_support::content(ability),
                outcome: AbilityOutcome::Unaffected,
            };
            after.combat.as_mut().unwrap().events = vec![event.clone()];
            let expected = format!(
                "The sentinel used {} on you.",
                crate::test_support::content(ability).name
            );
            assert_eq!(combat_event(&event, &before, &after), expected);
            let lines = observation_lines(&before, &after, None);
            assert!(lines
                .iter()
                .any(|line| line.topic == Topic::Combat(event.clone()) && line.text == expected));
            assert_eq!(
                lines
                    .iter()
                    .filter(|line| line.topic == Topic::Motion)
                    .count(),
                usize::from(displaced)
            );
            assert!(!lines.iter().any(|line| line.text.contains("no effect")));
            assert!(matches!(
                &after.combat.as_ref().unwrap().events[0],
                CombatEventView::Ability {
                    outcome: AbilityOutcome::Unaffected,
                    ..
                }
            ));
        }
    }

    #[test]
    fn anonymous_effect_feedback_stays_neutral_and_has_no_basic_attack_identity() {
        let before = crate::test_support::observation();
        let mut after = before.clone();
        let visible = after.visible_actors[0].id;
        for (caster, target, phrase) in [
            (
                Some(after.self_target),
                Some(visible),
                "You used an unidentified effect on the sentinel",
            ),
            (
                Some(visible),
                Some(after.self_target),
                "The sentinel used an unidentified effect on you",
            ),
            (
                None,
                Some(after.self_target),
                "Something used an unidentified effect on you",
            ),
            (
                Some(after.self_target),
                None,
                "You used an unidentified effect on something",
            ),
        ] {
            for outcome in [
                AbilityOutcome::Applied,
                AbilityOutcome::Unaffected,
                AbilityOutcome::Miss,
            ] {
                let event = CombatEventView::AnonymousAbility {
                    caster,
                    target,
                    outcome,
                };
                after.combat.as_mut().unwrap().events = vec![event.clone()];
                let expected = if outcome == AbilityOutcome::Miss {
                    format!("{phrase}, but missed.")
                } else {
                    format!("{phrase}.")
                };
                assert_eq!(combat_event(&event, &before, &after), expected);
                let lines = observation_lines(&before, &after, None);
                let line = lines
                    .iter()
                    .find(|line| line.topic == Topic::Combat(event.clone()))
                    .unwrap();
                assert_eq!(line.text, expected);
                assert_eq!(line.basic_attack, None);
                assert!(!line.text.contains("attacks"));
                assert!(!line.text.contains("wand"));
                assert!(!line.text.contains("spell"));
                assert!(!line.text.contains("no effect"));
            }
        }
        let event = CombatEventView::AnonymousAbility {
            caster: None,
            target: Some(after.self_target),
            outcome: AbilityOutcome::Applied,
        };
        after.visible_actors[0].name = "later identified figure".into();
        assert_eq!(
            combat_event(&event, &before, &after),
            "Something used an unidentified effect on you."
        );
    }

    #[test]
    fn unseen_attack_history_never_acquires_a_later_visible_name() {
        let before = crate::test_support::observation();
        let mut after = before.clone();
        after.visible_actors[0].name = "identified phantom".into();
        for caster in [None, Some(after.visible_actors[0].id)] {
            for (outcome, expected) in [
                (AbilityOutcome::Applied, "Something attacks you."),
                (AbilityOutcome::Unaffected, "Something attacks you."),
                (AbilityOutcome::Miss, "Something attacks you, but misses."),
            ] {
                let event = CombatEventView::UnseenAbility {
                    caster,
                    target: after.self_target,
                    outcome,
                };
                after.combat.as_mut().unwrap().events = vec![event.clone()];
                assert_eq!(combat_event(&event, &before, &after), expected);
                let lines = observation_lines(&before, &after, None);
                assert!(lines.iter().any(|line| {
                    line.topic == Topic::Combat(event.clone()) && line.text == expected
                }));
            }
        }
    }
}
