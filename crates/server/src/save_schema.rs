//! Save-owned schemas and explicit mappings from backend values.
//! Action DTOs own numeric targets and stored variants independently of transport
//! and simulation enums. Remaining shared identity/metadata values use remote
//! derives so wire serializers never define the persisted representation.
use serde::{Deserialize, Serialize};
use tor_protocol as wire;
#[path = "save_advancement.rs"]
mod advancement;

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

pub(crate) mod optional_direction {
    use super::*;
    pub(crate) fn serialize<S: serde::Serializer>(
        value: &Option<crate::actions::Direction>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        value.map(Direction::from).serialize(serializer)
    }
    pub(crate) fn deserialize<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<crate::actions::Direction>, D::Error> {
        Option::<Direction>::deserialize(deserializer).map(|value| value.map(Into::into))
    }
}

#[derive(Serialize, Deserialize)]
#[serde(
    remote = "crate::journal::IntentionChange",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub(crate) enum IntentionChange {
    Suspended,
    Resumed,
    Cancelled,
    Escaped {
        #[serde(with = "Direction")]
        direction: crate::actions::Direction,
    },
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum Action {
    #[serde(deserialize_with = "advancement::read_growth_source")]
    ChooseGrowthSource {
        #[serde(with = "advancement::basis")]
        basis: Box<tor_simulation::player_advancement::AdvancementBasis>,
        #[serde(with = "advancement::growth_source")]
        source: tor_simulation::earned_growth::GrowthSource,
    },
    #[serde(deserialize_with = "advancement::read_purchase")]
    Purchase {
        #[serde(with = "advancement::basis")]
        basis: Box<tor_simulation::player_advancement::AdvancementBasis>,
        #[serde(with = "advancement::purchase")]
        purchase: tor_simulation::player_advancement::PurchaseOperation,
    },
    #[serde(deserialize_with = "advancement::read_respec")]
    Respec {
        #[serde(with = "advancement::basis")]
        basis: Box<tor_simulation::player_advancement::AdvancementBasis>,
        #[serde(with = "advancement::replacement")]
        replacement: tor_simulation::player_respec::ReplacementOperation,
        supplies: SupplySelection,
    },
    UseAbility {
        #[serde(with = "ability_id")]
        id: tor_simulation::content_ids::AbilityId,
        invocation: AbilityInvocation,
        supplies: SupplySelection,
    },
    Equip {
        item: u64,
        slot: u16,
    },
    Unequip {
        item: u64,
    },
    Drink {
        item: u64,
    },
    SetDoor {
        door: u64,
        open: bool,
    },
    Move {
        direction: Direction,
    },
    Take {
        item: u64,
        quantity: Option<u64>,
    },
    Drop {
        item: u64,
        quantity: Option<u64>,
    },
    Wait,
}

/// A saved content reference, independent of frontend display data.
pub(crate) mod ability_id {
    use serde::{Deserialize, Serialize};
    pub(crate) fn serialize<S: serde::Serializer>(
        id: &tor_simulation::content_ids::AbilityId,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        id.as_str().serialize(serializer)
    }
    pub(crate) fn deserialize<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> Result<tor_simulation::content_ids::AbilityId, D::Error> {
        tor_simulation::content_ids::AbilityId::deserialize(deserializer)
    }
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

/// Save-owned named records do not inherit serde's positional tuple alias.
fn named_map<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    deserializer: D,
) -> Result<T, D::Error> {
    struct Named<T>(std::marker::PhantomData<T>);
    impl<'de, T: Deserialize<'de>> serde::de::Visitor<'de> for Named<T> {
        type Value = T;
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("a named stored record")
        }
        fn visit_map<A: serde::de::MapAccess<'de>>(self, map: A) -> Result<T, A::Error> {
            T::deserialize(serde::de::value::MapAccessDeserializer::new(map))
        }
    }
    deserializer.deserialize_map(Named(std::marker::PhantomData))
}
#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum SupplySelection {
    Carried,
    Explicit { items: Vec<SelectedSupply> },
}
impl<'de> Deserialize<'de> for SupplySelection {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            Carried {},
            Explicit {
                #[serde(deserialize_with = "read_supply_items")]
                items: Vec<SelectedSupply>,
            },
        }
        Ok(match Fields::deserialize(deserializer)? {
            Fields::Carried {} => Self::Carried,
            Fields::Explicit { items } => Self::Explicit { items },
        })
    }
}
#[derive(Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SelectedSupply {
    cost_ordinal: u8,
    item: u64,
}
impl<'de> Deserialize<'de> for SelectedSupply {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            cost_ordinal: u8,
            item: u64,
        }
        let value: Fields = named_map(deserializer)?;
        Ok(Self {
            cost_ordinal: value.cost_ordinal,
            item: value.item,
        })
    }
}
fn read_supply_items<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<SelectedSupply>, D::Error> {
    struct Rows;
    impl<'de> serde::de::Visitor<'de> for Rows {
        type Value = Vec<SelectedSupply>;
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("1..32 named supply rows")
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(
            self,
            mut access: A,
        ) -> Result<Self::Value, A::Error> {
            use serde::de::Error;
            if access.size_hint().is_some_and(|n| n > 32) {
                return Err(A::Error::custom("too many supply rows"));
            }
            let mut rows = Vec::new();
            while rows.len() < 32 {
                let Some(row) = access.next_element::<SelectedSupply>()? else {
                    return if rows.is_empty() {
                        Err(A::Error::custom("empty supply rows"))
                    } else {
                        Ok(rows)
                    };
                };
                if row.cost_ordinal >= 8
                    || row.item == 0
                    || row.item == u64::MAX
                    || rows.last().is_some_and(|prior: &SelectedSupply| {
                        (prior.cost_ordinal, prior.item) >= (row.cost_ordinal, row.item)
                    })
                    || rows.iter().any(|prior| prior.item == row.item)
                {
                    return Err(A::Error::custom("invalid supply row"));
                }
                rows.push(row);
            }
            if rows.is_empty() || access.next_element::<serde::de::IgnoredAny>()?.is_some() {
                return Err(A::Error::custom("invalid supply row count"));
            }
            Ok(rows)
        }
    }
    deserializer.deserialize_seq(Rows)
}
impl From<&crate::actions::SupplySelection> for SupplySelection {
    fn from(value: &crate::actions::SupplySelection) -> Self {
        match value {
            crate::actions::SupplySelection::Carried => Self::Carried,
            crate::actions::SupplySelection::Explicit { items } => Self::Explicit {
                items: items
                    .iter()
                    .map(|row| SelectedSupply {
                        cost_ordinal: row.cost_ordinal,
                        item: row.item,
                    })
                    .collect(),
            },
        }
    }
}
impl From<SupplySelection> for crate::actions::SupplySelection {
    fn from(value: SupplySelection) -> Self {
        match value {
            SupplySelection::Carried => Self::Carried,
            SupplySelection::Explicit { items } => Self::Explicit {
                items: items
                    .into_iter()
                    .map(|row| crate::actions::SelectedSupply {
                        cost_ordinal: row.cost_ordinal,
                        item: row.item,
                    })
                    .collect(),
            },
        }
    }
}

impl From<&crate::actions::Action> for Action {
    fn from(action: &crate::actions::Action) -> Self {
        use crate::actions::Action as Backend;
        match action {
            Backend::ChooseGrowthSource { basis, source } => Self::ChooseGrowthSource {
                basis: basis.clone(),
                source: source.clone(),
            },
            Backend::Purchase { basis, purchase } => Self::Purchase {
                basis: basis.clone(),
                purchase: purchase.clone(),
            },
            Backend::Respec {
                basis,
                replacement,
                supplies,
            } => Self::Respec {
                basis: basis.clone(),
                replacement: replacement.clone(),
                supplies: SupplySelection::from(supplies),
            },
            Backend::UseAbility {
                id,
                invocation,
                supplies,
            } => Self::UseAbility {
                id: id.clone(),
                invocation: (*invocation).into(),
                supplies: SupplySelection::from(supplies),
            },
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
            Backend::Equip { item, slot } => Self::Equip {
                item: *item,
                slot: *slot,
            },
            Backend::Unequip { item } => Self::Unequip { item: *item },
            Backend::Drink { item } => Self::Drink { item: *item },
            Backend::Wait => Self::Wait,
        }
    }
}
impl From<Action> for crate::actions::Action {
    fn from(action: Action) -> Self {
        match action {
            Action::ChooseGrowthSource { basis, source } => {
                Self::ChooseGrowthSource { basis, source }
            }
            Action::Purchase { basis, purchase } => Self::Purchase { basis, purchase },
            Action::Respec {
                basis,
                replacement,
                supplies,
            } => Self::Respec {
                basis,
                replacement,
                supplies: supplies.into(),
            },
            Action::UseAbility {
                id,
                invocation,
                supplies,
            } => Self::UseAbility {
                id,
                invocation: invocation.into(),
                supplies: supplies.into(),
            },
            Action::SetDoor { door, open } => Self::SetDoor { door, open },
            Action::Move { direction } => Self::Move {
                direction: direction.into(),
            },
            Action::Take { item, quantity } => Self::Take { item, quantity },
            Action::Drop { item, quantity } => Self::Drop { item, quantity },
            Action::Equip { item, slot } => Self::Equip { item, slot },
            Action::Unequip { item } => Self::Unequip { item },
            Action::Drink { item } => Self::Drink { item },
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
    fn saved_invocations_keep_each_direction_and_numeric_actor_independently_of_wire() {
        use crate::actions::AbilityInvocation;
        let id = tor_simulation::content_ids::AbilityId::new("arbitrary_authored_name").unwrap();
        for (direction, spelling) in [
            (Direction::North, "north"),
            (Direction::East, "east"),
            (Direction::South, "south"),
            (Direction::West, "west"),
            (Direction::NorthEast, "north_east"),
            (Direction::SouthEast, "south_east"),
            (Direction::SouthWest, "south_west"),
            (Direction::NorthWest, "north_west"),
            (Direction::Up, "up"),
            (Direction::Down, "down"),
        ] {
            let value = Stored {
                action: Action::UseAbility {
                    id: id.clone(),
                    invocation: AbilityInvocation::SelfStep { direction },
                    supplies: Default::default(),
                },
            };
            let encoded = serde_json::to_string(&value).unwrap();
            assert_eq!(
                encoded,
                format!(
                    r#"{{"action":{{"type":"use_ability","id":"arbitrary_authored_name","invocation":{{"kind":"self_step","direction":"{spelling}"}},"supplies":{{"kind":"carried"}}}}}}"#
                )
            );
            assert_eq!(
                crate::storage::codec::strict::<Stored>(encoded.as_bytes()).unwrap(),
                value
            );
        }
        for target in [1, 9_007_199_254_740_993, u64::MAX] {
            let value = Stored {
                action: Action::UseAbility {
                    id: id.clone(),
                    invocation: AbilityInvocation::ActorTarget {
                        target: ActorId(target),
                    },
                    supplies: Default::default(),
                },
            };
            let encoded = serde_json::to_string(&value).unwrap();
            assert!(encoded.contains(&format!(
                r#""invocation":{{"kind":"actor_target","target":{target}}}"#
            )));
            assert_eq!(
                crate::storage::codec::strict::<Stored>(encoded.as_bytes()).unwrap(),
                value
            );
        }
    }

    #[test]
    fn saved_invocations_reject_legacy_positional_duplicate_and_cross_variant_operands() {
        for invocation in [
            r#"["self_step","north"]"#,
            r#"{"kind":"self_step"}"#,
            r#"{"kind":"self_step","direction":"north_up"}"#,
            r#"{"kind":"self_step","direction":{"north":null}}"#,
            r#"{"kind":"self_step","direction":["north"]}"#,
            r#"{"kind":"self_step","direction":null}"#,
            r#"{"kind":"self_step","direction":0}"#,
            r#"{"kind":"self_step","direction":"north","direction":"south"}"#,
            r#"{"kind":"self_step","kind":"actor_target","direction":"north"}"#,
            r#"{"kind":"self_step","direction":"north","target":2}"#,
            r#"{"kind":"actor_target","target":2,"direction":"north"}"#,
            r#"{"kind":"actor_target","target":"2"}"#,
            r#"{"kind":"actor_target","target":2,"target":3}"#,
            r#"{"kind":"actor_target","target":{"type":"self_actor"}}"#,
            r#"{"kind":"self_step","direction":"north","destination":{"region":1,"position":{"x":1,"y":2,"z":0}}}"#,
        ] {
            let encoded = format!(
                r#"{{"action":{{"type":"use_ability","id":"arbitrary_authored_name","invocation":{invocation},"supplies":{{"kind":"carried"}}}}}}"#
            );
            assert!(
                crate::storage::codec::strict::<Stored>(encoded.as_bytes()).is_err(),
                "{encoded}"
            );
        }
        for encoded in [
            r#"{"action":{"type":"use_ability","id":"arbitrary_authored_name","target":2,"supplies":{"kind":"carried"}}}"#,
            r#"{"action":{"type":"use_ability","id":"arbitrary_authored_name","invocation":{"kind":"self_step","direction":"north"},"invocation":{"kind":"self_step","direction":"south"},"supplies":{"kind":"carried"}}}"#,
        ] {
            assert!(crate::storage::codec::strict::<Stored>(encoded.as_bytes()).is_err());
        }
    }
    #[test]
    fn stored_actions_keep_numeric_extremes_and_explicit_variant_shapes() {
        let cases = [
            (
                Action::Equip {
                    item: u64::MAX,
                    slot: u16::MAX,
                },
                r#"{"type":"equip","item":18446744073709551615,"slot":65535}"#,
            ),
            (
                Action::Unequip { item: u64::MAX },
                r#"{"type":"unequip","item":18446744073709551615}"#,
            ),
            (
                Action::Drink { item: u64::MAX },
                r#"{"type":"drink","item":18446744073709551615}"#,
            ),
            (
                Action::UseAbility {
                    id: tor_simulation::content_ids::AbilityId::new("basic_melee").unwrap(),
                    invocation: crate::actions::AbilityInvocation::ActorTarget {
                        target: ActorId(u64::MAX),
                    },
                    supplies: Default::default(),
                },
                r#"{"type":"use_ability","id":"basic_melee","invocation":{"kind":"actor_target","target":18446744073709551615},"supplies":{"kind":"carried"}}"#,
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
            r#"{"action":{"type":"use_ability","ability":"basic_melee","target":1}}"#,
            r#"{"action":{"type":"use_ability","id":"fear","target":"1"}}"#,
            r#"{"action":{"type":"wait","extra":1}}"#,
            r#"{"action":{"type":"attack","target":1}}"#,
            r#"{"action":{"type":"use_ability","id":"basic_melee","target":"18446744073709551615"}}"#,
            r#"{"action":{"type":"take","item":1.5,"quantity":null}}"#,
            r#"{"action":{"type":"take","item":18446744073709551616,"quantity":null}}"#,
        ] {
            assert!(
                super::super::codec::strict::<Stored>(value.as_bytes()).is_err(),
                "{value}"
            );
        }
    }
    #[test]
    fn stored_ability_actions_preserve_numeric_targets_independently_of_wire_tokens() {
        for name in [
            "basic_melee",
            "power_strike",
            "magic_bolt",
            "fear",
            "fixture_ability",
        ] {
            let value = Stored {
                action: crate::actions::Action::UseAbility {
                    id: tor_simulation::content_ids::AbilityId::new(name).unwrap(),
                    invocation: crate::actions::AbilityInvocation::ActorTarget {
                        target: tor_simulation::ActorId(u64::MAX),
                    },
                    supplies: Default::default(),
                },
            };
            let encoded = serde_json::to_string(&value).unwrap();
            assert_eq!(encoded, format!("{{\"action\":{{\"type\":\"use_ability\",\"id\":\"{name}\",\"invocation\":{{\"kind\":\"actor_target\",\"target\":18446744073709551615}},\"supplies\":{{\"kind\":\"carried\"}}}}}}"));
            assert_eq!(
                super::super::codec::strict::<Stored>(encoded.as_bytes()).unwrap(),
                value
            );
        }
    }
    #[test]
    fn phase5_saved_selection_uses_required_bounded_numeric_source_rows() {
        let original = serde_json::json!({"action":{"type":"use_ability","id":"bind_wounds","invocation":{"kind":"actor_target","target":1},
            "supplies":{"kind":"explicit","items":[{"cost_ordinal":0,"item":10}]}}});
        let decoded: Stored = serde_json::from_value(original.clone()).unwrap();
        assert_eq!(serde_json::to_value(decoded).unwrap(), original);
        for mutation in 0..7 {
            let mut value = original.clone();
            match mutation {
                0 => {
                    value["action"].as_object_mut().unwrap().remove("supplies");
                }
                1 => value["action"]["supplies"]["items"][0]["item"] = serde_json::json!("10"),
                2 => value["action"]["supplies"]["items"][0]["cost_ordinal"] = serde_json::json!(8),
                3 => value["action"]["supplies"]["items"] = serde_json::json!([]),
                4 => {
                    value["action"]["supplies"]["items"] =
                        serde_json::json!(vec![serde_json::json!({"cost_ordinal":0,"item":10}); 33])
                }
                5 => value["action"]["supplies"]["items"][0]["held"] = serde_json::json!(1),
                6 => value["action"]["supplies"]["items"][0] = serde_json::json!([0, 10]),
                _ => unreachable!(),
            }
            assert!(
                serde_json::from_value::<Stored>(value).is_err(),
                "accepted corruption {mutation}"
            );
        }
    }

    #[test]
    fn phase5_saved_carried_selection_rejects_surplus_rows() {
        assert!(serde_json::from_value::<SupplySelection>(
            serde_json::json!({"kind":"carried","items":[]})
        )
        .is_err());
    }
    #[test]
    fn phase5_saved_selection_cannot_reuse_one_exact_item_across_costs() {
        assert!(serde_json::from_value::<SupplySelection>(
            serde_json::json!({"kind":"explicit","items":[
                {"cost_ordinal":0,"item":10},{"cost_ordinal":1,"item":10}
            ]})
        )
        .is_err());
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
            "activations": [],
            "entry": {
                "intention_suspensions": [], "intention_ends": [],
                "id": "entry", "branch": "branch", "actor": 18446744073709551615,
                "tick": 18446744073709551615,
                "author": {"type":"user", "user":"owner"}, "audience":"actor",
                "content": {"type":"intention_admitted", "intention":1,
                    "action":{"type":"use_ability", "id":"basic_melee", "invocation":{"kind":"actor_target","target":18446744073709551615},"supplies":{"kind":"carried"}}}
            },
            "receipt": {"user":"owner", "frontend":"test", "request_id":"request",
                "actor":18446744073709551615, "branch":"branch",
                "command":{"type":"admit_intention", "expected_revision":18446744073709551615,
                    "action":{"type":"use_ability", "id":"basic_melee", "invocation":{"kind":"actor_target","target":18446744073709551615},"supplies":{"kind":"carried"}}}}
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
#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum AbilityInvocation {
    ActorTarget { target: u64 },
    SelfStep { direction: Direction },
}
impl<'de> Deserialize<'de> for AbilityInvocation {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // Unit-enum deserialization alone also accepts an externally tagged
        // object. Selected directions are the documented string token only.
        fn direction_token<'de, D: serde::Deserializer<'de>>(
            deserializer: D,
        ) -> Result<Direction, D::Error> {
            let token = String::deserialize(deserializer)?;
            <Direction as Deserialize>::deserialize(
                serde::de::value::StringDeserializer::<D::Error>::new(token),
            )
        }
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            ActorTarget {
                target: u64,
            },
            SelfStep {
                #[serde(deserialize_with = "direction_token")]
                direction: Direction,
            },
        }
        Ok(match named_map(deserializer)? {
            Fields::ActorTarget { target } => Self::ActorTarget { target },
            Fields::SelfStep { direction } => Self::SelfStep { direction },
        })
    }
}
impl From<crate::actions::AbilityInvocation> for AbilityInvocation {
    fn from(value: crate::actions::AbilityInvocation) -> Self {
        match value {
            crate::actions::AbilityInvocation::ActorTarget { target } => {
                Self::ActorTarget { target: target.0 }
            }
            crate::actions::AbilityInvocation::SelfStep { direction } => Self::SelfStep {
                direction: direction.into(),
            },
        }
    }
}
impl From<AbilityInvocation> for crate::actions::AbilityInvocation {
    fn from(value: AbilityInvocation) -> Self {
        match value {
            AbilityInvocation::ActorTarget { target } => Self::ActorTarget {
                target: tor_simulation::ActorId(target),
            },
            AbilityInvocation::SelfStep { direction } => Self::SelfStep {
                direction: direction.into(),
            },
        }
    }
}
