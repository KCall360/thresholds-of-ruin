//! Save-owned schemas and explicit mappings from backend values.
//! Action DTOs own numeric targets and stored variants independently of transport
//! and simulation enums. Remaining shared identity/metadata values use remote
//! derives so wire serializers never define the persisted representation.
use serde::{Deserialize, Serialize};
use tor_protocol as wire;

#[path = "save_scenario.rs"]
pub(crate) mod scenario;

#[derive(Serialize, Deserialize)]
#[serde(remote = "wire::ActorId", transparent)]
pub(crate) struct ActorId(pub u64);

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Direction {
    North,
    East,
    South,
    West,
    NorthEast,
    SouthEast,
    SouthWest,
    NorthWest,
    Up,
    Down,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum Action {
    Attack { target: u64 },
    SetDoor { door: u64, open: bool },
    Move { direction: Direction },
    Take { item: u64, quantity: Option<u64> },
    Drop { item: u64, quantity: Option<u64> },
    Wait,
}

impl Direction {
    pub(crate) fn serialize<S: serde::Serializer>(
        direction: &crate::actions::Direction,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        Serialize::serialize(&Self::from(*direction), serializer)
    }
    pub(crate) fn deserialize<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> Result<crate::actions::Direction, D::Error> {
        <Self as Deserialize>::deserialize(deserializer).map(Into::into)
    }
}
impl From<crate::actions::Direction> for Direction {
    fn from(direction: crate::actions::Direction) -> Self {
        match direction {
            crate::actions::Direction::North => Self::North,
            crate::actions::Direction::East => Self::East,
            crate::actions::Direction::South => Self::South,
            crate::actions::Direction::West => Self::West,
            crate::actions::Direction::NorthEast => Self::NorthEast,
            crate::actions::Direction::SouthEast => Self::SouthEast,
            crate::actions::Direction::SouthWest => Self::SouthWest,
            crate::actions::Direction::NorthWest => Self::NorthWest,
            crate::actions::Direction::Up => Self::Up,
            crate::actions::Direction::Down => Self::Down,
        }
    }
}
impl From<Direction> for crate::actions::Direction {
    fn from(direction: Direction) -> Self {
        match direction {
            Direction::North => Self::North,
            Direction::East => Self::East,
            Direction::South => Self::South,
            Direction::West => Self::West,
            Direction::NorthEast => Self::NorthEast,
            Direction::SouthEast => Self::SouthEast,
            Direction::SouthWest => Self::SouthWest,
            Direction::NorthWest => Self::NorthWest,
            Direction::Up => Self::Up,
            Direction::Down => Self::Down,
        }
    }
}

impl From<&crate::actions::Action> for Action {
    fn from(action: &crate::actions::Action) -> Self {
        use crate::actions::Action as Backend;
        match action {
            Backend::Attack { target } => Self::Attack { target: target.0 },
            Backend::SetDoor { door, open } => Self::SetDoor {
                door: *door,
                open: *open,
            },
            Backend::Move { direction } => Self::Move {
                direction: (*direction).into(),
            },
            Backend::Take { item, quantity } => Self::Take {
                item: *item,
                quantity: *quantity,
            },
            Backend::Drop { item, quantity } => Self::Drop {
                item: *item,
                quantity: *quantity,
            },
            Backend::Wait => Self::Wait,
        }
    }
}
impl From<Action> for crate::actions::Action {
    fn from(action: Action) -> Self {
        match action {
            Action::Attack { target } => Self::Attack {
                target: tor_simulation::ActorId(target),
            },
            Action::SetDoor { door, open } => Self::SetDoor { door, open },
            Action::Move { direction } => Self::Move {
                direction: direction.into(),
            },
            Action::Take { item, quantity } => Self::Take { item, quantity },
            Action::Drop { item, quantity } => Self::Drop { item, quantity },
            Action::Wait => Self::Wait,
        }
    }
}
impl Action {
    pub(crate) fn serialize<S: serde::Serializer>(
        action: &crate::actions::Action,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        Serialize::serialize(&Self::from(action), serializer)
    }
    pub(crate) fn deserialize<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> Result<crate::actions::Action, D::Error> {
        <Self as Deserialize>::deserialize(deserializer).map(Into::into)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(
    remote = "wire::Anchor",
    tag = "type",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub(crate) enum Anchor {
    State {
        revision: u64,
    },
    Entry {
        #[serde(with = "EntryId")]
        id: wire::EntryId,
    },
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "wire::EntryId", transparent)]
pub(crate) struct EntryId(pub String);

#[derive(Serialize, Deserialize)]
#[serde(remote = "wire::BranchId", transparent)]
pub(crate) struct BranchId(pub String);

#[derive(Serialize, Deserialize)]
#[serde(
    remote = "wire::Author",
    tag = "type",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub(crate) enum Author {
    User { user: String },
    Frontend { user: String, component: String },
    Backend { component: String },
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "wire::Audience", rename_all = "snake_case")]
pub(crate) enum Audience {
    Private,
    Actor,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "wire::AnnotationCategory", rename_all = "snake_case")]
pub(crate) enum AnnotationCategory {
    Note,
    Bookmark,
    Explanation,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "wire::ClientSource", rename_all = "snake_case")]
pub(crate) enum ClientSource {
    User,
    Frontend,
}

/// Optional retained-boundary identities retain their stored string/null shape.
pub(crate) mod optional_entry {
    use super::*;
    #[derive(Serialize)]
    #[serde(transparent)]
    struct Borrowed<'a>(#[serde(with = "EntryId")] &'a wire::EntryId);
    #[derive(Deserialize)]
    #[serde(transparent)]
    struct Owned(#[serde(with = "EntryId")] wire::EntryId);
    pub(crate) fn serialize<S: serde::Serializer>(
        value: &Option<wire::EntryId>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        value.as_ref().map(Borrowed).serialize(serializer)
    }
    pub(crate) fn deserialize<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<wire::EntryId>, D::Error> {
        Option::<Owned>::deserialize(deserializer).map(|value| value.map(|value| value.0))
    }
}

/// Revision keys stay numeric actor identities regardless of their wire encoding.
pub(crate) mod revisions {
    use super::*;
    use serde::ser::SerializeMap;
    use std::collections::BTreeMap;
    pub(crate) fn serialize<S: serde::Serializer>(
        values: &BTreeMap<wire::ActorId, u64>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(values.len()))?;
        for (actor, revision) in values {
            map.serialize_entry(&actor.0, revision)?;
        }
        map.end()
    }
    pub(crate) fn deserialize<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> Result<BTreeMap<wire::ActorId, u64>, D::Error> {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = BTreeMap<wire::ActorId, u64>;
            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("stored actor revisions with numeric identity keys")
            }
            fn visit_map<M: serde::de::MapAccess<'de>>(
                self,
                mut map: M,
            ) -> Result<Self::Value, M::Error> {
                let mut values = BTreeMap::new();
                while let Some((actor, revision)) = map.next_entry::<u64, u64>()? {
                    if values.insert(wire::ActorId(actor), revision).is_some() {
                        return Err(serde::de::Error::custom("duplicate stored actor revision"));
                    }
                }
                Ok(values)
            }
        }
        deserializer.deserialize_map(Visitor)
    }
}

pub(crate) mod shared_revisions {
    use super::*;
    use std::collections::BTreeMap;
    pub(crate) fn serialize<S: serde::Serializer>(
        values: &tor_world::Shared<BTreeMap<wire::ActorId, u64>>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        revisions::serialize(values, serializer)
    }
    pub(crate) fn deserialize<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> Result<tor_world::Shared<BTreeMap<wire::ActorId, u64>>, D::Error> {
        revisions::deserialize(deserializer).map(tor_world::Shared::new)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::{Action, Direction};
    use tor_simulation::ActorId;
    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct Stored {
        #[serde(with = "super::Action")]
        action: Action,
    }
    #[test]
    fn stored_actions_keep_numeric_extremes_and_explicit_variant_shapes() {
        let cases = [
            (
                Action::Attack {
                    target: ActorId(u64::MAX),
                },
                r#"{"type":"attack","target":18446744073709551615}"#,
            ),
            (
                Action::SetDoor {
                    door: u64::MAX,
                    open: true,
                },
                r#"{"type":"set_door","door":18446744073709551615,"open":true}"#,
            ),
            (
                Action::Move {
                    direction: Direction::NorthEast,
                },
                r#"{"type":"move","direction":"north_east"}"#,
            ),
            (
                Action::Take {
                    item: u64::MAX,
                    quantity: Some(u64::MAX),
                },
                r#"{"type":"take","item":18446744073709551615,"quantity":18446744073709551615}"#,
            ),
            (
                Action::Drop {
                    item: 9007199254740993,
                    quantity: None,
                },
                r#"{"type":"drop","item":9007199254740993,"quantity":null}"#,
            ),
            (Action::Wait, r#"{"type":"wait"}"#),
        ];
        for (action, expected) in cases {
            let value = Stored { action };
            let encoded = serde_json::to_string(&value).unwrap();
            assert_eq!(encoded, format!("{{\"action\":{expected}}}"));
            assert_eq!(
                super::super::codec::strict::<Stored>(encoded.as_bytes()).unwrap(),
                value
            );
        }
    }
    #[test]
    fn stored_action_rejects_unknown_fields_and_lossy_or_string_numbers() {
        for value in [
            r#"{"action":{"type":"wait","extra":1}}"#,
            r#"{"action":{"type":"attack","target":"18446744073709551615"}}"#,
            r#"{"action":{"type":"take","item":1.5,"quantity":null}}"#,
            r#"{"action":{"type":"take","item":18446744073709551616,"quantity":null}}"#,
        ] {
            assert!(
                super::super::codec::strict::<Stored>(value.as_bytes()).is_err(),
                "{value}"
            );
        }
    }
    fn check_stored<T: serde::de::DeserializeOwned + Serialize>(json: &str) {
        let decoded: T = super::super::codec::strict(json.as_bytes()).unwrap();
        assert_eq!(
            serde_json::to_value(decoded).unwrap(),
            serde_json::from_str::<serde_json::Value>(json).unwrap()
        );
    }

    #[test]
    fn nested_journal_receipts_and_annotations_keep_save_owned_numbers() {
        check_stored::<crate::engine::Record>(
            r#"{
            "entry": {
                "intention_suspensions": [], "intention_ends": [],
                "id": "entry", "branch": "branch", "actor": 18446744073709551615,
                "tick": 18446744073709551615,
                "author": {"type":"user", "user":"owner"}, "audience":"actor",
                "content": {"type":"intention_admitted", "intention":1,
                    "action":{"type":"attack", "target":18446744073709551615}}
            },
            "receipt": {"user":"owner", "frontend":"test", "request_id":"request",
                "actor":18446744073709551615, "branch":"branch",
                "command":{"type":"admit_intention", "expected_revision":18446744073709551615,
                    "action":{"type":"attack", "target":18446744073709551615}}}
        }"#,
        );
        check_stored::<crate::journal::Command>(
            r#"{
            "type":"annotate", "anchor":{"type":"state", "revision":18446744073709551615},
            "text":"note", "source":"user", "audience":"actor", "category":"note"
        }"#,
        );
        check_stored::<crate::journal::IntentionEnd>(
            r#"{
            "actor":18446744073709551615, "intention":1, "kind":"resolved"
        }"#,
        );
        check_stored::<crate::journal::IntentionSuspension>(
            r#"{
            "actor":18446744073709551615, "intention":1
        }"#,
        );
    }

    #[test]
    fn checkpoint_revision_maps_preserve_numeric_keys_and_values() {
        check_stored::<crate::engine::Revisions>(
            r#"{
            "loaded":{"9007199254740993":18446744073709551615},
            "parked":{"18446744073709551615":9007199254740993}
        }"#,
        );
        for invalid in [
            r#"{"loaded":{"1":1,"1":2},"parked":{}}"#,
            r#"{"loaded":{"1":"2"},"parked":{}}"#,
            r#"{"loaded":{"18446744073709551616":1},"parked":{}}"#,
            r#"{"loaded":{},"parked":{},"extra":0}"#,
        ] {
            assert!(
                super::super::codec::strict::<crate::engine::Revisions>(invalid.as_bytes())
                    .is_err(),
                "{invalid}"
            );
        }
    }
    #[test]
    fn metadata_identities_and_all_direction_variants_keep_stored_names() {
        for direction in [
            "north",
            "east",
            "south",
            "west",
            "north_east",
            "south_east",
            "south_west",
            "north_west",
            "up",
            "down",
        ] {
            check_stored::<Stored>(&format!(
                r#"{{"action":{{"type":"move","direction":"{direction}"}}}}"#
            ));
        }
        for target in ["null", r#""entry\\escaped\"identity""#] {
            check_stored::<crate::journal::Command>(&format!(
                r#"{{"type":"wizard","expected_revision":1,"operation":{{"type":"rewind","target":{target}}}}}"#
            ));
        }
        for source in ["user", "frontend"] {
            for audience in ["private", "actor"] {
                for category in ["note", "bookmark", "explanation"] {
                    check_stored::<crate::journal::Command>(&format!(
                        r#"{{"type":"annotate","anchor":{{"type":"entry","id":"identity"}},"text":"note","source":"{source}","audience":"{audience}","category":"{category}"}}"#
                    ));
                }
            }
        }
        for author in [
            r#"{"type":"user","user":"owner"}"#,
            r#"{"type":"frontend","user":"owner","component":"client"}"#,
            r#"{"type":"backend","component":"scheduler"}"#,
        ] {
            check_stored::<crate::journal::JournalEntry>(&format!(
                r#"{{"intention_suspensions":[],"intention_ends":[],"id":"entry","branch":"branch","actor":1,"tick":0,"author":{author},"audience":"actor","content":{{"type":"annotation","anchor":{{"type":"state","revision":0}},"category":"note","text":"note"}}}}"#
            ));
        }
    }
}
