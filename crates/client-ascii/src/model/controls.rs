//! NetHack-shaped input aliases over existing disclosed TOR operations.
//! There is no client-side rules implementation or assumed ability grant here.
use super::*;

#[derive(Clone, Copy)]
pub(super) enum MovementPrefix {
    Run,
    MoveOnly,
    /// Consume the pending direction after an attachment/control interruption.
    Cancelled,
}

struct CommandChoice {
    name: &'static str,
    description: &'static str,
    key: Key,
}

macro_rules! commands {
    ($(($name:literal, $description:literal, $key:ident)),* $(,)?) => {
        const COMMANDS: &[CommandChoice] = &[$(CommandChoice {
            name: $name, description: $description, key: Key::$key,
        }),*];
    };
}

commands![
    ("abilities", "choose a disclosed ability (Z)", Abilities),
    ("apply", "use a disclosed portable light (a)", Apply),
    ("attack", "choose a visible attack target", Attack),
    (
        "advancement",
        "owner advancement / respec (F10)",
        Advancement
    ),
    (
        "cancel",
        "cancel eligible server work (F9)",
        CancelIntention
    ),
    ("charts", "read-only detection charts", Charts),
    ("control", "request actor control (F3)", Control),
    ("drop", "drop carried items (d)", Drop),
    ("eat", "eat a carried item (e)", Eat),
    ("equip", "equip any compatible item, including tools", Equip),
    (
        "escape-hold",
        "escape a disclosed linked hold (F11)",
        EscapeHold
    ),
    (
        "extinguish",
        "choose a disclosed light to extinguish",
        Extinguish
    ),
    ("faster", "show journeys more quickly", Faster),
    ("fire", "choose a disclosed Launch source (f)", Fire),
    ("help", "controls and map symbols (?)", Help),
    ("history", "read server game history (F2)", History),
    ("ignite", "choose a disclosed light to ignite", Ignite),
    ("inventory", "show carried items (i)", Inventory),
    ("look", "describe a map cell (; or /)", Look),
    ("messages", "read earlier messages (Ctrl-P)", MessageLog),
    ("note", "write a state-anchored note (F4)", Note),
    ("pickup", "pick up reachable floor items (,)", Pickup),
    (
        "places",
        "remembered places and naming (F5 / Ctrl-O)",
        Places
    ),
    ("puton", "equip a ring or amulet (P)", PutOn),
    ("quaff", "drink a carried item (q)", Drink),
    ("read", "read a carried item (r / F6)", Read),
    ("release", "release actor control", Release),
    (
        "remove",
        "remove any equipped item, including tools",
        Unequip
    ),
    (
        "resume",
        "resume eligible server work (F8)",
        ResumeIntention
    ),
    (
        "save",
        "save pending work and close this window (S)",
        SaveQuit
    ),
    ("search", "use your disclosed Search ability (s)", Search),
    ("slower", "show journeys more slowly", Slower),
    ("stats", "read personal creature stats (Ctrl-X)", Stats),
    ("takeoff", "remove armor (T)", TakeOff),
    ("throw", "choose a disclosed Throw source (t)", Throw),
    ("travel", "travel to a known destination (_)", Travel),
    ("wear", "equip armor (W)", Wear),
    ("wield", "equip a weapon (w)", Wield),
    ("wizard", "authenticated wizard command editor (F7)", Wizard),
    ("zap", "choose a disclosed device (z / F12)", Zap),
];

impl App {
    pub fn strike_detail_open(&self) -> bool {
        self.strike_detail_open
    }

    fn command_matches(&self) -> Vec<&'static CommandChoice> {
        let text = self
            .command_line
            .as_deref()
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase();
        COMMANDS
            .iter()
            .filter(|row| {
                row.name.contains(&text) || row.description.to_ascii_lowercase().contains(&text)
            })
            .collect()
    }

    /// Menu rows are client command descriptions, not promises of availability.
    pub fn command_rows(&self) -> Vec<String> {
        self.command_matches()
            .iter()
            .enumerate()
            .map(|(index, row)| {
                format!(
                    "{} #{:<12} {}",
                    if index == self.command_selected {
                        ">"
                    } else {
                        " "
                    },
                    row.name,
                    row.description
                )
            })
            .collect()
    }

    pub(super) fn command_input(&mut self, input: Input) -> Effect {
        match input {
            Input::Text { text } => {
                let line = self.command_line.as_mut().expect("open command editor");
                for ch in text.chars().filter(|ch| !ch.is_control()) {
                    if line.len() + ch.len_utf8() <= 80 {
                        line.push(ch);
                    }
                }
                self.command_selected = 0;
                self.command_selected_explicit = false;
            }
            Input::Key {
                key: Key::Backspace,
            } => {
                self.command_line
                    .as_mut()
                    .expect("open command editor")
                    .pop();
                self.command_selected = 0;
                self.command_selected_explicit = false;
            }
            Input::Key {
                key: Key::Escape | Key::Commands,
            } => {
                self.command_line = None;
                self.status = "Never mind.".into();
            }
            Input::Key { key: Key::Up } => {
                self.command_selected_explicit = true;
                self.command_selected = self.command_selected.saturating_sub(1);
            }
            Input::Key { key: Key::Down } => {
                self.command_selected_explicit = true;
                self.command_selected = self
                    .command_selected
                    .saturating_add(1)
                    .min(self.command_matches().len().saturating_sub(1));
            }
            Input::Key { key: Key::Enter } => {
                let text = self.command_line.as_deref().unwrap_or("").trim();
                let matches = self.command_matches();
                let choice = COMMANDS
                    .iter()
                    .find(|row| row.name.eq_ignore_ascii_case(text))
                    .or_else(|| {
                        (text.is_empty() || matches.len() == 1 || self.command_selected_explicit)
                            .then(|| matches.get(self.command_selected).copied())
                            .flatten()
                    });
                let Some(choice) = choice else {
                    self.status = if matches.len() > 1 {
                        "Command is ambiguous. Type its full name or choose with Up/Down."
                    } else {
                        "No supported command matches. Edit the name, or Esc cancels."
                    }
                    .into();
                    return Effect::None;
                };
                let key = choice.key;
                self.command_line = None;
                // Re-enter the ordinary boundary: spectators, disconnected clients,
                // message paging and readiness receive precisely the normal gates.
                return self.input(Input::Key { key });
            }
            _ => {}
        }
        Effect::None
    }

    pub(super) fn prefix_input(&mut self, input: Input) -> Effect {
        let prefix = self.movement_prefix.take().expect("active movement prefix");
        let Input::Key { key } = input else {
            self.status = "Direction cancelled.".into();
            return Effect::None;
        };
        if key == Key::Escape {
            self.status = "Never mind.".into();
            return Effect::None;
        }
        if !self.connected || self.busy || self.role == AccessRole::Spectator {
            self.status = "Movement is not available right now.".into();
            return Effect::None;
        }
        if self.messages.more() {
            self.messages.page();
            return Effect::None;
        }
        let Some(direction) = key.step_direction() else {
            self.status = "Expected a direction; prefix cancelled.".into();
            return Effect::None;
        };
        match prefix {
            MovementPrefix::Cancelled => {
                self.status =
                    "Direction prefix was cancelled by a connection/control change. Choose again."
                        .into();
                Effect::None
            }
            MovementPrefix::Run => {
                let run = match direction {
                    Direction::North => Key::RunUp,
                    Direction::South => Key::RunDown,
                    Direction::West => Key::RunLeft,
                    Direction::East => Key::RunRight,
                    Direction::NorthEast => Key::RunNorthEast,
                    Direction::NorthWest => Key::RunNorthWest,
                    Direction::SouthEast => Key::RunSouthEast,
                    Direction::SouthWest => Key::RunSouthWest,
                    Direction::Up | Direction::Down => {
                        self.status =
                            "Running uses a horizontal direction; use < or > for stairs.".into();
                        return Effect::None;
                    }
                };
                self.input(Input::Key { key: run })
            }
            MovementPrefix::MoveOnly => {
                // TOR has no automatic pickup. Suppress only the existing client
                // bump-to-attack conversion; the server still validates the step.
                let previous = self.bump_attacks;
                self.bump_attacks = BumpAttacks::Off;
                let effect = self.act(Action::Move { direction });
                self.bump_attacks = previous;
                effect
            }
        }
    }

    pub(super) fn named_ability(&mut self, name: &str, kind: AbilityTargetKind) -> Effect {
        let Some(state) = &self.state else {
            return Effect::None;
        };
        let ability =
            match tor_client_common::abilities::match_command(&state.state().observation, name) {
                Ok(Some((ability, ""))) if ability.target_kind == kind => ability.clone(),
                Ok(_) => {
                    self.status = format!(
                        "No supported {name} ability is disclosed. Press Z for your abilities."
                    );
                    return Effect::None;
                }
                Err(reason) => {
                    self.status = reason;
                    return Effect::None;
                }
            };
        self.selected_weapon = WeaponSelection::Default;
        self.choose_ability(ability)
    }

    /// One dispatch owner for menus and conventional direct ability shortcuts.
    pub(super) fn choose_ability(&mut self, ability: AbilityView) -> Effect {
        let Some(state) = &self.state else {
            return Effect::None;
        };
        if !self.connected || self.busy {
            return Effect::None;
        }
        if self.role == AccessRole::Spectator || !state.has_control() {
            self.status = "Ability use requires control of your character.".into();
            return Effect::None;
        }
        if !tor_client_common::abilities::choices(&state.state().observation).contains(&ability) {
            self.status = "That ability is no longer disclosed. Choose again.".into();
            return Effect::None;
        }
        if ability.target_kind == AbilityTargetKind::DirectionalProjectile {
            self.selected_weapon = WeaponSelection::Default;
            return self.open_projectile_picker(&ability.id);
        }
        if matches!(
            ability.target_kind,
            AbilityTargetKind::SelfStep | AbilityTargetKind::DirectionalRay
        ) {
            self.status = format!(
                "Choose a direction for {}. Movement keys select; {}Esc cancels.",
                ability.name,
                if ability.weapon_selection {
                    "Tab chooses a weapon; "
                } else {
                    ""
                }
            );
            self.selected_ability = Some(ability);
            return Effect::None;
        }
        if ability.target_kind == AbilityTargetKind::SelfOnly {
            let target = state.state().observation.self_target;
            return self.choose_ability_target(ability, target);
        }
        self.select_targets(Some(ability));
        Effect::None
    }

    pub(super) fn open_item_picker(
        &mut self,
        operation: ItemOperation,
        filter: Option<Key>,
    ) -> Effect {
        let Some(state) = &self.state else {
            return Effect::None;
        };
        if !state.has_control() {
            self.status = "You are observing. Press F3 to request control.".into();
            return Effect::None;
        }
        let view = &state.state().observation;
        let mut seen = std::collections::BTreeSet::new();
        let items: Vec<_> = item_choices(view, operation)
            .filter(|item| equipment_matches(item.class, filter) && seen.insert(item.id))
            .cloned()
            .collect();
        self.quantity.clear();
        self.applying = false;
        self.dropping = false;
        if items.is_empty() {
            self.status = if view.interactions.is_none() && !view.inventory.is_empty() {
                format!(
                    "Nothing you carry can be used to {} here.",
                    operation.verb()
                )
            } else {
                format!(
                    "You don't have anything to {}{}.",
                    operation.verb(),
                    match filter {
                        Some(Key::Wield) => " in the weapon category",
                        Some(Key::Wear | Key::TakeOff) => " in the armor category",
                        Some(Key::PutOn | Key::RemoveAccessory) => " in the accessory category",
                        _ => "",
                    }
                )
            };
        } else {
            self.pickup = items;
            self.item_operation = Some(operation);
            self.selected = 0;
            self.status = format!(
                "What do you want to {}? Type its letter; Esc cancels.",
                operation.verb()
            );
        }
        Effect::None
    }

    pub(super) fn open_apply_picker(&mut self) -> Effect {
        let Some(state) = &self.state else {
            return Effect::None;
        };
        if !state.has_control() {
            self.status = "You are observing. Press F3 to request control.".into();
            return Effect::None;
        }
        let view = &state.state().observation;
        self.pickup = view
            .inventory
            .iter()
            .filter(|item| light_operation(view, item.id).is_some())
            .cloned()
            .collect();
        self.item_operation = None;
        self.dropping = false;
        self.quantity.clear();
        self.selected = 0;
        self.applying = !self.pickup.is_empty();
        self.status = if self.applying {
            "Apply which light? Type its letter; Enter uses its current disclosed operation; Esc cancels."
        } else {
            "No usable portable light is disclosed. Apply supports portable lights only."
        }.into();
        Effect::None
    }
}

fn equipment_matches(class: ItemClass, filter: Option<Key>) -> bool {
    match filter {
        Some(Key::Wield) => class == ItemClass::Weapon,
        Some(Key::Wear | Key::TakeOff) => class == ItemClass::Armor,
        Some(Key::PutOn | Key::RemoveAccessory) => {
            matches!(class, ItemClass::Ring | ItemClass::Amulet)
        }
        _ => true,
    }
}

fn light_operation(view: &Observation, item: ItemTarget) -> Option<ItemOperation> {
    let light = view
        .interactions
        .as_ref()?
        .inventory
        .iter()
        .find(|row| row.item == item)?
        .portable_light
        .as_ref()?;
    match (light.ignitable, light.extinguishable) {
        (true, false) => Some(ItemOperation::Ignite),
        (false, true) => Some(ItemOperation::Extinguish),
        _ => None,
    }
}

pub(super) fn apply_action(view: &Observation, item: ItemTarget) -> Result<Action, String> {
    let operation =
        light_operation(view, item).ok_or("That light has no current usable operation.")?;
    item_action(view, item, operation)
}
