use crate::journal::Direction;
use crate::journal::{Position, RegionView, WizardItem, WizardOperation};
use tor_protocol::{ActorId, EntryId};

pub fn parse_wizard(text: &str) -> Result<WizardOperation, String> {
    if text.trim_start().starts_with('{') {
        return serde_json::from_str(text).map_err(|_| "Invalid developer command".into());
    }
    let words: Vec<_> = text.split_whitespace().collect();
    let usage = "Wizard commands: wizard identify <actor> <item>; wizard chamber <id> <width> <depth> <height> <name>; wizard door <region> <x> <y> <z> <open|closed>; wizard item <token|tablet> <region> <x> <y> <z>; wizard actor <turn-ticks> <region> <x> <y> <z>; wizard teleport <actor> <region> <x> <y> <z>; wizard rewind <initial|entry-id>; wizard room <id> <width> <depth> <height> <name>; wizard connect <from-region> <x> <y> <z> <direction> <to-region> <x> <y> <z> <quarter-turns>; wizard place <region> <x> <y> <z> <on|off>; wizard wall <region> <x> <y> <z> <open|closed>";
    let position = |v: &[&str]| -> Result<Position, String> {
        Ok(Position {
            region: v[0].parse().map_err(|_| usage)?,
            x: v[1].parse().map_err(|_| usage)?,
            y: v[2].parse().map_err(|_| usage)?,
            z: v[3].parse().map_err(|_| usage)?,
        })
    };
    let operation = match words.as_slice() {
        ["gravity", region, x, y, z] => WizardOperation::SetGravity {
            region: region.parse().map_err(|_| usage)?,
            vector: [
                x.parse().map_err(|_| usage)?,
                y.parse().map_err(|_| usage)?,
                z.parse().map_err(|_| usage)?,
            ],
        },
        ["cell-gravity", r, x, y, z, gx, gy, gz] => WizardOperation::SetCellGravity {
            position: position(&[r, x, y, z])?,
            vector: [
                gx.parse().map_err(|_| usage)?,
                gy.parse().map_err(|_| usage)?,
                gz.parse().map_err(|_| usage)?,
            ],
        },
        ["velocity", actor, x, y, z] => WizardOperation::SetVelocity {
            actor: ActorId(actor.parse().map_err(|_| usage)?),
            velocity: [
                x.parse().map_err(|_| usage)?,
                y.parse().map_err(|_| usage)?,
                z.parse().map_err(|_| usage)?,
            ],
        },
        ["door", r, x, y, z, state, height @ ..] if height.len() <= 1 => {
            WizardOperation::PlaceDoor {
                position: position(&[r, x, y, z])?,
                open: match *state {
                    "open" => true,
                    "closed" => false,
                    _ => return Err(usage.into()),
                },
                height: match height {
                    [] => 1,
                    [cells] => cells.parse().map_err(|_| usage)?,
                    _ => unreachable!("at most one height"),
                },
            }
        }
        ["join", r, x, y, z, facing, r2, x2, y2, z2, turns, width, height] => {
            WizardOperation::ConnectArea {
                from: position(&[r, x, y, z])?,
                to: position(&[r2, x2, y2, z2])?,
                direction: match *facing {
                    "north" => Direction::North,
                    "east" => Direction::East,
                    "south" => Direction::South,
                    "west" => Direction::West,
                    "up" => Direction::Up,
                    "down" => Direction::Down,
                    _ => return Err(usage.into()),
                },
                quarter_turns: turns.parse().map_err(|_| usage)?,
                width: width.parse().map_err(|_| usage)?,
                height: height.parse().map_err(|_| usage)?,
            }
        }
        [kind @ ("room" | "chamber"), id, width, depth, height, name @ ..] if !name.is_empty() => {
            let region = RegionView {
                id: id.parse().map_err(|_| usage)?,
                name: name.join(" "),
                width: width.parse().map_err(|_| usage)?,
                depth: depth.parse().map_err(|_| usage)?,
                height: height.parse().map_err(|_| usage)?,
            };
            if *kind == "chamber" {
                WizardOperation::PlaceChamber { region }
            } else {
                WizardOperation::PlaceRoom { region }
            }
        }
        ["connect", r, x, y, z, facing, r2, x2, y2, z2, turns] => WizardOperation::Connect {
            from: position(&[r, x, y, z])?,
            direction: match *facing {
                "north" => Direction::North,
                "east" => Direction::East,
                "south" => Direction::South,
                "west" => Direction::West,
                "up" => Direction::Up,
                "down" => Direction::Down,
                _ => return Err(usage.into()),
            },
            to: position(&[r2, x2, y2, z2])?,
            quarter_turns: turns.parse().map_err(|_| usage)?,
        },
        ["place", r, x, y, z, value] => WizardOperation::SetPlaceHint {
            position: position(&[r, x, y, z])?,
            present: match *value {
                "on" => true,
                "off" => false,
                _ => return Err(usage.into()),
            },
        },
        ["wall", r, x, y, z, value] => WizardOperation::SetWall {
            position: position(&[r, x, y, z])?,
            wall: match *value {
                "closed" => true,
                "open" => false,
                _ => return Err(usage.into()),
            },
        },
        ["identify", actor, item] => WizardOperation::IdentifyItem {
            actor: tor_protocol::ActorId(actor.parse().map_err(|_| usage)?),
            item: item.parse().map_err(|_| usage)?,
        },
        ["item", kind, r, x, y, z] => WizardOperation::PlaceItem {
            kind: match *kind {
                "token" => WizardItem::Token,
                "tablet" => WizardItem::Tablet,
                _ => return Err(usage.into()),
            },
            position: position(&[r, x, y, z])?,
        },
        ["actor", ticks, r, x, y, z] => WizardOperation::SpawnActor {
            turn_ticks: ticks.parse().map_err(|_| usage)?,
            position: position(&[r, x, y, z])?,
        },
        ["teleport", actor, r, x, y, z] => WizardOperation::Teleport {
            actor: ActorId(actor.parse().map_err(|_| usage)?),
            position: position(&[r, x, y, z])?,
        },
        ["rewind", target] => WizardOperation::Rewind {
            target: (*target != "initial").then(|| EntryId((*target).into())),
        },
        _ => return Err(usage.into()),
    };
    Ok(operation)
}
