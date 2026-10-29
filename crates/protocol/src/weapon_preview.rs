//! Owner-scoped terms for one exact ability/weapon choice; never affordability.
use crate::{AbilityInvocation, DiagnosticCostBundle, ReplyContext, SupplySelection};
use serde::{Deserialize, Serialize};

fn ability<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    let value = String::deserialize(d)?;
    if !crate::valid_content_id(&value) {
        return Err(serde::de::Error::custom("invalid ability identity"));
    }
    Ok(value)
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AbilityWeaponPreviewPhase {
    Fresh,
    Resuming,
    PaidPrefix,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct AbilityWeaponPreview {
    pub ability: String,
    pub invocation: AbilityInvocation,
    pub supplies: SupplySelection,
    pub phase: AbilityWeaponPreviewPhase,
    pub cost: Option<DiagnosticCostBundle>,
    #[serde(with = "crate::integers::unsigned")]
    pub preparation: u64,
    #[serde(with = "crate::integers::unsigned")]
    pub recovery: u64,
}
impl<'de> Deserialize<'de> for AbilityWeaponPreview {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            #[serde(deserialize_with = "ability")]
            ability: String,
            invocation: AbilityInvocation,
            supplies: SupplySelection,
            phase: AbilityWeaponPreviewPhase,
            #[serde(deserialize_with = "crate::wire::required_option")]
            cost: Option<DiagnosticCostBundle>,
            #[serde(with = "crate::integers::unsigned")]
            preparation: u64,
            #[serde(with = "crate::integers::unsigned")]
            recovery: u64,
        }
        let f: Fields = crate::bounded::named_map(d)?;
        Ok(Self {
            ability: f.ability,
            invocation: f.invocation,
            supplies: f.supplies,
            phase: f.phase,
            cost: f.cost,
            preparation: f.preparation,
            recovery: f.recovery,
        })
    }
}
pub(crate) fn read_command<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<(u64, String, AbilityInvocation, SupplySelection), D::Error> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Fields {
        #[serde(with = "crate::integers::unsigned")]
        expected_revision: u64,
        #[serde(deserialize_with = "ability")]
        ability: String,
        invocation: AbilityInvocation,
        supplies: SupplySelection,
    }
    let f: Fields = crate::bounded::named_map(d)?;
    Ok((f.expected_revision, f.ability, f.invocation, f.supplies))
}
pub(crate) fn read_reply<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<(ReplyContext, String, Box<AbilityWeaponPreview>), D::Error> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Fields {
        #[serde(deserialize_with = "crate::bounded::named_map")]
        context: ReplyContext,
        request_id: String,
        preview: Box<AbilityWeaponPreview>,
    }
    let f: Fields = crate::bounded::named_map(d)?;
    if f.request_id.is_empty()
        || f.request_id.len() > 128
        || f.request_id.chars().any(char::is_control)
    {
        return Err(serde::de::Error::custom("invalid preview request identity"));
    }
    Ok((f.context, f.request_id, f.preview))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::*;
    use serde_json::{json, Value};
    fn preview() -> AbilityWeaponPreview {
        AbilityWeaponPreview {
            ability: "basic_melee".into(),
            invocation: AbilityInvocation {
                aim: AbilityAim::ActorTarget {
                    target: ActorTarget::from_digest([1; 32]),
                },
                weapon: WeaponSelection::Equipped {
                    item: ItemTarget::from_digest([2; 32]),
                    socket: 63,
                },
            },
            supplies: SupplySelection::Carried,
            phase: AbilityWeaponPreviewPhase::Fresh,
            cost: None,
            preparation: 300,
            recovery: 100,
        }
    }
    fn context() -> ReplyContext {
        serde_json::from_value(json!({"snapshot":"fixture", "input":{"stream":{"stream":"attachment","epoch":"0"},"readiness_revision":"0"},
            "actor":"1","branch":"main","cursor":{"sequence":"0","tick":"0"},"revision":"0"
        }))
        .unwrap()
    }
    #[test]
    fn preview_schema_requires_named_exact_operands_and_nullable_canonical_cost() {
        let p = preview();
        let value = serde_json::to_value(&p).unwrap();
        assert_eq!(
            serde_json::from_value::<AbilityWeaponPreview>(value.clone()).unwrap(),
            p
        );
        for field in [
            "ability",
            "invocation",
            "supplies",
            "phase",
            "cost",
            "preparation",
            "recovery",
        ] {
            let mut missing = value.clone();
            missing.as_object_mut().unwrap().remove(field);
            assert!(
                serde_json::from_value::<AbilityWeaponPreview>(missing).is_err(),
                "missing {field}"
            );
            let mut duplicate = serde_json::to_string(&value).unwrap();
            duplicate.insert_str(1, &format!("{field:?}:{},", value[field]));
            assert!(
                serde_json::from_str::<AbilityWeaponPreview>(&duplicate).is_err(),
                "duplicate {field}"
            );
        }
        let mut unknown = value.clone();
        unknown["other"] = json!(true);
        assert!(serde_json::from_value::<AbilityWeaponPreview>(unknown).is_err());
        assert!(serde_json::from_value::<AbilityWeaponPreview>(json!([
            "basic_melee",
            p.invocation,
            p.supplies,
            "fresh",
            null,
            "300",
            "100"
        ]))
        .is_err());
        for (pointer, bad) in [
            ("/invocation/weapon/socket", json!(64)),
            (
                "/invocation/weapon/item",
                json!(ActorTarget::from_digest([2; 32])),
            ),
            ("/supplies", json!({"kind":"carried","extra":0})),
            ("/phase", json!("affordable")),
            ("/preparation", json!(300)),
            ("/cost", json!({"stamina":null,"focus":null,"mana":null})),
        ] {
            let mut bad_value = value.clone();
            *bad_value.pointer_mut(pointer).unwrap() = bad;
            assert!(
                serde_json::from_value::<AbilityWeaponPreview>(bad_value).is_err(),
                "{pointer}"
            );
        }
        let command = Command::PreviewAbilityWeapon {
            expected_revision: 0,
            ability: p.ability.clone(),
            invocation: p.invocation,
            supplies: p.supplies.clone(),
        };
        let command = serde_json::to_value(command).unwrap();
        for field in ["expected_revision", "ability", "invocation", "supplies"] {
            let mut missing = command.clone();
            missing.as_object_mut().unwrap().remove(field);
            assert!(
                serde_json::from_value::<Command>(missing).is_err(),
                "command missing {field}"
            );
            let mut duplicate = serde_json::to_string(&command).unwrap();
            duplicate.insert_str(1, &format!("{field:?}:{},", command[field]));
            assert!(
                serde_json::from_str::<Command>(&duplicate).is_err(),
                "command duplicate {field}"
            );
        }
        let mut unknown = command;
        unknown["other"] = Value::Null;
        assert!(serde_json::from_value::<Command>(unknown).is_err());
        let reply = serde_json::to_value(ServerMessage::AbilityWeaponPreview {
            context: context(),
            request_id: "preview".into(),
            preview: Box::new(p),
        })
        .unwrap();
        for field in ["context", "request_id", "preview"] {
            let mut missing = reply.clone();
            missing.as_object_mut().unwrap().remove(field);
            assert!(serde_json::from_value::<ServerMessage>(missing).is_err());
            let mut duplicate = serde_json::to_string(&reply).unwrap();
            duplicate.insert_str(1, &format!("{field:?}:{},", reply[field]));
            assert!(serde_json::from_str::<ServerMessage>(&duplicate).is_err());
        }
        let mut unknown = reply;
        unknown["other"] = Value::Null;
        assert!(serde_json::from_value::<ServerMessage>(unknown).is_err());
    }
    #[test]
    fn maximum_fragment_echo_and_escaped_labels_use_negotiated_response_bound() {
        let mut p = preview();
        p.ability = "a".repeat(60);
        let mut items: Vec<_> = (0..MAX_SUPPLY_FRAGMENTS)
            .map(|index| SelectedSupply {
                cost_ordinal: (index % MAX_SUPPLY_COSTS) as u8,
                item: ItemTarget::from_digest([index as u8; 32]),
            })
            .collect();
        items.sort();
        p.supplies = SupplySelection::Explicit { items };
        p.preparation = u64::MAX;
        p.recovery = u64::MAX;
        let cost = |id: &str| {
            Some(DiagnosticCost {
                resource: ContentRef {
                    id: id.into(),
                    name: "\\\"".repeat(128),
                },
                start: 500_000,
                resolution: 500_000,
            })
        };
        p.cost = Some(DiagnosticCostBundle {
            stamina: cost("stamina"),
            focus: cost("focus"),
            mana: cost("mana"),
        });
        let mut context = context();
        context.input.stream.stream = StreamId("\\".repeat(128));
        context.input.stream.epoch = u64::MAX;
        context.input.readiness_revision = u64::MAX;
        context.actor = ActorId(u64::MAX);
        context.cursor.sequence = u64::MAX;
        context.cursor.tick = u64::MAX;
        context.revision = u64::MAX;
        let request = ClientMessage::Request {
            request_id: "\\".repeat(128),
            request: Box::new(Request::Command {
                context: context.input.clone(),
                branch: context.branch.clone(),
                command: Command::PreviewAbilityWeapon {
                    expected_revision: u64::MAX,
                    ability: p.ability.clone(),
                    invocation: p.invocation,
                    supplies: p.supplies.clone(),
                },
            }),
        };
        let request = encode_bounded_json(&request, MAX_REQUEST_BYTES).unwrap();
        assert!(decode_request(&request).is_ok());
        let reply = ServerMessage::AbilityWeaponPreview {
            context,
            request_id: "\\".repeat(128),
            preview: Box::new(p),
        };
        let encoded = encode_response(&reply, None, MAX_RESPONSE_BYTES)
            .unwrap()
            .text;
        assert!(
            encoded.len() > 4096,
            "a valid complete echo must not be capped at 4 KiB"
        );
        assert_eq!(decode_response(&encoded).unwrap(), reply);
        assert!(encode_response(&reply, None, encoded.len() - 1).is_err());
        assert_eq!(
            encode_response(&reply, None, encoded.len()).unwrap().text,
            encoded
        );
    }
}
