//! Explicit transport-to-backend command boundary.
//! Decode preserves requested facts; it grants no authority and resolves no
//! live targets. The session checks authority and durable receipts before fresh
//! input validation. Fresh resolution produces admission; execution stays simulation-owned.
//! Journal command/action types do not implement conversion methods or developer parsing.
use crate::actions as a;
use crate::journal::Command;
use tor_protocol::{self as p, EntryId};
use tor_simulation as s;
#[path = "wire_targets.rs"]
mod targets;
pub use targets::TargetScope;

/// Resolve only entities present in the observer's immutable disclosure.
pub fn decode_action(
    action: &p::Action,
    scope: &TargetScope,
    observation: &s::Observation,
) -> Result<a::Action, crate::Failure> {
    let unavailable = || crate::Failure::new(p::ErrorCode::InvalidAction, "Target is unavailable");
    if scope.observer() != observation.actor {
        return Err(unavailable());
    }
    Ok(match action {
        p::Action::Escape { hold } => a::Action::Escape {
            hold: observation
                .linked_holds
                .iter()
                .find(|offer| scope.hold(offer.id) == *hold)
                .ok_or_else(unavailable)?
                .id,
        },
        p::Action::ChooseGrowthSource { basis, source } => a::Action::ChooseGrowthSource {
            basis: Box::new(crate::advancement_adapter::basis(basis)?),
            source: crate::advancement_adapter::growth_source(source)?,
        },
        p::Action::ChooseIntellectExpertise { basis, choice } => {
            a::Action::ChooseIntellectExpertise {
                basis: Box::new(crate::advancement_adapter::basis(basis)?),
                choice: crate::advancement_adapter::intellect_expertise(choice)?,
            }
        }
        p::Action::Purchase { basis, purchase } => a::Action::Purchase {
            basis: Box::new(crate::advancement_adapter::basis(basis)?),
            purchase: crate::advancement_adapter::purchase(purchase)?,
        },
        p::Action::Respec {
            basis,
            replacement,
            supplies,
        } => a::Action::Respec {
            basis: Box::new(crate::advancement_adapter::basis(basis)?),
            replacement: crate::advancement_adapter::replacement(replacement)?,
            supplies: decode_supplies(supplies, scope, observation)?,
        },
        p::Action::UseAbility {
            id,
            invocation,
            supplies,
        } => {
            let id = s::content_ids::AbilityId::new(id).map_err(|_| unavailable())?;
            if matches!(invocation.aim, p::AbilityAim::DirectionalProjectile { .. })
                && invocation.weapon != p::WeaponSelection::Default
            {
                return Err(unavailable());
            }
            let aim = match &invocation.aim {
                p::AbilityAim::ActorTarget { target } => {
                    let (identity, reference) = std::iter::once((
                        observation.actor,
                        s::perception::ActorReferenceKind::Creature,
                    ))
                    .chain(observation.visible_actors.iter().filter_map(|actor| {
                        actor.disclosure.reference().map(|kind| (actor.id, kind))
                    }))
                    .find(|(actor, kind)| scope.actor_reference(*actor, *kind) == *target)
                    .ok_or_else(unavailable)?;
                    match reference {
                        s::perception::ActorReferenceKind::Creature => {
                            a::AbilityAim::ActorTarget { target: identity }
                        }
                        s::perception::ActorReferenceKind::Contact => {
                            a::AbilityAim::ContactTarget { target: identity }
                        }
                    }
                }
                p::AbilityAim::DirectionalProjectile {
                    direction,
                    delivery,
                } => a::AbilityAim::DirectionalProjectile {
                    direction: crate::adapt::requested_direction(*direction),
                    delivery: decode_projectile_delivery(*delivery, scope, observation)?,
                },
                p::AbilityAim::SelfStep { direction } => a::AbilityAim::SelfStep {
                    direction: crate::adapt::requested_direction(*direction),
                },
                p::AbilityAim::DirectionalRay { direction } => a::AbilityAim::DirectionalRay {
                    direction: crate::adapt::requested_direction(*direction),
                },
            };
            let weapon = match invocation.weapon {
                p::WeaponSelection::Default => a::WeaponSelection::Default,
                p::WeaponSelection::Equipped { item, socket } => {
                    if usize::from(socket) >= p::MAX_EQUIPMENT_SLOTS {
                        return Err(unavailable());
                    }
                    let identity = observation
                        .inventory
                        .iter()
                        .find(|candidate| scope.item(candidate.id) == item)
                        .ok_or_else(unavailable)?
                        .id;
                    let equipped = observation.interactions.as_ref().and_then(|interactions| {
                        interactions.inventory.iter().find(|offer| {
                            offer.item == identity
                                && offer.equipped_slot == Some(s::EquipmentSlotId(socket))
                        })
                    });
                    if equipped.is_none() {
                        return Err(unavailable());
                    }
                    a::WeaponSelection::Equipped {
                        item: identity.0,
                        socket,
                    }
                }
            };
            let invocation = a::AbilityInvocation { aim, weapon };
            let supplies = decode_supplies(supplies, scope, observation)?;
            a::Action::UseAbility {
                id,
                invocation,
                supplies,
            }
        }
        p::Action::ExamineTrap { trap } => {
            let identity = observation
                .visible_cells
                .iter()
                .filter_map(|cell| cell.trap.as_ref())
                .find(|candidate| scope.trap(candidate.id) == *trap)
                .ok_or_else(unavailable)?
                .id;
            a::Action::ExamineTrap { trap: identity }
        }
        p::Action::SetDoor { door, open } => {
            let identity = observation
                .visible_cells
                .iter()
                .filter_map(|cell| cell.door.as_ref())
                .find(|candidate| scope.door(candidate.id) == *door)
                .ok_or_else(unavailable)?
                .id;
            a::Action::SetDoor {
                door: identity,
                open: *open,
            }
        }
        p::Action::Move { direction } => a::Action::Move {
            direction: crate::adapt::requested_direction(*direction),
        },
        p::Action::Take { item, quantity } => {
            if let Some(candidate) = observation
                .ground_items
                .iter()
                .find(|candidate| scope.item(candidate.id) == *item)
            {
                a::Action::Take {
                    item: candidate.id.0,
                    quantity: *quantity,
                }
            } else {
                let object = observation
                    .apparent_objects
                    .iter()
                    .find(|candidate| scope.apparent_object(candidate.subject) == *item)
                    .ok_or_else(unavailable)?;
                a::Action::TouchApparentObject {
                    subject: object.subject,
                    quantity: *quantity,
                }
            }
        }
        p::Action::Drop { item, quantity } => {
            let identity = observation
                .inventory
                .iter()
                .find(|candidate| scope.item(candidate.id) == *item)
                .ok_or_else(unavailable)?
                .id
                .0;
            a::Action::Drop {
                item: identity,
                quantity: *quantity,
            }
        }
        p::Action::Equip { item, .. }
        | p::Action::Unequip { item }
        | p::Action::Ignite { item }
        | p::Action::Extinguish { item }
        | p::Action::Drink { item }
        | p::Action::Eat { item }
        | p::Action::Read { item }
        | p::Action::Zap { item, .. } => {
            let identity = observation
                .inventory
                .iter()
                .find(|candidate| scope.item(candidate.id) == *item)
                .ok_or_else(unavailable)?
                .id
                .0;
            match action {
                p::Action::Equip { slot, .. } => a::Action::Equip {
                    item: identity,
                    slot: *slot,
                },
                p::Action::Unequip { .. } => a::Action::Unequip { item: identity },
                p::Action::Ignite { .. } => a::Action::Ignite { item: identity },
                p::Action::Extinguish { .. } => a::Action::Extinguish { item: identity },
                p::Action::Drink { .. } => a::Action::Drink { item: identity },
                p::Action::Eat { .. } => a::Action::Eat { item: identity },
                p::Action::Read { .. } => a::Action::Read { item: identity },
                p::Action::Zap { aim, .. } => a::Action::Zap {
                    item: identity,
                    aim: crate::adapt::requested_device_aim(*aim),
                },
                _ => unreachable!(),
            }
        }
        p::Action::Identify { choice, item } => {
            let pending = observation
                .interactions
                .as_ref()
                .and_then(|interaction| interaction.identification.as_ref())
                .filter(|pending| scope.identification_choice(pending.choice) == *choice)
                .ok_or_else(unavailable)?;
            let item = item
                .map(|token| {
                    observation
                        .inventory
                        .iter()
                        .find(|candidate| {
                            scope.item(candidate.id) == token
                                && pending.classes.contains(&candidate.class)
                                && pending.items.contains(&candidate.id)
                        })
                        .map(|candidate| candidate.id.0)
                        .ok_or_else(unavailable)
                })
                .transpose()?;
            a::Action::Identify {
                choice: pending.choice,
                item,
            }
        }
        p::Action::Wait => a::Action::Wait,
    })
}

fn decode_projectile_delivery(
    delivery: p::ProjectileDeliverySelection,
    scope: &TargetScope,
    observation: &s::Observation,
) -> Result<a::ProjectileDeliverySelection, crate::Failure> {
    let unavailable = || crate::Failure::new(p::ErrorCode::InvalidAction, "Target is unavailable");
    let interactions = observation.interactions.as_ref().ok_or_else(unavailable)?;
    let owned_item = |token: p::ItemTarget| {
        observation
            .inventory
            .iter()
            .find(|item| scope.item(item.id) == token)
            .map(|item| item.id)
            .ok_or_else(unavailable)
    };
    let owned = |token: p::ItemTarget| {
        let item = owned_item(token)?;
        let offer = interactions
            .inventory
            .iter()
            .find(|offer| offer.item == item)
            .ok_or_else(unavailable)?;
        Ok::<_, crate::Failure>((item.0, offer.equipped_slot))
    };
    let socket_exists = |socket: u16| {
        usize::from(socket) < p::MAX_EQUIPMENT_SLOTS
            && usize::from(socket) < interactions.slots.len()
    };
    let equipped = |token: p::ItemTarget, socket: u16| {
        let (item, equipped) = owned(token)?;
        if !socket_exists(socket) || equipped != Some(s::EquipmentSlotId(socket)) {
            return Err(unavailable());
        }
        Ok(item)
    };
    let carried = |token: p::ItemTarget| {
        let (item, equipped) = owned(token)?;
        if equipped.is_some() {
            return Err(unavailable());
        }
        Ok(item)
    };
    let carried_ammunition = |token: p::ItemTarget| {
        let item = owned_item(token)?;
        // Plain ammunition has no equipment/use interaction row. The complete
        // inventory still discloses its owner-scoped identity, and any equipped
        // row would make this carried-only operand unavailable.
        if interactions
            .inventory
            .iter()
            .any(|offer| offer.item == item && offer.equipped_slot.is_some())
        {
            return Err(unavailable());
        }
        Ok(item.0)
    };
    Ok(match delivery {
        p::ProjectileDeliverySelection::Throw { source } => a::ProjectileDeliverySelection::Throw {
            source: match source {
                p::ThrownSourceSelection::Equipped { item, socket } => {
                    a::ThrownSourceSelection::Equipped {
                        item: equipped(item, socket)?,
                        socket,
                    }
                }
                p::ThrownSourceSelection::Carried { item, socket } => {
                    if !socket_exists(socket) {
                        return Err(unavailable());
                    }
                    a::ThrownSourceSelection::Carried {
                        item: carried(item)?,
                        socket,
                    }
                }
            },
        },
        p::ProjectileDeliverySelection::Launch {
            launcher,
            ammunition,
        } => {
            if launcher.item == ammunition {
                return Err(unavailable());
            }
            a::ProjectileDeliverySelection::Launch {
                launcher: a::EquippedProjectileWeapon {
                    item: equipped(launcher.item, launcher.socket)?,
                    socket: launcher.socket,
                },
                ammunition: carried_ammunition(ammunition)?,
            }
        }
    })
}

fn encode_projectile_delivery(
    delivery: a::ProjectileDeliverySelection,
    scope: &TargetScope,
) -> p::ProjectileDeliverySelection {
    match delivery {
        a::ProjectileDeliverySelection::Throw { source } => p::ProjectileDeliverySelection::Throw {
            source: match source {
                a::ThrownSourceSelection::Equipped { item, socket } => {
                    p::ThrownSourceSelection::Equipped {
                        item: scope.item(s::ItemId(item)),
                        socket,
                    }
                }
                a::ThrownSourceSelection::Carried { item, socket } => {
                    p::ThrownSourceSelection::Carried {
                        item: scope.item(s::ItemId(item)),
                        socket,
                    }
                }
            },
        },
        a::ProjectileDeliverySelection::Launch {
            launcher,
            ammunition,
        } => p::ProjectileDeliverySelection::Launch {
            launcher: p::EquippedProjectileWeapon {
                item: scope.item(s::ItemId(launcher.item)),
                socket: launcher.socket,
            },
            ammunition: scope.item(s::ItemId(ammunition)),
        },
    }
}

/// Encode backend action facts explicitly for the current wire schema.
pub fn encode_action(action: &a::Action, scope: &TargetScope) -> p::Action {
    match action {
        a::Action::Escape { hold } => p::Action::Escape {
            hold: scope.hold(*hold),
        },
        a::Action::ChooseGrowthSource { basis, source } => p::Action::ChooseGrowthSource {
            basis: Box::new(crate::advancement_adapter::wire_basis(basis)),
            source: crate::advancement_adapter::wire_growth_source(source),
        },
        a::Action::ChooseIntellectExpertise { basis, choice } => {
            p::Action::ChooseIntellectExpertise {
                basis: Box::new(crate::advancement_adapter::wire_basis(basis)),
                choice: crate::advancement_adapter::wire_intellect_expertise(choice),
            }
        }
        a::Action::Purchase { basis, purchase } => p::Action::Purchase {
            basis: Box::new(crate::advancement_adapter::wire_basis(basis)),
            purchase: crate::advancement_adapter::wire_purchase(purchase),
        },
        a::Action::Respec {
            basis,
            replacement,
            supplies,
        } => p::Action::Respec {
            basis: Box::new(crate::advancement_adapter::wire_basis(basis)),
            replacement: crate::advancement_adapter::wire_replacement(replacement),
            supplies: encode_supplies(supplies, scope),
        },
        a::Action::UseAbility {
            id,
            invocation,
            supplies,
        } => p::Action::UseAbility {
            id: id.to_string(),
            invocation: p::AbilityInvocation {
                aim: match &invocation.aim {
                    a::AbilityAim::ActorTarget { target } => p::AbilityAim::ActorTarget {
                        target: scope.actor(*target),
                    },
                    a::AbilityAim::ContactTarget { target } => p::AbilityAim::ActorTarget {
                        target: scope.contact(*target),
                    },
                    a::AbilityAim::DirectionalProjectile {
                        direction,
                        delivery,
                    } => p::AbilityAim::DirectionalProjectile {
                        direction: crate::adapt::wire_direction(*direction),
                        delivery: encode_projectile_delivery(*delivery, scope),
                    },
                    a::AbilityAim::SelfStep { direction } => p::AbilityAim::SelfStep {
                        direction: crate::adapt::wire_direction(*direction),
                    },
                    a::AbilityAim::DirectionalRay { direction } => p::AbilityAim::DirectionalRay {
                        direction: crate::adapt::wire_direction(*direction),
                    },
                },
                weapon: match invocation.weapon {
                    a::WeaponSelection::Default => p::WeaponSelection::Default,
                    a::WeaponSelection::Equipped { item, socket } => p::WeaponSelection::Equipped {
                        item: scope.item(s::ItemId(item)),
                        socket,
                    },
                },
            },
            supplies: encode_supplies(supplies, scope),
        },
        a::Action::ExamineTrap { trap } => p::Action::ExamineTrap {
            trap: scope.trap(*trap),
        },
        a::Action::SetDoor { door, open } => p::Action::SetDoor {
            door: scope.door(*door),
            open: *open,
        },
        a::Action::Move { direction } => p::Action::Move {
            direction: crate::adapt::wire_direction(*direction),
        },
        a::Action::Take { item, quantity } => p::Action::Take {
            item: scope.item(s::ItemId(*item)),
            quantity: *quantity,
        },
        a::Action::TouchApparentObject { subject, quantity } => p::Action::Take {
            item: scope.apparent_object(*subject),
            quantity: *quantity,
        },
        a::Action::Drop { item, quantity } => p::Action::Drop {
            item: scope.item(s::ItemId(*item)),
            quantity: *quantity,
        },
        a::Action::Equip { item, slot } => p::Action::Equip {
            item: scope.item(s::ItemId(*item)),
            slot: *slot,
        },
        a::Action::Unequip { item } => p::Action::Unequip {
            item: scope.item(s::ItemId(*item)),
        },
        a::Action::Ignite { item } => p::Action::Ignite {
            item: scope.item(s::ItemId(*item)),
        },
        a::Action::Extinguish { item } => p::Action::Extinguish {
            item: scope.item(s::ItemId(*item)),
        },
        a::Action::Drink { item } => p::Action::Drink {
            item: scope.item(s::ItemId(*item)),
        },
        a::Action::Eat { item } => p::Action::Eat {
            item: scope.item(s::ItemId(*item)),
        },
        a::Action::Read { item } => p::Action::Read {
            item: scope.item(s::ItemId(*item)),
        },
        a::Action::Zap { item, aim } => p::Action::Zap {
            item: scope.item(s::ItemId(*item)),
            aim: crate::adapt::wire_device_aim(*aim),
        },
        a::Action::Identify { choice, item } => p::Action::Identify {
            choice: scope.identification_choice(*choice),
            item: item.map(|item| scope.item(s::ItemId(item))),
        },
        a::Action::Wait => p::Action::Wait,
    }
}

/// Runtime-only diagnostic controls and bounded private queries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CombatDiagnosticsOperation {
    Inspect { through: Option<u64> },
    Capture { enabled: bool },
}

/// Normalized request facts before any live target resolution.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DecodedCommand {
    ProjectileChoices {
        expected_revision: u64,
        query: p::ProjectileChoiceQuery,
    },
    AbilityWeaponPreview {
        expected_revision: u64,
        ability: String,
        invocation: p::AbilityInvocation,
        supplies: p::SupplySelection,
    },
    Advancement {
        expected_revision: u64,
        query: p::AdvancementQuery,
    },
    Backend(Command),
    StrikeInspection {
        expected_revision: u64,
        target: p::ActorId,
        target_revision: u64,
        source: p::StrikeSourceSelector,
        cursor: Option<p::StrikePageCursor>,
    },
    CreatureCatalog {
        expected_revision: u64,
        depth: Option<u32>,
        after: Option<String>,
    },
    CreatureInspection {
        expected_revision: u64,
        target: p::ActorId,
    },
    CombatDiagnostics {
        expected_revision: u64,
        operation: CombatDiagnosticsOperation,
    },
    Gameplay {
        expected_revision: u64,
        action: p::Action,
    },
}

impl DecodedCommand {
    pub(crate) fn requires_control(&self) -> bool {
        match self {
            Self::Gameplay { .. }
            | Self::ProjectileChoices { .. }
            | Self::Advancement { .. }
            | Self::AbilityWeaponPreview { .. } => true,
            Self::CreatureCatalog { .. }
            | Self::CreatureInspection { .. }
            | Self::CombatDiagnostics { .. }
            | Self::StrikeInspection { .. } => false,
            Self::Backend(command) => matches!(
                command,
                Command::RenamePlace { .. }
                    | Command::ResumeIntention { .. }
                    | Command::CancelIntention { .. }
                    | Command::Act { .. }
                    | Command::AdmitIntention { .. }
                    | Command::Travel { .. }
            ),
        }
    }
    /// Compare with the original saved request without consulting live targets.
    pub fn matches(&self, command: &Command, scope: &TargetScope) -> bool {
        match (self, command) {
            (Self::Backend(request), original) => request == original,
            (
                Self::Gameplay {
                    expected_revision,
                    action,
                },
                Command::AdmitIntention {
                    expected_revision: original_revision,
                    action: original,
                },
            ) => {
                expected_revision == original_revision && *action == encode_action(original, scope)
            }
            _ => false,
        }
    }

    pub(crate) fn revision_requirement(&self) -> Option<(u64, &'static str)> {
        match self {
            Self::Backend(command) => command.revision_requirement(),
            Self::AbilityWeaponPreview {
                expected_revision, ..
            }
            | Self::ProjectileChoices {
                expected_revision, ..
            }
            | Self::Advancement {
                expected_revision, ..
            }
            | Self::CreatureCatalog {
                expected_revision, ..
            }
            | Self::CreatureInspection {
                expected_revision, ..
            }
            | Self::CombatDiagnostics {
                expected_revision, ..
            }
            | Self::StrikeInspection {
                expected_revision, ..
            } => Some((
                *expected_revision,
                "Refresh the observation before inspecting",
            )),
            Self::Gameplay {
                expected_revision, ..
            } => Some((*expected_revision, "Refresh the observation before acting")),
        }
    }
}

/// Normalize transport input; preserve gameplay references until fresh resolution.
pub fn decode_command(command: &tor_protocol::Command) -> Result<DecodedCommand, crate::Failure> {
    if let p::Command::ProjectileChoices {
        expected_revision,
        query,
    } = command
    {
        if !query.is_valid() {
            return Err(crate::Failure::new(
                p::ErrorCode::InvalidRequest,
                "Invalid projectile choice query",
            ));
        }
        return Ok(DecodedCommand::ProjectileChoices {
            expected_revision: *expected_revision,
            query: query.clone(),
        });
    }
    if let p::Command::PreviewAbilityWeapon {
        expected_revision,
        ability,
        invocation,
        supplies,
    } = command
    {
        if !p::valid_content_id(ability) {
            return Err(crate::Failure::new(
                p::ErrorCode::InvalidRequest,
                "Invalid ability identity",
            ));
        }
        return Ok(DecodedCommand::AbilityWeaponPreview {
            expected_revision: *expected_revision,
            ability: ability.clone(),
            invocation: *invocation,
            supplies: supplies.clone(),
        });
    }
    if let p::Command::Advancement {
        expected_revision,
        query,
    } = command
    {
        return Ok(DecodedCommand::Advancement {
            expected_revision: *expected_revision,
            query: query.clone(),
        });
    }
    if let p::Command::InspectStrikeSource {
        expected_revision,
        actor,
        target_revision,
        source,
        cursor,
    } = command
    {
        if actor.0 == 0
            || !source.valid()
            || cursor.as_ref().is_some_and(|cursor| {
                cursor.inspection.0.is_empty()
                    || cursor.inspection.0.len() > 128
                    || cursor.inspection.0.chars().any(char::is_control)
                    || !cursor.position.valid()
            })
        {
            return Err(crate::Failure::new(
                p::ErrorCode::InvalidRequest,
                "Invalid strike inspection query",
            ));
        }
        return Ok(DecodedCommand::StrikeInspection {
            expected_revision: *expected_revision,
            target: *actor,
            target_revision: *target_revision,
            source: source.clone(),
            cursor: cursor.clone(),
        });
    }

    // Keep the transport and journal schemas independent: extending either
    // enum must require an explicit decision at this boundary.
    if let p::Command::Act {
        expected_revision,
        action,
    } = command
    {
        return Ok(DecodedCommand::Gameplay {
            expected_revision: *expected_revision,
            action: action.clone(),
        });
    }
    if let p::Command::Wizard {
        expected_revision,
        operation,
    } = command
    {
        let words: Vec<_> = operation.split_whitespace().collect();
        if words.first() == Some(&"combat") {
            let operation = match words.as_slice() {
                ["combat", "inspect"] => {
                    Some(CombatDiagnosticsOperation::Inspect { through: None })
                }
                ["combat", "inspect", through] => through
                    .parse::<u64>()
                    .ok()
                    .filter(|value| *value != 0 && value.to_string() == *through)
                    .map(|through| CombatDiagnosticsOperation::Inspect {
                        through: Some(through),
                    }),
                ["combat", "capture", "on"] => {
                    Some(CombatDiagnosticsOperation::Capture { enabled: true })
                }
                ["combat", "capture", "off"] => {
                    Some(CombatDiagnosticsOperation::Capture { enabled: false })
                }
                _ => None,
            }
            .ok_or_else(|| {
                crate::Failure::new(
                    p::ErrorCode::InvalidRequest,
                    "Use wizard combat inspect [through] or wizard combat capture <on|off>",
                )
            })?;
            return Ok(DecodedCommand::CombatDiagnostics {
                expected_revision: *expected_revision,
                operation,
            });
        }
        if words.starts_with(&["creature", "list"]) {
            let usage = || {
                crate::Failure::new(
                    p::ErrorCode::InvalidRequest,
                    "Use wizard creature list [depth <N>] [after <selector>]",
                )
            };
            let (depth, after) = match &words[2..] {
                [] => (None, None),
                ["depth", depth] => (Some(*depth), None),
                ["after", after] => (None, Some(*after)),
                ["depth", depth, "after", after] => (Some(*depth), Some(*after)),
                _ => return Err(usage()),
            };
            let depth = depth
                .map(|value| {
                    value
                        .parse::<u32>()
                        .ok()
                        .filter(|v| *v > 0)
                        .ok_or_else(usage)
                })
                .transpose()?;
            if after.is_some_and(|value| !crate::wizard_creatures::valid_selector(value)) {
                return Err(usage());
            }
            return Ok(DecodedCommand::CreatureCatalog {
                expected_revision: *expected_revision,
                depth,
                after: after.map(str::to_owned),
            });
        }
        if words.starts_with(&["creature", "inspect"]) {
            let target = match words.as_slice() {
                ["creature", "inspect", target] => {
                    target.parse::<u64>().ok().filter(|value| *value != 0)
                }
                _ => None,
            }
            .ok_or_else(|| {
                crate::Failure::new(
                    p::ErrorCode::InvalidRequest,
                    "Use wizard creature inspect <actor>",
                )
            })?;
            return Ok(DecodedCommand::CreatureInspection {
                expected_revision: *expected_revision,
                target: p::ActorId(target),
            });
        }
    }
    Ok(DecodedCommand::Backend(match command {
        tor_protocol::Command::ResumeIntention {
            expected_revision,
            intention,
        } => Command::ResumeIntention {
            expected_revision: *expected_revision,
            admission: EntryId(intention.0.clone()),
        },
        tor_protocol::Command::CancelIntention {
            expected_revision,
            intention,
            step,
        } => Command::CancelIntention {
            expected_revision: *expected_revision,
            admission: EntryId(intention.0.clone()),
            step: step.map(crate::adapt::requested_direction),
        },
        tor_protocol::Command::RenamePlace {
            expected_revision,
            key,
            name,
        } => Command::RenamePlace {
            expected_revision: *expected_revision,
            key: key.clone(),
            name: name.clone(),
        },
        tor_protocol::Command::Travel {
            expected_revision,
            destination,
        } => Command::Travel {
            expected_revision: *expected_revision,
            destination: destination.clone(),
        },
        tor_protocol::Command::Wizard {
            expected_revision,
            operation,
        } => Command::Wizard {
            expected_revision: *expected_revision,
            operation: crate::developer::parse_wizard(operation).map_err(|_| {
                crate::Failure::new(
                    tor_protocol::ErrorCode::InvalidRequest,
                    "Invalid developer command",
                )
            })?,
        },
        p::Command::Act { .. }
        | p::Command::InspectStrikeSource { .. }
        | p::Command::Advancement { .. }
        | p::Command::ProjectileChoices { .. }
        | p::Command::PreviewAbilityWeapon { .. } => {
            unreachable!("runtime request decoded above")
        }
        tor_protocol::Command::Annotate {
            anchor,
            text,
            source,
            audience,
            category,
        } => Command::Annotate {
            anchor: anchor.clone(),
            text: text.clone(),
            source: *source,
            audience: *audience,
            category: *category,
        },
    }))
}

/// Encode supported backend requests and refuse operations with no wire authority.
pub fn encode_command(
    command: Command,
    scope: &TargetScope,
) -> Result<tor_protocol::Command, &'static str> {
    match command {
        Command::ResumeIntention {
            expected_revision,
            admission,
        } => Ok(tor_protocol::Command::ResumeIntention {
            expected_revision,
            intention: tor_protocol::IntentionId(admission.0),
        }),
        Command::CancelIntention {
            expected_revision,
            admission,
            step,
        } => Ok(tor_protocol::Command::CancelIntention {
            expected_revision,
            intention: tor_protocol::IntentionId(admission.0),
            step: step.map(crate::adapt::wire_direction),
        }),
        Command::PausePreparation => Err("Preparation suspension is backend-only"),
        Command::Wizard {
            expected_revision,
            operation,
        } => Ok(tor_protocol::Command::Wizard {
            expected_revision,
            operation: serde_json::to_string(&operation).expect("developer command serializes"),
        }),
        Command::RenamePlace {
            expected_revision,
            key,
            name,
        } => Ok(tor_protocol::Command::RenamePlace {
            expected_revision,
            key,
            name,
        }),
        Command::Travel {
            expected_revision,
            destination,
        } => Ok(tor_protocol::Command::Travel {
            expected_revision,
            destination,
        }),
        Command::AdmitIntention {
            expected_revision,
            action,
        }
        | Command::Act {
            expected_revision,
            action,
        } => Ok(tor_protocol::Command::Act {
            expected_revision,
            action: encode_action(&action, scope),
        }),
        Command::Annotate {
            anchor,
            text,
            source,
            audience,
            category,
        } => Ok(tor_protocol::Command::Annotate {
            anchor,
            text,
            source,
            audience,
            category,
        }),
    }
}

/// Only this projection crosses the network; journal topology stays private.
pub fn disclose_entry(
    entry: &crate::journal::JournalEntry,
    scope: &TargetScope,
) -> Option<tor_protocol::HistoryEntry> {
    use crate::journal::{Action, Event, JournalContent, WizardResult};
    use tor_protocol::{Event as VisibleEvent, HistoryContent as Content};
    if entry.content.private_advancement() {
        return None;
    }
    let content = match &entry.content {
        JournalContent::IntentionAdmitted { .. }
        | JournalContent::AutonomousIntentionAdmitted { .. }
        | JournalContent::TravelIntentionAdmitted { .. }
        | JournalContent::IntentionFailed { .. }
        | JournalContent::IntentionContinuationFailed { .. }
        | JournalContent::IntentionChanged { .. } => return None,
        JournalContent::PlaceRenamed { key, name } => Content::PlaceRenamed {
            key: key.clone(),
            name: name.clone(),
        },
        JournalContent::Travel { destination } => Content::Travel {
            destination: destination.clone(),
        },
        JournalContent::Wizard { result, .. } => Content::Wizard {
            summary: match result {
                WizardResult::CreatureSpawned {
                    actor,
                    selector,
                    depth,
                    ..
                } => {
                    let mode = if selector.starts_with("archetype:") {
                        "authored base; no population AI or stock".into()
                    } else {
                        depth.map_or_else(
                            || "authored base".into(),
                            |depth| format!("depth {depth}"),
                        )
                    };
                    format!("Spawned actor {} from {} ({mode}).", actor.0, selector)
                }

                WizardResult::ArenaControlled {
                    paused: true,
                    advance: 0,
                } => "Arena paused.".into(),
                WizardResult::ArenaControlled { paused: true, .. } => {
                    "Arena action advance requested.".into()
                }
                WizardResult::ArenaControlled { paused: false, .. } => "Arena resumed.".into(),
                WizardResult::Rewound { .. } => "Timeline rewound.".into(),
                _ => "Developer setup completed.".into(),
            },
            rewind: matches!(result, WizardResult::Rewound { .. }),
        },
        JournalContent::Action { action, event }
        | JournalContent::IntentionStarted { action, event, .. }
        | JournalContent::IntentionContinued { action, event, .. } => Content::Action {
            action: encode_action(action, scope),
            event: Box::new(match event {
                Event::GrowthSourceChosen
                | Event::IntellectExpertiseChosen
                | Event::Purchased
                | Event::RespecStarted
                | Event::IdentificationResolved { .. }
                | Event::TrapExamined { .. } => return None,
                Event::EscapeStarted { hold } => VisibleEvent::EscapeStarted {
                    hold: scope.hold(*hold),
                },
                Event::AbilityStarted {
                    id,
                    name,
                    target,
                    reference,
                } => VisibleEvent::AbilityStarted {
                    ability: p::ContentRef {
                        id: id.to_string(),
                        name: name.clone(),
                    },
                    target: scope.actor_reference(s::ActorId(target.0), *reference),
                },
                Event::ItemStarted { action } => VisibleEvent::ItemStarted {
                    action: encode_action(action, scope),
                },
                Event::PreparationPaused => VisibleEvent::PreparationPaused,
                Event::DoorChanged { door, open } => VisibleEvent::DoorChanged {
                    door: scope.door(*door),
                    open: *open,
                },
                Event::Moved { .. } => VisibleEvent::Moved {
                    direction: match action {
                        Action::Move { direction } => crate::adapt::wire_direction(*direction),
                        _ => unreachable!("movement has a move action"),
                    },
                },
                Event::Crumbled { item, quantity } => VisibleEvent::Crumbled {
                    item: scope.item(s::ItemId(*item)),
                    quantity: *quantity,
                },
                Event::Taken {
                    item,
                    result,
                    quantity,
                } => VisibleEvent::Taken {
                    item: scope.item(s::ItemId(*item)),
                    result: scope.item(s::ItemId(*result)),
                    quantity: *quantity,
                },
                Event::Dropped {
                    item,
                    result,
                    quantity,
                } => VisibleEvent::Dropped {
                    item: scope.item(s::ItemId(*item)),
                    result: scope.item(s::ItemId(*result)),
                    quantity: *quantity,
                },
                Event::Escaped { item } => VisibleEvent::Escaped {
                    item: scope.item(s::ItemId(*item)),
                },
                Event::Waited => VisibleEvent::Waited,
            }),
        },
        JournalContent::Annotation {
            anchor,
            category,
            text,
        } => Content::Annotation {
            anchor: anchor.clone(),
            category: *category,
            text: text.clone(),
        },
    };
    Some(tor_protocol::HistoryEntry {
        id: entry.id.clone(),
        branch: entry.branch.clone(),
        actor: entry.actor,
        tick: entry.tick,
        author: entry.author.clone(),
        audience: entry.audience,
        content,
    })
}

#[cfg(test)]
mod inspection_query_tests {
    use super::*;
    #[test]
    fn creature_inspection_decodes_as_a_readonly_query() {
        let decode = |operation: &str| {
            decode_command(&p::Command::Wizard {
                expected_revision: 17,
                operation: operation.into(),
            })
        };
        assert_eq!(
            decode("creature inspect 2").unwrap(),
            DecodedCommand::CreatureInspection {
                expected_revision: 17,
                target: p::ActorId(2),
            }
        );
        for text in [
            "creature inspect",
            "creature inspect 0",
            "creature inspect -1",
            "creature inspect 2 extra",
        ] {
            assert_eq!(decode(text).unwrap_err().code, p::ErrorCode::InvalidRequest);
        }
        let query = decode("creature inspect 2").unwrap();
        assert!(!query.requires_control());
        assert_eq!(query.revision_requirement().unwrap().0, 17);
    }
    #[test]
    fn combat_diagnostics_commands_are_private_nonjournal_requests() {
        let decode = |operation: &str| {
            decode_command(&p::Command::Wizard {
                expected_revision: 17,
                operation: operation.into(),
            })
        };
        for (text, operation) in [
            (
                "combat inspect",
                CombatDiagnosticsOperation::Inspect { through: None },
            ),
            (
                "combat inspect 23",
                CombatDiagnosticsOperation::Inspect { through: Some(23) },
            ),
            (
                "combat capture on",
                CombatDiagnosticsOperation::Capture { enabled: true },
            ),
            (
                "combat capture off",
                CombatDiagnosticsOperation::Capture { enabled: false },
            ),
        ] {
            let command = decode(text).unwrap();
            assert_eq!(
                command,
                DecodedCommand::CombatDiagnostics {
                    expected_revision: 17,
                    operation,
                }
            );
            assert!(!command.requires_control());
            assert_eq!(command.revision_requirement().unwrap().0, 17);
        }
        for text in [
            "combat",
            "combat inspect 0",
            "combat inspect -1",
            "combat inspect +1",
            "combat inspect 01",
            "combat inspect 1 extra",
            "combat inspect 18446744073709551616",
            "combat capture",
            "combat capture true",
            "combat capture on extra",
            "combat unknown",
        ] {
            assert_eq!(
                decode(text).unwrap_err().code,
                p::ErrorCode::InvalidRequest,
                "{text}"
            );
        }
    }
}

#[cfg(test)]
mod strike_sequence_adapter_tests {
    use super::*;

    #[test]
    fn ordered_strikes_cancel_step_roundtrips_independent_wire_and_backend_shapes() {
        let scope = TargetScope::new(uuid::Uuid::nil(), s::ActorId(1));
        for step in [
            serde_json::Value::Null,
            serde_json::json!("north_east"),
            serde_json::json!("down"),
        ] {
            let wire = serde_json::json!({"type":"cancel_intention","expected_revision":"9007199254740993",
                "intention":"same-admission","step":step});
            let command: p::Command = serde_json::from_value(wire.clone()).unwrap();
            let DecodedCommand::Backend(backend) = decode_command(&command).unwrap() else {
                panic!("ordinary command");
            };
            let stored = serde_json::to_value(&backend).unwrap();
            assert_eq!(
                stored["expected_revision"],
                serde_json::json!(9007199254740993u64)
            );
            assert_eq!(stored["admission"], "same-admission");
            assert_eq!(stored["step"], wire["step"]);
            assert!(stored.get("intention").is_none());
            let restored: Command = serde_json::from_value(stored).unwrap();
            assert_eq!(
                serde_json::to_value(encode_command(restored, &scope).unwrap()).unwrap(),
                wire
            );
        }
    }

    #[test]
    fn ordered_strikes_saved_cancel_requires_step_and_rejects_extra_authority() {
        for step in [serde_json::Value::Null, serde_json::json!("west")] {
            let saved = serde_json::json!({"type":"cancel_intention","expected_revision":7,
                "admission":"same-admission","step":step});
            let decoded: Command = serde_json::from_value(saved.clone()).unwrap();
            assert_eq!(serde_json::to_value(decoded).unwrap(), saved);
            for extra in ["actor", "cost", "recovery"] {
                let mut forged = saved.clone();
                forged[extra] = serde_json::json!(0);
                assert!(serde_json::from_value::<Command>(forged).is_err());
            }
        }
        assert!(serde_json::from_value::<Command>(serde_json::json!({
            "type":"cancel_intention","expected_revision":7,"admission":"same-admission"
        }))
        .is_err());
    }
}

fn decode_supplies(
    value: &p::SupplySelection,
    scope: &TargetScope,
    observation: &s::Observation,
) -> Result<a::SupplySelection, crate::Failure> {
    Ok(match value {
        p::SupplySelection::Carried => a::SupplySelection::Carried,
        p::SupplySelection::Explicit { items } => {
            let mut selected = items
                .iter()
                .map(|row| {
                    let item = observation
                        .inventory
                        .iter()
                        .map(|item| item.id)
                        .chain(observation.ground_items.iter().map(|item| item.id))
                        .find(|item| scope.item(*item) == row.item)
                        .ok_or_else(|| {
                            crate::Failure::new(
                                p::ErrorCode::InvalidAction,
                                "Target is unavailable",
                            )
                        })?;
                    Ok(a::SelectedSupply {
                        cost_ordinal: row.cost_ordinal,
                        item: item.0,
                    })
                })
                .collect::<Result<Vec<_>, crate::Failure>>()?;
            selected.sort_by_key(|row| (row.cost_ordinal, row.item));
            a::SupplySelection::Explicit { items: selected }
        }
    })
}
fn encode_supplies(value: &a::SupplySelection, scope: &TargetScope) -> p::SupplySelection {
    match value {
        a::SupplySelection::Carried => p::SupplySelection::Carried,
        a::SupplySelection::Explicit { items } => {
            // A durable receipt can reconstruct a last-unit source after deletion.
            let mut items: Vec<_> = items
                .iter()
                .map(|row| p::SelectedSupply {
                    cost_ordinal: row.cost_ordinal,
                    item: scope.item(s::ItemId(row.item)),
                })
                .collect();
            items.sort_by_key(|row| (row.cost_ordinal, row.item));
            p::SupplySelection::Explicit { items }
        }
    }
}

#[cfg(test)]
mod retrieval_event_tests {
    use super::*;

    #[test]
    fn intentional_escape_saves_exact_identity_and_discloses_only_the_scoped_target() {
        let id = s::ItemId(9_007_199_254_740_993);
        let event = crate::adapt::event(s::OutcomeKind::Escaped { item: id });
        let stored = serde_json::to_value(&event).unwrap();
        assert_eq!(stored, serde_json::json!({"type":"escaped","item":id.0}));
        let decoded: crate::journal::Event =
            crate::storage::codec::strict(&serde_json::to_vec(&event).unwrap()).unwrap();
        assert_eq!(decoded, event);
        let entry = crate::journal::JournalEntry {
            id: p::EntryId("retrieval-entry".into()),
            branch: p::BranchId("retrieval-branch".into()),
            actor: p::ActorId(1),
            tick: 100,
            author: p::Author::User {
                user: "player".into(),
            },
            audience: p::Audience::Actor,
            intention_suspensions: Vec::new(),
            intention_ends: Vec::new(),
            content: crate::journal::JournalContent::Action {
                action: a::Action::Move {
                    direction: a::Direction::Up,
                },
                event,
            },
        };
        assert!(entry.visible_to(p::ActorId(1), "player"));
        assert!(entry.visible_to(p::ActorId(1), "spectator"));
        assert!(!entry.visible_to(p::ActorId(2), "player"));
        assert!(!entry.visible_to(p::ActorId(2), "spectator"));
        let scope = TargetScope::new(uuid::Uuid::nil(), s::ActorId(1));
        let disclosed = disclose_entry(&entry, &scope).unwrap();
        assert!(disclosed.visible_to(p::ActorId(1), "player"));
        assert!(disclosed.visible_to(p::ActorId(1), "spectator"));
        assert!(!disclosed.visible_to(p::ActorId(2), "player"));
        assert!(!disclosed.visible_to(p::ActorId(2), "spectator"));
        let p::HistoryContent::Action { event, .. } = disclosed.content else {
            panic!("terminal escape must remain an action");
        };
        let p::Event::Escaped { item } = *event else {
            panic!("terminal escape must remain distinct from movement");
        };
        assert_eq!(item, scope.item(id));
        assert_ne!(
            item,
            TargetScope::new(uuid::Uuid::nil(), s::ActorId(2)).item(id)
        );
        assert!(!serde_json::to_string(&item)
            .unwrap()
            .contains(&id.0.to_string()));
    }
}

#[cfg(test)]
#[path = "perception_wire_tests.rs"]
mod perception_wire_tests;
