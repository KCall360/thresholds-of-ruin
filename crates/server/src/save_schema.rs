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
    Escape {
        hold: tor_simulation::HoldId,
    },
    #[serde(deserialize_with = "advancement::read_growth_source")]
    ChooseGrowthSource {
        #[serde(with = "advancement::basis")]
        basis: Box<tor_simulation::player_advancement::AdvancementBasis>,
        #[serde(with = "advancement::growth_source")]
        source: tor_simulation::earned_growth::GrowthSource,
    },
    #[serde(deserialize_with = "advancement::read_intellect_expertise")]
    ChooseIntellectExpertise {
        #[serde(with = "advancement::basis")]
        basis: Box<tor_simulation::player_advancement::AdvancementBasis>,
        #[serde(with = "advancement::intellect_expertise")]
        choice: tor_simulation::player_advancement::IntellectExpertiseOperation,
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
    Ignite {
        item: u64,
    },
    Extinguish {
        item: u64,
    },
    Drink {
        item: u64,
    },
    Read {
        item: u64,
    },
    Zap {
        item: u64,
        aim: DeviceAim,
    },
    Identify {
        choice: tor_simulation::IntentionId,
        #[serde(deserialize_with = "required_option")]
        item: Option<u64>,
    },
    Eat {
        item: u64,
    },
    ExamineTrap {
        trap: u64,
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
    TouchApparentObject {
        subject: u64,
        #[serde(deserialize_with = "required_option")]
        quantity: Option<u64>,
    },
    Drop {
        item: u64,
        quantity: Option<u64>,
    },
    Wait,
}

fn required_option<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    deserializer: D,
) -> Result<Option<T>, D::Error> {
    Option::<T>::deserialize(deserializer)
}

/// Save-owned spelling: later simulation or transport serde changes cannot
/// rewrite the historical identity domain attached to an accepted preparation.
pub(crate) mod actor_reference_kind {
    use serde::{Deserialize, Serialize};
    use tor_simulation::perception::ActorReferenceKind;

    pub(crate) fn serialize<S: serde::Serializer>(
        kind: &ActorReferenceKind,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        match kind {
            ActorReferenceKind::Creature => "creature",
            ActorReferenceKind::Contact => "contact",
        }
        .serialize(serializer)
    }
    pub(crate) fn deserialize<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> Result<ActorReferenceKind, D::Error> {
        match String::deserialize(deserializer)?.as_str() {
            "creature" => Ok(ActorReferenceKind::Creature),
            "contact" => Ok(ActorReferenceKind::Contact),
            _ => Err(serde::de::Error::custom(
                "invalid saved actor reference kind",
            )),
        }
    }
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
pub(crate) fn named_map<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
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
            Backend::Escape { hold } => Self::Escape { hold: *hold },
            Backend::ChooseGrowthSource { basis, source } => Self::ChooseGrowthSource {
                basis: basis.clone(),
                source: source.clone(),
            },
            Backend::ChooseIntellectExpertise { basis, choice } => Self::ChooseIntellectExpertise {
                basis: basis.clone(),
                choice: choice.clone(),
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
            Backend::ExamineTrap { trap } => Self::ExamineTrap { trap: *trap },
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
            Backend::TouchApparentObject { subject, quantity } => Self::TouchApparentObject {
                subject: subject.0,
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
            Backend::Ignite { item } => Self::Ignite { item: *item },
            Backend::Extinguish { item } => Self::Extinguish { item: *item },
            Backend::Drink { item } => Self::Drink { item: *item },
            Backend::Read { item } => Self::Read { item: *item },
            Backend::Zap { item, aim } => Self::Zap {
                item: *item,
                aim: (*aim).into(),
            },
            Backend::Identify { choice, item } => Self::Identify {
                choice: *choice,
                item: *item,
            },
            Backend::Eat { item } => Self::Eat { item: *item },
            Backend::Wait => Self::Wait,
        }
    }
}
impl From<Action> for crate::actions::Action {
    fn from(action: Action) -> Self {
        match action {
            Action::Escape { hold } => Self::Escape { hold },
            Action::ChooseGrowthSource { basis, source } => {
                Self::ChooseGrowthSource { basis, source }
            }
            Action::ChooseIntellectExpertise { basis, choice } => {
                Self::ChooseIntellectExpertise { basis, choice }
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
            Action::ExamineTrap { trap } => Self::ExamineTrap { trap },
            Action::SetDoor { door, open } => Self::SetDoor { door, open },
            Action::Move { direction } => Self::Move {
                direction: direction.into(),
            },
            Action::Take { item, quantity } => Self::Take { item, quantity },
            Action::TouchApparentObject { subject, quantity } => Self::TouchApparentObject {
                subject: tor_simulation::ActorId(subject),
                quantity,
            },
            Action::Drop { item, quantity } => Self::Drop { item, quantity },
            Action::Equip { item, slot } => Self::Equip { item, slot },
            Action::Unequip { item } => Self::Unequip { item },
            Action::Ignite { item } => Self::Ignite { item },
            Action::Extinguish { item } => Self::Extinguish { item },
            Action::Drink { item } => Self::Drink { item },
            Action::Read { item } => Self::Read { item },
            Action::Zap { item, aim } => Self::Zap {
                item,
                aim: aim.into(),
            },
            Action::Identify { choice, item } => Self::Identify { choice, item },
            Action::Eat { item } => Self::Eat { item },
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
        snapshot: wire::SnapshotId,
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

#[derive(Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AbilityInvocation {
    aim: AbilityAim,
    weapon: WeaponSelection,
}
impl<'de> Deserialize<'de> for AbilityInvocation {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            aim: AbilityAim,
            weapon: WeaponSelection,
        }
        let Fields { aim, weapon } = named_map(deserializer)?;
        if matches!(aim, AbilityAim::DirectionalProjectile { .. })
            && !matches!(weapon, WeaponSelection::Default)
        {
            return Err(serde::de::Error::custom(
                "projectile delivery is the sole weapon selector",
            ));
        }
        Ok(Self { aim, weapon })
    }
}
#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum WeaponSelection {
    Default,
    Equipped { item: u64, socket: u16 },
}
impl<'de> Deserialize<'de> for WeaponSelection {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            Default {},
            Equipped { item: u64, socket: u16 },
        }
        Ok(match named_map(deserializer)? {
            Fields::Default {} => Self::Default,
            Fields::Equipped { item, socket } => {
                if usize::from(socket) >= tor_simulation::limits::EQUIPMENT_SOCKETS
                    || item == 0
                    || item == u64::MAX
                {
                    return Err(serde::de::Error::custom(
                        "invalid equipped weapon selection",
                    ));
                }
                Self::Equipped { item, socket }
            }
        })
    }
}
impl From<crate::actions::AbilityInvocation> for AbilityInvocation {
    fn from(value: crate::actions::AbilityInvocation) -> Self {
        Self {
            aim: value.aim.into(),
            weapon: match value.weapon {
                crate::actions::WeaponSelection::Default => WeaponSelection::Default,
                crate::actions::WeaponSelection::Equipped { item, socket } => {
                    WeaponSelection::Equipped { item, socket }
                }
            },
        }
    }
}
impl From<AbilityInvocation> for crate::actions::AbilityInvocation {
    fn from(value: AbilityInvocation) -> Self {
        Self {
            aim: value.aim.into(),
            weapon: match value.weapon {
                WeaponSelection::Default => crate::actions::WeaponSelection::Default,
                WeaponSelection::Equipped { item, socket } => {
                    crate::actions::WeaponSelection::Equipped { item, socket }
                }
            },
        }
    }
}
#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum AbilityAim {
    ActorTarget {
        target: u64,
    },
    ContactTarget {
        target: u64,
    },
    SelfStep {
        direction: Direction,
    },
    DirectionalRay {
        direction: Direction,
    },
    DirectionalProjectile {
        direction: Direction,
        delivery: ProjectileDeliverySelection,
    },
}
impl<'de> Deserialize<'de> for AbilityAim {
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
            ContactTarget {
                target: u64,
            },
            SelfStep {
                #[serde(deserialize_with = "direction_token")]
                direction: Direction,
            },
            DirectionalRay {
                #[serde(deserialize_with = "direction_token")]
                direction: Direction,
            },
            DirectionalProjectile {
                #[serde(deserialize_with = "direction_token")]
                direction: Direction,
                delivery: ProjectileDeliverySelection,
            },
        }
        Ok(match named_map(deserializer)? {
            Fields::ActorTarget { target } => Self::ActorTarget { target },
            Fields::ContactTarget { target } => Self::ContactTarget { target },
            Fields::SelfStep { direction } => Self::SelfStep { direction },
            Fields::DirectionalRay { direction } => Self::DirectionalRay { direction },
            Fields::DirectionalProjectile {
                direction,
                delivery,
            } => Self::DirectionalProjectile {
                direction,
                delivery,
            },
        })
    }
}
impl From<crate::actions::AbilityAim> for AbilityAim {
    fn from(value: crate::actions::AbilityAim) -> Self {
        match value {
            crate::actions::AbilityAim::ActorTarget { target } => {
                Self::ActorTarget { target: target.0 }
            }
            crate::actions::AbilityAim::ContactTarget { target } => {
                Self::ContactTarget { target: target.0 }
            }
            crate::actions::AbilityAim::DirectionalProjectile {
                direction,
                delivery,
            } => Self::DirectionalProjectile {
                direction: direction.into(),
                delivery: delivery.into(),
            },
            crate::actions::AbilityAim::SelfStep { direction } => Self::SelfStep {
                direction: direction.into(),
            },
            crate::actions::AbilityAim::DirectionalRay { direction } => Self::DirectionalRay {
                direction: direction.into(),
            },
        }
    }
}
impl From<AbilityAim> for crate::actions::AbilityAim {
    fn from(value: AbilityAim) -> Self {
        match value {
            AbilityAim::ActorTarget { target } => Self::ActorTarget {
                target: tor_simulation::ActorId(target),
            },
            AbilityAim::ContactTarget { target } => Self::ContactTarget {
                target: tor_simulation::ActorId(target),
            },
            AbilityAim::DirectionalProjectile {
                direction,
                delivery,
            } => Self::DirectionalProjectile {
                direction: direction.into(),
                delivery: delivery.into(),
            },
            AbilityAim::SelfStep { direction } => Self::SelfStep {
                direction: direction.into(),
            },
            AbilityAim::DirectionalRay { direction } => Self::DirectionalRay {
                direction: direction.into(),
            },
        }
    }
}

fn valid_projectile_item(item: u64) -> bool {
    item != 0 && item != u64::MAX
}
fn valid_projectile_socket(socket: u16) -> bool {
    usize::from(socket) < tor_simulation::limits::EQUIPMENT_SOCKETS
}

/// The selected equipped launcher, with no implicit primary-weapon fallback.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct EquippedProjectileWeapon {
    pub item: u64,
    pub socket: u16,
}
impl EquippedProjectileWeapon {
    pub(crate) fn valid(self) -> bool {
        valid_projectile_item(self.item) && valid_projectile_socket(self.socket)
    }
}
impl<'de> Deserialize<'de> for EquippedProjectileWeapon {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            item: u64,
            socket: u16,
        }
        let fields: Fields = named_map(d)?;
        let result = Self {
            item: fields.item,
            socket: fields.socket,
        };
        if !result.valid() {
            return Err(serde::de::Error::custom(
                "invalid projectile weapon selector",
            ));
        }
        Ok(result)
    }
}

/// Throw an equipped item or a carried stack using the selected socket.
/// The socket remains explicit for either source mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum ThrownSourceSelection {
    Equipped { item: u64, socket: u16 },
    Carried { item: u64, socket: u16 },
}
impl ThrownSourceSelection {
    pub(crate) fn valid(self) -> bool {
        let (Self::Equipped { item, socket } | Self::Carried { item, socket }) = self;
        valid_projectile_item(item) && valid_projectile_socket(socket)
    }
}
impl<'de> Deserialize<'de> for ThrownSourceSelection {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            Equipped { item: u64, socket: u16 },
            Carried { item: u64, socket: u16 },
        }
        let result = match named_map(d)? {
            Fields::Equipped { item, socket } => Self::Equipped { item, socket },
            Fields::Carried { item, socket } => Self::Carried { item, socket },
        };
        if !result.valid() {
            return Err(serde::de::Error::custom("invalid thrown source selector"));
        }
        Ok(result)
    }
}

/// A closed delivery choice for exactly one projectile. Launch always names
/// both the equipped launcher and the carried ammunition stack.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum ProjectileDeliverySelection {
    Throw {
        source: ThrownSourceSelection,
    },
    Launch {
        launcher: EquippedProjectileWeapon,
        ammunition: u64,
    },
}
impl ProjectileDeliverySelection {
    pub(crate) fn valid(self) -> bool {
        match self {
            Self::Throw { source } => source.valid(),
            Self::Launch {
                launcher,
                ammunition,
            } => {
                launcher.valid() && valid_projectile_item(ammunition) && launcher.item != ammunition
            }
        }
    }
}
impl<'de> Deserialize<'de> for ProjectileDeliverySelection {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            Throw {
                source: ThrownSourceSelection,
            },
            Launch {
                launcher: EquippedProjectileWeapon,
                ammunition: u64,
            },
        }
        let result = match named_map(d)? {
            Fields::Throw { source } => Self::Throw { source },
            Fields::Launch {
                launcher,
                ammunition,
            } => Self::Launch {
                launcher,
                ammunition,
            },
        };
        if !result.valid() {
            return Err(serde::de::Error::custom(
                "invalid projectile delivery selector",
            ));
        }
        Ok(result)
    }
}

impl From<crate::actions::ProjectileDeliverySelection> for ProjectileDeliverySelection {
    fn from(value: crate::actions::ProjectileDeliverySelection) -> Self {
        match value {
            crate::actions::ProjectileDeliverySelection::Throw { source } => Self::Throw {
                source: match source {
                    crate::actions::ThrownSourceSelection::Equipped { item, socket } => {
                        ThrownSourceSelection::Equipped { item, socket }
                    }
                    crate::actions::ThrownSourceSelection::Carried { item, socket } => {
                        ThrownSourceSelection::Carried { item, socket }
                    }
                },
            },
            crate::actions::ProjectileDeliverySelection::Launch {
                launcher,
                ammunition,
            } => Self::Launch {
                launcher: EquippedProjectileWeapon {
                    item: launcher.item,
                    socket: launcher.socket,
                },
                ammunition,
            },
        }
    }
}

impl From<ProjectileDeliverySelection> for crate::actions::ProjectileDeliverySelection {
    fn from(value: ProjectileDeliverySelection) -> Self {
        match value {
            ProjectileDeliverySelection::Throw { source } => Self::Throw {
                source: match source {
                    ThrownSourceSelection::Equipped { item, socket } => {
                        crate::actions::ThrownSourceSelection::Equipped { item, socket }
                    }
                    ThrownSourceSelection::Carried { item, socket } => {
                        crate::actions::ThrownSourceSelection::Carried { item, socket }
                    }
                },
            },
            ProjectileDeliverySelection::Launch {
                launcher,
                ammunition,
            } => Self::Launch {
                launcher: crate::actions::EquippedProjectileWeapon {
                    item: launcher.item,
                    socket: launcher.socket,
                },
                ammunition,
            },
        }
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
    fn saved_projectile_operands_are_numeric_exact_and_independent_of_live_items() {
        use serde_json::json;
        for delivery in [
            json!({"kind":"throw","source":{"kind":"equipped","item":9_007_199_254_740_993_u64,"socket":0}}),
            json!({"kind":"throw","source":{"kind":"carried","item":9_007_199_254_740_995_u64,"socket":63}}),
            json!({"kind":"launch","launcher":{"item":9_007_199_254_740_993_u64,"socket":0},"ammunition":9_007_199_254_740_995_u64}),
        ] {
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
                let value = json!({"action":{"type":"use_ability","id":"fixture_projectile",
                    "invocation":{"aim":{"kind":"directional_projectile","direction":direction,"delivery":delivery},"weapon":{"kind":"default"}},"supplies":{"kind":"carried"}}});
                let stored: Stored =
                    crate::storage::codec::strict(&serde_json::to_vec(&value).unwrap()).unwrap();
                assert_eq!(serde_json::to_value(&stored).unwrap(), value);
                assert_eq!(
                    crate::adapt::recorded_action(crate::adapt::action(&stored.action)),
                    Some(stored.action)
                );
            }
        }
    }

    #[test]
    fn saved_projectile_selector_codec_rejects_missing_extra_positional_and_conflicting_operands() {
        use serde_json::json;
        let good = json!({"aim":{"kind":"directional_projectile","direction":"north","delivery":{"kind":"launch","launcher":{"item":1,"socket":0},"ammunition":2}},"weapon":{"kind":"default"}});
        assert!(serde_json::from_value::<AbilityInvocation>(good.clone()).is_ok());
        let mut bad = Vec::new();
        for path in ["", "/aim", "/aim/delivery", "/aim/delivery/launcher"] {
            let original = good.pointer(path).unwrap().as_object().unwrap();
            for key in original.keys() {
                let mut value = good.clone();
                value
                    .pointer_mut(path)
                    .unwrap()
                    .as_object_mut()
                    .unwrap()
                    .remove(key);
                bad.push(value);
            }
            let mut value = good.clone();
            value.pointer_mut(path).unwrap()["extra"] = json!(null);
            bad.push(value);
            let mut value = good.clone();
            *value.pointer_mut(path).unwrap() = json!(original.values().collect::<Vec<_>>());
            bad.push(value);
        }
        for path in ["/aim/delivery/launcher/item", "/aim/delivery/ammunition"] {
            for item in [
                json!(0),
                json!(u64::MAX),
                json!("1"),
                json!(1.0),
                json!(-1),
                json!(null),
            ] {
                let mut value = good.clone();
                *value.pointer_mut(path).unwrap() = item;
                bad.push(value);
            }
        }
        for socket in [json!(64), json!(65536), json!(-1), json!(0.0), json!("0")] {
            let mut value = good.clone();
            value["aim"]["delivery"]["launcher"]["socket"] = socket;
            bad.push(value);
        }
        let mut value = good.clone();
        value["aim"]["delivery"]["ammunition"] = json!(1);
        bad.push(value);
        let mut value = good.clone();
        value["weapon"] = json!({"kind":"equipped","item":1,"socket":0});
        bad.push(value);
        for direction in [
            json!({"north":null}),
            json!(["north"]),
            json!("north_up"),
            json!(null),
        ] {
            let mut value = good.clone();
            value["aim"]["direction"] = direction;
            bad.push(value);
        }
        for value in bad {
            assert!(
                serde_json::from_value::<AbilityInvocation>(value.clone()).is_err(),
                "{value}"
            );
        }
        let raw = serde_json::to_string(&good).unwrap();
        for (from, to) in [
            ("\"item\":1", "\"item\":1,\"item\":2"),
            ("\"socket\":0", "\"socket\":0,\"socket\":1"),
            ("\"ammunition\":2", "\"ammunition\":2,\"ammunition\":3"),
            (
                "\"kind\":\"launch\"",
                "\"kind\":\"launch\",\"kind\":\"throw\"",
            ),
        ] {
            assert!(serde_json::from_str::<AbilityInvocation>(&raw.replace(from, to)).is_err());
        }
        for kind in ["equipped", "carried"] {
            let source = json!({"kind":kind,"item":1,"socket":0});
            assert!(serde_json::from_value::<ThrownSourceSelection>(source.clone()).is_ok());
            for key in ["kind", "item", "socket"] {
                let mut value = source.clone();
                value.as_object_mut().unwrap().remove(key);
                assert!(serde_json::from_value::<ThrownSourceSelection>(value).is_err());
            }
            for item in [json!(0), json!(u64::MAX), json!("1")] {
                let mut value = source.clone();
                value["item"] = item;
                assert!(serde_json::from_value::<ThrownSourceSelection>(value).is_err());
            }
            let mut value = source.clone();
            value["socket"] = json!(64);
            assert!(serde_json::from_value::<ThrownSourceSelection>(value).is_err());
            let mut value = source.clone();
            value["extra"] = json!(null);
            assert!(serde_json::from_value::<ThrownSourceSelection>(value).is_err());
            assert!(serde_json::from_value::<ThrownSourceSelection>(json!([kind, 1, 0])).is_err());
            let raw = serde_json::to_string(&source).unwrap();
            assert!(serde_json::from_str::<ThrownSourceSelection>(
                &raw.replace("\"item\":1", "\"item\":1,\"item\":2")
            )
            .is_err());
        }
    }

    #[test]
    fn apparent_touch_saved_input_requires_exact_numeric_subject_and_nullable_quantity() {
        for quantity in [None, Some(1), Some(0), Some(2), Some(u64::MAX)] {
            let value = Stored {
                action: Action::TouchApparentObject {
                    subject: ActorId(u64::MAX),
                    quantity,
                },
            };
            let saved = serde_json::json!({"action": {
                "type": "touch_apparent_object", "subject": u64::MAX, "quantity": quantity
            }});
            assert_eq!(serde_json::to_value(&value).unwrap(), saved);
            assert_eq!(
                crate::storage::codec::strict::<Stored>(&serde_json::to_vec(&saved).unwrap())
                    .unwrap(),
                value
            );
        }
        for invalid in [
            r#"{"action":{"type":"touch_apparent_object","subject":7}}"#,
            r#"{"action":{"type":"touch_apparent_object","subject":"7","quantity":null}}"#,
            r#"{"action":{"type":"touch_apparent_object","subject":7.0,"quantity":null}}"#,
            r#"{"action":{"type":"touch_apparent_object","subject":7,"quantity":"1"}}"#,
            r#"{"action":{"type":"touch_apparent_object","subject":7,"quantity":null,"item":7}}"#,
            r#"{"action":{"type":"touch_apparent_object","subject":7,"quantity":null,"reference":"creature"}}"#,
        ] {
            assert!(
                crate::storage::codec::strict::<Stored>(invalid.as_bytes()).is_err(),
                "{invalid}"
            );
        }
    }

    #[test]
    fn saved_crumbled_pickup_keeps_internal_identity_and_no_result_transfer() {
        let saved = serde_json::json!({"type":"crumbled", "item":7, "quantity":2});
        let restored: crate::journal::Event =
            crate::storage::codec::strict(&serde_json::to_vec(&saved).unwrap()).unwrap();
        assert_eq!(serde_json::to_value(restored).unwrap(), saved);
        for invalid in [
            serde_json::json!({"type":"crumbled", "item":"7", "quantity":2}),
            serde_json::json!({"type":"crumbled", "item":7}),
            serde_json::json!({"type":"crumbled", "item":7, "quantity":2, "result":8}),
        ] {
            assert!(crate::storage::codec::strict::<crate::journal::Event>(
                &serde_json::to_vec(&invalid).unwrap()
            )
            .is_err());
        }
    }

    #[test]
    fn saved_ability_started_reference_is_required_and_save_owned() {
        for reference in ["creature", "contact"] {
            let saved = serde_json::json!({"type":"ability_started", "reference":reference,
                "id":"basic_melee", "name":"Basic melee", "target":u64::MAX});
            let restored: crate::journal::Event =
                crate::storage::codec::strict(&serde_json::to_vec(&saved).unwrap()).unwrap();
            assert_eq!(serde_json::to_value(restored).unwrap(), saved);
        }
        for reference in [
            serde_json::Value::Null,
            serde_json::json!("apparent_object"),
            serde_json::json!({"contact":null}),
            serde_json::json!(0),
        ] {
            let invalid = serde_json::json!({"type":"ability_started", "reference":reference,
                "id":"basic_melee", "name":"Basic melee", "target":7});
            assert!(crate::storage::codec::strict::<crate::journal::Event>(
                &serde_json::to_vec(&invalid).unwrap()
            )
            .is_err());
        }
        assert!(crate::storage::codec::strict::<crate::journal::Event>(
            br#"{"type":"ability_started","id":"basic_melee","name":"Basic melee","target":7}"#
        )
        .is_err());
    }

    #[test]
    fn saved_escape_uses_exact_internal_numeric_identity_not_wire_tokens() {
        let value = Stored {
            action: Action::Escape {
                hold: tor_simulation::HoldId(7),
            },
        };
        assert_eq!(
            serde_json::to_value(&value).unwrap(),
            serde_json::json!({"action":{"type":"escape","hold":7}})
        );
        assert_eq!(
            serde_json::from_value::<Stored>(serde_json::to_value(&value).unwrap()).unwrap(),
            value
        );
        for hold in [
            serde_json::json!(0),
            serde_json::json!(u64::MAX),
            serde_json::json!("7"),
            serde_json::json!(format!("h_{}", "09".repeat(32))),
        ] {
            assert!(serde_json::from_value::<Stored>(
                serde_json::json!({"action":{"type":"escape","hold":hold}})
            )
            .is_err());
        }
    }
    #[test]
    fn saved_directional_ray_has_no_actor_or_predicted_path_operand() {
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
            let value = format!(
                r#"{{"action":{{"type":"use_ability","id":"fixture_ray","invocation":{{"aim":{{"kind":"directional_ray","direction":"{direction}"}},"weapon":{{"kind":"default"}}}},"supplies":{{"kind":"carried"}}}}}}"#
            );
            check_stored::<Stored>(&value);
            for extra in [
                r#", "target":null"#,
                r#", "path":[]"#,
                r#", "destination":null"#,
            ] {
                let forged = value.replace(
                    &format!(r#""direction":"{direction}""#),
                    &format!(r#""direction":"{direction}"{extra}"#),
                );
                assert!(crate::storage::codec::strict::<Stored>(forged.as_bytes()).is_err());
            }
        }
    }
    #[test]
    fn saved_invocations_keep_each_direction_and_numeric_actor_independently_of_wire() {
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
                    invocation: crate::actions::AbilityInvocation {
                        aim: crate::actions::AbilityAim::SelfStep { direction },
                        weapon: crate::actions::WeaponSelection::Default,
                    },
                    supplies: Default::default(),
                },
            };
            let encoded = serde_json::to_string(&value).unwrap();
            assert_eq!(
                encoded,
                format!(
                    r#"{{"action":{{"type":"use_ability","id":"arbitrary_authored_name","invocation":{{"aim":{{"kind":"self_step","direction":"{spelling}"}},"weapon":{{"kind":"default"}}}},"supplies":{{"kind":"carried"}}}}}}"#
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
                    invocation: crate::actions::AbilityInvocation {
                        aim: crate::actions::AbilityAim::ActorTarget {
                            target: ActorId(target),
                        },
                        weapon: crate::actions::WeaponSelection::Default,
                    },
                    supplies: Default::default(),
                },
            };
            let encoded = serde_json::to_string(&value).unwrap();
            assert!(encoded.contains(&format!(
                r#""invocation":{{"aim":{{"kind":"actor_target","target":{target}}},"weapon":{{"kind":"default"}}}}"#
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
            r#"{"aim":{"kind":"self_step"},"weapon":{"kind":"default"}}"#,
            r#"{"aim":{"kind":"self_step","direction":"north_up"},"weapon":{"kind":"default"}}"#,
            r#"{"aim":{"kind":"self_step","direction":{"north":null}},"weapon":{"kind":"default"}}"#,
            r#"{"aim":{"kind":"self_step","direction":["north"]},"weapon":{"kind":"default"}}"#,
            r#"{"aim":{"kind":"self_step","direction":null},"weapon":{"kind":"default"}}"#,
            r#"{"aim":{"kind":"self_step","direction":0},"weapon":{"kind":"default"}}"#,
            r#"{"aim":{"kind":"self_step","direction":"north","direction":"south"},"weapon":{"kind":"default"}}"#,
            r#"{"aim":{"kind":"self_step","kind":"actor_target","direction":"north"},"weapon":{"kind":"default"}}"#,
            r#"{"aim":{"kind":"self_step","direction":"north","target":2},"weapon":{"kind":"default"}}"#,
            r#"{"aim":{"kind":"actor_target","target":2,"direction":"north"},"weapon":{"kind":"default"}}"#,
            r#"{"aim":{"kind":"actor_target","target":"2"},"weapon":{"kind":"default"}}"#,
            r#"{"aim":{"kind":"actor_target","target":2,"target":3},"weapon":{"kind":"default"}}"#,
            r#"{"aim":{"kind":"actor_target","target":{"type":"self_actor"}},"weapon":{"kind":"default"}}"#,
            r#"{"aim":{"kind":"self_step","direction":"north","destination":{"region":1,"position":{"x":1,"y":2,"z":0}}},"weapon":{"kind":"default"}}"#,
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
            r#"{"action":{"type":"use_ability","id":"arbitrary_authored_name","invocation":{"aim":{"kind":"self_step","direction":"north"},"weapon":{"kind":"default"}},"invocation":{"aim":{"kind":"self_step","direction":"south"},"weapon":{"kind":"default"}},"supplies":{"kind":"carried"}}}"#,
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
                Action::Ignite { item: u64::MAX },
                r#"{"type":"ignite","item":18446744073709551615}"#,
            ),
            (
                Action::Extinguish { item: u64::MAX },
                r#"{"type":"extinguish","item":18446744073709551615}"#,
            ),
            (
                Action::Drink { item: u64::MAX },
                r#"{"type":"drink","item":18446744073709551615}"#,
            ),
            (
                Action::Eat { item: u64::MAX },
                r#"{"type":"eat","item":18446744073709551615}"#,
            ),
            (
                Action::UseAbility {
                    id: tor_simulation::content_ids::AbilityId::new("basic_melee").unwrap(),
                    invocation: crate::actions::AbilityInvocation {
                        aim: crate::actions::AbilityAim::ActorTarget {
                            target: ActorId(u64::MAX),
                        },
                        weapon: crate::actions::WeaponSelection::Default,
                    },
                    supplies: Default::default(),
                },
                r#"{"type":"use_ability","id":"basic_melee","invocation":{"aim":{"kind":"actor_target","target":18446744073709551615},"weapon":{"kind":"default"}},"supplies":{"kind":"carried"}}"#,
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
    fn stored_device_aim_is_closed_named_and_independent_of_transport() {
        use crate::actions::DeviceAim;
        for aim in [
            DeviceAim::Direction {
                direction: Direction::NorthWest,
            },
            DeviceAim::SelfTarget {},
        ] {
            let stored = Stored {
                action: Action::Zap { item: 71, aim },
            };
            let encoded = serde_json::to_vec(&stored).unwrap();
            assert_eq!(
                crate::storage::codec::strict::<Stored>(&encoded).unwrap(),
                stored
            );
            let value: serde_json::Value = serde_json::from_slice(&encoded).unwrap();
            assert!(value["action"].get("direction").is_none());
            assert!(value["action"]["aim"].is_object());
        }
        for aim in [
            r#""self_target""#,
            r#"["self_target"]"#,
            r#"{"kind":"self_target","direction":"east"}"#,
            r#"{"kind":"self_target","target":1}"#,
            r#"{"kind":"direction"}"#,
            r#"{"kind":"direction","direction":"east","target":1}"#,
            r#"{"kind":"direction","direction":null}"#,
            r#"{"kind":"self_target","kind":"self_target"}"#,
        ] {
            let encoded = format!(r#"{{"action":{{"type":"zap","item":71,"aim":{aim}}}}}"#);
            assert!(
                crate::storage::codec::strict::<Stored>(encoded.as_bytes()).is_err(),
                "{encoded}"
            );
        }
        assert!(crate::storage::codec::strict::<Stored>(
            br#"{"action":{"type":"zap","item":71,"direction":"east"}}"#
        )
        .is_err());
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
                    invocation: crate::actions::AbilityInvocation {
                        aim: crate::actions::AbilityAim::ActorTarget {
                            target: tor_simulation::ActorId(u64::MAX),
                        },
                        weapon: crate::actions::WeaponSelection::Default,
                    },
                    supplies: Default::default(),
                },
            };
            let encoded = serde_json::to_string(&value).unwrap();
            assert_eq!(encoded, format!("{{\"action\":{{\"type\":\"use_ability\",\"id\":\"{name}\",\"invocation\":{{\"aim\":{{\"kind\":\"actor_target\",\"target\":18446744073709551615}},\"weapon\":{{\"kind\":\"default\"}}}},\"supplies\":{{\"kind\":\"carried\"}}}}}}"));
            assert_eq!(
                super::super::codec::strict::<Stored>(encoded.as_bytes()).unwrap(),
                value
            );
        }
    }
    #[test]
    fn phase5_saved_selection_uses_required_bounded_numeric_source_rows() {
        let original = serde_json::json!({"action":{"type":"use_ability","id":"bind_wounds","invocation":{"aim":{"kind":"actor_target","target":1},"weapon":{"kind":"default"}},
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
            "before_tick": 18446744073709551615,
            "after_tick": 18446744073709551615,
            "activations": [],
            "retrievals": [],
            "retrieval_before": null,
            "retrieval_after": null,
            "action_resolution": {"kind":"unchanged"},
            "entry": {
                "intention_suspensions": [], "intention_ends": [],
                "id": "entry", "branch": "branch", "actor": 18446744073709551615,
                "tick": 18446744073709551615,
                "author": {"type":"user", "user":"owner"}, "audience":"actor",
                "content": {"type":"intention_admitted", "intention":1,
                    "action":{"type":"use_ability", "id":"basic_melee", "invocation":{"aim":{"kind":"actor_target","target":18446744073709551615},"weapon":{"kind":"default"}},"supplies":{"kind":"carried"}}}
            },
            "receipt": {"user":"owner", "frontend":"test", "request_id":"request",
                "actor":18446744073709551615, "branch":"branch",
                "command":{"type":"admit_intention", "expected_boundary":18446744073709551615,
                    "action":{"type":"use_ability", "id":"basic_melee", "invocation":{"aim":{"kind":"actor_target","target":18446744073709551615},"weapon":{"kind":"default"}},"supplies":{"kind":"carried"}}}}
        }"#,
        );
        check_stored::<crate::journal::Command>(
            r#"{
            "type":"annotate", "anchor":{"type":"state", "snapshot":"issued-snapshot"},
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
                r#"{{"type":"wizard","expected_boundary":1,"operation":{{"type":"rewind","target":{target}}}}}"#
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
                r#"{{"intention_suspensions":[],"intention_ends":[],"id":"entry","branch":"branch","actor":1,"tick":0,"author":{author},"audience":"actor","content":{{"type":"annotation","anchor":{{"type":"state","snapshot":"issued-snapshot","boundary":null}},"category":"note","text":"note"}}}}"#
            ));
        }
    }
    #[test]
    fn saved_weapon_selection_is_exact_numeric_named_and_independent_of_wire() {
        use crate::actions as backend;
        for (item, socket) in [(1, 0), (9_007_199_254_740_993, 63), (u64::MAX - 1, 1)] {
            let stored = Stored {
                action: backend::Action::UseAbility {
                    id: tor_simulation::content_ids::AbilityId::new("basic_melee").unwrap(),
                    invocation: backend::AbilityInvocation {
                        aim: backend::AbilityAim::ContactTarget { target: ActorId(2) },
                        weapon: backend::WeaponSelection::Equipped { item, socket },
                    },
                    supplies: Default::default(),
                },
            };
            let value = serde_json::to_value(&stored).unwrap();
            assert_eq!(
                value["action"]["invocation"],
                serde_json::json!({
                    "aim":{"kind":"contact_target","target":2},
                    "weapon":{"kind":"equipped","item":item,"socket":socket}
                })
            );
            assert_eq!(
                crate::storage::codec::strict::<Stored>(&serde_json::to_vec(&value).unwrap())
                    .unwrap(),
                stored
            );
        }
        let aim = r#"{"kind":"actor_target","target":2}"#;
        for weapon in [
            r#""default""#,
            r#"["equipped",7,0]"#,
            r#"{"kind":"default","item":7}"#,
            r#"{"kind":"default","socket":0}"#,
            r#"{"kind":"equipped","item":7}"#,
            r#"{"kind":"equipped","socket":0}"#,
            r#"{"kind":"equipped","item":0,"socket":0}"#,
            r#"{"kind":"equipped","item":18446744073709551615,"socket":0}"#,
            r#"{"kind":"equipped","item":7,"socket":64}"#,
            r#"{"kind":"equipped","item":7,"socket":-1}"#,
            r#"{"kind":"equipped","item":7,"socket":0.0}"#,
            r#"{"kind":"equipped","item":7,"socket":"0"}"#,
            r#"{"kind":"equipped","item":"7","socket":0}"#,
            r#"{"kind":"equipped","item":7,"socket":0,"target":2}"#,
            r#"{"kind":"equipped","item":7,"item":8,"socket":0}"#,
            r#"{"kind":"equipped","item":7,"socket":0,"socket":1}"#,
            r#"{"kind":"equipped","kind":"default","item":7,"socket":0}"#,
        ] {
            let encoded = format!(r#"{{"aim":{aim},"weapon":{weapon}}}"#);
            assert!(
                crate::storage::codec::strict::<super::AbilityInvocation>(encoded.as_bytes())
                    .is_err(),
                "{encoded}"
            );
        }
        for encoded in [
            format!(r#"{{"aim":{aim}}}"#),
            r#"{"weapon":{"kind":"default"}}"#.into(),
            format!(r#"{{"aim":{aim},"aim":{aim},"weapon":{{"kind":"default"}}}}"#),
            format!(
                r#"{{"aim":{aim},"weapon":{{"kind":"default"}},"weapon":{{"kind":"default"}}}}"#
            ),
            format!(r#"{{"aim":{aim},"weapon":{{"kind":"default"}},"kind":"actor_target"}}"#),
            format!(r#"[{aim},{{"kind":"default"}}]"#),
            aim.into(),
        ] {
            assert!(
                crate::storage::codec::strict::<super::AbilityInvocation>(encoded.as_bytes())
                    .is_err(),
                "{encoded}"
            );
        }
    }
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum DeviceAim {
    Direction { direction: Direction },
    SelfTarget {},
}
impl<'de> Deserialize<'de> for DeviceAim {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            Direction { direction: Direction },
            SelfTarget {},
        }
        Ok(match named_map(d)? {
            Fields::Direction { direction } => Self::Direction { direction },
            Fields::SelfTarget {} => Self::SelfTarget {},
        })
    }
}
impl From<crate::actions::DeviceAim> for DeviceAim {
    fn from(value: crate::actions::DeviceAim) -> Self {
        match value {
            crate::actions::DeviceAim::Direction { direction } => Self::Direction {
                direction: direction.into(),
            },
            crate::actions::DeviceAim::SelfTarget {} => Self::SelfTarget {},
        }
    }
}
impl From<DeviceAim> for crate::actions::DeviceAim {
    fn from(value: DeviceAim) -> Self {
        match value {
            DeviceAim::Direction { direction } => Self::Direction {
                direction: direction.into(),
            },
            DeviceAim::SelfTarget {} => Self::SelfTarget {},
        }
    }
}
