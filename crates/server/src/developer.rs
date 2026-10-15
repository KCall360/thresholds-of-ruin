use crate::journal::{CreatureAdvancement, Position, RegionView, WizardItem, WizardOperation};
use crate::journal::{Direction, PurchaseReplacement};
use tor_protocol::{ActorId, EntryId};

fn catalog_value<T: serde::de::DeserializeOwned>(name: &str, usage: &str) -> Result<T, String> {
    if !tor_simulation::content_ids::valid_id(name) {
        return Err(usage.into());
    }
    serde_json::from_value(serde_json::Value::String(name.into())).map_err(|_| usage.into())
}

pub fn parse_wizard(text: &str) -> Result<WizardOperation, String> {
    if text.trim_start().starts_with('{') {
        return serde_json::from_str(text).map_err(|_| "Invalid developer command".into());
    }
    let words: Vec<_> = text.split_whitespace().collect();
    let usage = "Wizard commands: wizard creature list [depth <N>] [after <selector>]; wizard creature spawn <selector> <region> <x> <y> <z> [depth <N>]; wizard creature add-hd <actor> <racial|class-id>; wizard creature train <actor> <hit-die> <skill>; wizard creature attribute <actor> <hit-die> <attribute>; wizard creature talent <actor> <hit-die> <talent>; wizard creature replace <actor> <hit-die> <purchase> refund|<pool> <skill|talent> <id>; wizard creature template <actor> <template> <on|off>; wizard creature remove-hd <actor>; wizard arena pause|resume|step [actions]; wizard identify <actor> <item>; wizard chamber <id> <width> <depth> <height> <name>; wizard door <region> <x> <y> <z> <open|closed>; wizard item <token|tablet> <region> <x> <y> <z>; wizard actor <turn-ticks> <region> <x> <y> <z>; wizard teleport <actor> <region> <x> <y> <z>; wizard rewind <initial|entry-id>; wizard room <id> <width> <depth> <height> <name>; wizard connect <from-region> <x> <y> <z> <direction> <to-region> <x> <y> <z> <quarter-turns>; wizard place <region> <x> <y> <z> <on|off>; wizard wall <region> <x> <y> <z> <open|closed>";
    let owner = |text: &str| -> Result<u64, String> {
        text.parse::<u64>()
            .ok()
            .filter(|value| *value > 0)
            .ok_or_else(|| usage.into())
    };
    let purchase_index = |text: &str| -> Result<u16, String> {
        text.parse::<u16>()
            .ok()
            .filter(|value| (1..=256).contains(value))
            .ok_or_else(|| usage.into())
    };
    let position = |v: &[&str]| -> Result<Position, String> {
        Ok(Position {
            region: v[0].parse().map_err(|_| usage)?,
            x: v[1].parse().map_err(|_| usage)?,
            y: v[2].parse().map_err(|_| usage)?,
            z: v[3].parse().map_err(|_| usage)?,
        })
    };
    let operation = match words.as_slice() {
        ["creature", "spawn", selector, r, x, y, z, tail @ ..] => {
            let depth = match tail {
                [] => None,
                ["depth", value] => {
                    Some(value.parse::<u32>().ok().filter(|v| *v > 0).ok_or(usage)?)
                }
                _ => return Err(usage.into()),
            };
            if !crate::wizard_creatures::valid_selector(selector) {
                return Err(usage.into());
            }
            WizardOperation::SpawnCreature {
                selector: (*selector).into(),
                position: position(&[r, x, y, z])?,
                depth,
            }
        }
        ["creature", "add-hd", actor, source] => WizardOperation::AdvanceCreature {
            actor: ActorId(actor.parse().map_err(|_| usage)?),
            advancement: CreatureAdvancement::AddHitDie {
                source: catalog_value(source, usage)?,
            },
        },
        ["creature", "train", actor, ordinal, skill] => WizardOperation::AdvanceCreature {
            actor: ActorId(actor.parse().map_err(|_| usage)?),
            advancement: CreatureAdvancement::Train {
                owner: owner(ordinal)?,
                skill: crate::creature_authoring::skill(skill)
                    .map(|_| (*skill).to_owned())
                    .map_err(|_| usage)?,
            },
        },
        ["creature", "attribute", actor, ordinal, attribute] => WizardOperation::AdvanceCreature {
            actor: ActorId(actor.parse().map_err(|_| usage)?),
            advancement: CreatureAdvancement::IncreaseAttribute {
                owner: owner(ordinal)?,
                attribute: crate::creature_authoring::attribute(attribute)
                    .map(|_| (*attribute).to_owned())
                    .map_err(|_| usage)?,
            },
        },
        ["creature", "talent", actor, ordinal, talent] => WizardOperation::AdvanceCreature {
            actor: ActorId(actor.parse().map_err(|_| usage)?),
            advancement: CreatureAdvancement::SelectTalent {
                owner: owner(ordinal)?,
                talent: catalog_value(talent, usage)?,
            },
        },
        ["creature", "replace", actor, ordinal, purchase, "refund"] => {
            WizardOperation::AdvanceCreature {
                actor: ActorId(actor.parse().map_err(|_| usage)?),
                advancement: CreatureAdvancement::ReplacePurchase {
                    owner: owner(ordinal)?,
                    purchase: purchase_index(purchase)?,
                    replacement: PurchaseReplacement::Refund {},
                },
            }
        }
        ["creature", "replace", actor, ordinal, purchase, pool, kind, value] => {
            WizardOperation::AdvanceCreature {
                actor: ActorId(actor.parse().map_err(|_| usage)?),
                advancement: CreatureAdvancement::ReplacePurchase {
                    owner: owner(ordinal)?,
                    purchase: purchase_index(purchase)?,
                    replacement: match *kind {
                        "skill" => PurchaseReplacement::SkillRank {
                            pool: catalog_value(pool, usage)?,
                            skill: crate::creature_authoring::skill(value)
                                .map(|_| (*value).to_owned())
                                .map_err(|_| usage)?,
                        },
                        "talent" => PurchaseReplacement::Talent {
                            pool: catalog_value(pool, usage)?,
                            talent: catalog_value(value, usage)?,
                        },
                        _ => return Err(usage.into()),
                    },
                },
            }
        }
        ["creature", "template", actor, template, mode] => WizardOperation::SetCreatureTemplate {
            actor: ActorId(actor.parse().map_err(|_| usage)?),
            template: (*template).into(),
            enabled: match *mode {
                "on" => true,
                "off" => false,
                _ => return Err(usage.into()),
            },
        },
        ["creature", "remove-hd", actor] => WizardOperation::RemoveCreatureHitDie {
            actor: ActorId(actor.parse().map_err(|_| usage)?),
        },
        ["arena", "pause"] => WizardOperation::ArenaControl {
            paused: true,
            advance: 0,
        },
        ["arena", "resume"] => WizardOperation::ArenaControl {
            paused: false,
            advance: 0,
        },
        ["arena", "step"] => WizardOperation::ArenaControl {
            paused: true,
            advance: 1,
        },
        ["arena", "step", count] => {
            let advance = count.parse::<u64>().map_err(|_| usage)?;
            if !(1..=10000).contains(&advance) {
                return Err(usage.into());
            }
            WizardOperation::ArenaControl {
                paused: true,
                advance,
            }
        }
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

#[cfg(test)]
mod arena_tests {
    use super::*;
    #[test]
    fn arena_controls_parse_bounded_steps_and_explicit_pause_resume() {
        for (text, paused, advance) in [
            ("arena pause", true, 0),
            ("arena resume", false, 0),
            ("arena step", true, 1),
            ("arena step 3", true, 3),
        ] {
            assert_eq!(
                parse_wizard(text).unwrap(),
                WizardOperation::ArenaControl { paused, advance }
            );
        }
        for text in [
            "arena",
            "arena step 0",
            "arena step -1",
            "arena step 10001",
            "arena step 18446744073709551616",
            "arena pause 1",
        ] {
            assert!(parse_wizard(text).is_err(), "{text}");
        }
    }
}

#[cfg(test)]
mod creature_tests {
    use super::*;
    #[test]
    fn removal_names_one_actor_and_rejects_forged_or_invalid_arguments() {
        assert_eq!(
            parse_wizard("creature remove-hd 42").unwrap(),
            WizardOperation::RemoveCreatureHitDie { actor: ActorId(42) }
        );
        for text in [
            "creature remove-hd",
            "creature remove-hd -1",
            "creature remove-hd 1 2",
            "creature remove-hd 18446744073709551616",
            "creature remove-hd nope",
        ] {
            assert!(parse_wizard(text).is_err(), "{text}");
        }
        assert!(
            parse_wizard(r#"{"type":"remove_creature_hit_die","actor":1,"hit_dice":99}"#).is_err()
        );
    }
}

#[cfg(test)]
mod template_tests {
    use super::*;
    #[test]
    fn templates_parse_explicit_modes_and_reject_forged_definitions() {
        for (mode, enabled) in [("on", true), ("off", false)] {
            assert_eq!(
                parse_wizard(&format!("creature template 7 arcane {mode}")).unwrap(),
                WizardOperation::SetCreatureTemplate {
                    actor: ActorId(7),
                    template: "arcane".into(),
                    enabled
                }
            );
        }
        for text in [
            "creature template",
            "creature template 1 arcane",
            "creature template -1 arcane on",
            "creature template 1 arcane toggle",
            "creature template 1 arcane on extra",
        ] {
            assert!(parse_wizard(text).is_err(), "{text}");
        }
        assert!(parse_wizard(r#"{"type":"set_creature_template","actor":1,"template":"arcane","enabled":true,"grants":[]}"#).is_err());
        assert!(parse_wizard(r#"{"type":"set_creature_template","actor":1,"template":{"kind":"undead"},"enabled":true}"#).is_err());
    }
}

#[cfg(test)]
mod advancement_tests {
    use super::*;
    #[test]
    fn purchase_replacement_uses_strict_one_based_locations_and_typed_choices() {
        for text in [
            "creature replace 1 2 1 refund",
            "creature replace 1 2 1 advancement skill athletics",
            "creature replace 1 2 1 advancement talent hardiness",
        ] {
            let operation = parse_wizard(text).unwrap();
            let restored: WizardOperation =
                serde_json::from_str(&serde_json::to_string(&operation).unwrap()).unwrap();
            assert_eq!(operation, restored);
        }
        for text in [
            "creature replace 1 0 1 refund",
            "creature replace 1 2 0 refund",
            "creature replace 1 2 257 refund",
            "creature replace 1 2 1 refund extra",
            "creature replace 1 2 1 bad! skill athletics",
            "creature replace 1 2 1 advancement skill unknown",
            "creature replace 1 2 1 advancement spell magic_bolt",
        ] {
            assert!(parse_wizard(text).is_err(), "{text}");
        }
        assert!(parse_wizard(r#"{"type":"advance_creature","actor":1,"advancement":{"type":"replace_purchase","owner":2,"purchase":1,"replacement":{"kind":"refund","extra":true}}}"#).is_err());
    }
    #[test]
    fn advancement_uses_explicit_one_based_owners_and_named_catalog_values() {
        // Content IDs parse independently of the loaded catalog; resolution checks existence.
        for text in [
            "creature add-hd 1 racial",
            "creature add-hd 1 warrior",
            "creature add-hd 1 mage",
            "creature add-hd 1 bard",
            "creature talent 1 4 unknown",
            "creature train 1 4 athletics",
            "creature attribute 1 4 strength",
            "creature talent 1 4 hardiness",
        ] {
            let operation = parse_wizard(text).unwrap();
            assert!(matches!(operation, WizardOperation::AdvanceCreature { .. }));
            let restored: WizardOperation =
                serde_json::from_str(&serde_json::to_string(&operation).unwrap()).unwrap();
            assert_eq!(operation, restored);
        }
        for text in [
            "creature add-hd 1 bad!",
            "creature add-hd -1 warrior",
            "creature add-hd 1 warrior extra",
            "creature train 1 0 athletics",
            "creature train 1 18446744073709551616 athletics",
            "creature train 1 -1 athletics",
            "creature train 1 4 unknown",
            "creature attribute 1 4 unknown",
            "creature talent 1 4 bad!",
            "creature train 1 athletics",
            "creature attribute 1 strength",
            "creature talent 1 hardiness",
        ] {
            assert!(parse_wizard(text).is_err(), "{text}");
        }
        assert!(parse_wizard(r#"{"type":"advance_creature","actor":1,"advancement":{"type":"add_hit_die","source":"warrior","health_seed":42}}"#).is_err());
    }
}

#[cfg(test)]
mod wide_scalar_tests {
    use super::*;
    #[test]
    fn wide_scalar_wizard_hd_owner_is_full_width_and_purchase_is_bounded() {
        assert!(matches!(
            parse_wizard("creature train 1 18446744073709551615 athletics").unwrap(),
            WizardOperation::AdvanceCreature {
                advancement: CreatureAdvancement::Train {
                    owner: u64::MAX,
                    ..
                },
                ..
            }
        ));
        assert!(parse_wizard("creature replace 1 65537 256 refund").is_ok());
        for command in [
            "creature train 1 0 athletics",
            "creature train 1 18446744073709551616 athletics",
            "creature replace 1 65537 257 refund",
        ] {
            assert!(parse_wizard(command).is_err());
        }
    }
}

#[cfg(test)]
mod named_creature_tests {
    use super::*;

    #[test]
    fn named_creature_spawn_rejects_bad_depth_coordinates_and_extra_arguments() {
        for text in [
            "creature spawn",
            "creature spawn rat 1 2 1 0 depth 0",
            "creature spawn rat 1 2 1 0 depth 4294967296",
            "creature spawn rat 1 x 1 0",
            "creature spawn rat 1 2 1 0 extra",
            "creature spawn rat 1 2 1 0 9",
        ] {
            assert!(parse_wizard(text).is_err(), "{text}");
        }
        assert!(parse_wizard("creature spawn rat 1 2 1 0").is_ok());
    }

    #[test]
    fn named_creature_spawn_preserves_an_explicit_authored_recipe_and_depth() {
        let operation =
            parse_wizard("creature spawn population:baseline/foes/rat 1 2 1 0 depth 27")
                .expect("named authored creatures must be spawnable");
        let encoded = serde_json::to_value(&operation).unwrap();
        assert_eq!(encoded["type"], "spawn_creature");
        assert_eq!(encoded["selector"], "population:baseline/foes/rat");
        assert_eq!(encoded["depth"], 27);
        assert_eq!(encoded["position"]["region"], 1);
        assert_eq!(encoded["position"]["x"], 2);
        assert_eq!(
            serde_json::from_value::<WizardOperation>(encoded).unwrap(),
            operation
        );
    }
}
