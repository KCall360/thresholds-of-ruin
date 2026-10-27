//! Standalone strict spatial codec; no Game/save field or canonical source authority.
use super::mapping::{self, MappingCertification, MappingFrames, MappingRoomFrame};
use super::registry::{Budget, CellRun, PrimaryRun, RoomSurfaces, Scope};
use super::{Membership, PassageId, PrimarySpace, RoomId};
use serde::de::{DeserializeSeed, Error as _, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::BTreeSet;
use std::{fmt, marker::PhantomData};
use tor_world::{Direction, RegionId, Shared};

#[derive(Clone, Copy, Debug)]
pub struct DecodeLimits {
    pub charge_bytes: u64,
    pub root_references: u32,
    pub directories: u32,
}
#[derive(Debug)]
pub struct DecodedPool {
    scopes: Vec<Shared<Scope>>,
    directories: Vec<Vec<u32>>,
    limits: DecodeLimits,
    consumed_charge: u64,
}
#[derive(Debug)]
pub struct ValidatedPool {
    scopes: Vec<Shared<Scope>>,
    directories: Vec<Vec<u32>>,
    roots: Vec<u32>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PoolError {
    Structure,
    Capacity,
}
pub struct EncodedPool<'a>(pub &'a ValidatedPool);
pub struct PoolSeed(pub DecodeLimits);

struct Meter {
    used: u64,
    limit: u64,
}
impl Meter {
    fn charge<E: serde::de::Error>(&mut self, amount: u64) -> Result<(), E> {
        let next = self
            .used
            .checked_add(amount)
            .ok_or_else(|| E::custom("spatial charge overflow"))?;
        if next > self.limit {
            return Err(E::custom("spatial charge exceeds limit"));
        }
        self.used = next;
        Ok(())
    }
    fn structural_budget<E: serde::de::Error>(&self) -> Result<Budget, E> {
        let size = |unit| {
            usize::try_from(self.limit / unit).map_err(|_| E::custom("spatial count overflow"))
        };
        Ok(Budget {
            scopes: size(47)?,
            members: size(21)?,
            rooms: size(30)?,
            passages: size(195)?,
            runs: size(107)?,
            label_bytes: size(6)?,
            expanded_refs: (self.limit / 107).saturating_mul(1_u64 << 32),
        })
    }
}

/// Structural count envelope derived from the unchanged archive allowance.
/// This is not a source geometry/expanded-work certificate.
pub fn archive_structural_budget() -> Result<Budget, PoolError> {
    Meter {
        used: 0,
        limit: crate::checkpoint::ARCHIVE_BYTE_LIMIT as u64,
    }
    .structural_budget::<serde::de::value::Error>()
    .map_err(|_| PoolError::Capacity)
}

// Local entry seeds share one allowance. No size_hint allocation, Value tree or
// discarded oversized tail. Prepaid nested primary leaves do not charge twice.
trait Entry: Sized {
    fn read<'de, D: Deserializer<'de>>(d: D, meter: &mut Meter) -> Result<Self, D::Error>;
}
struct EntrySeed<'a, T> {
    meter: &'a mut Meter,
    marker: PhantomData<T>,
}
impl<'de, T: Entry> DeserializeSeed<'de> for EntrySeed<'_, T> {
    type Value = T;
    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<T, D::Error> {
        T::read(d, self.meter)
    }
}
struct ListSeed<'a, T> {
    meter: &'a mut Meter,
    maximum: Option<u32>,
    marker: PhantomData<T>,
}
struct RejectEntry;
impl<'de> DeserializeSeed<'de> for RejectEntry {
    type Value = ();
    fn deserialize<D: Deserializer<'de>>(self, _d: D) -> Result<(), D::Error> {
        Err(D::Error::custom("spatial sequence exceeds limit"))
    }
}
impl<'de, T: Entry> DeserializeSeed<'de> for ListSeed<'_, T> {
    type Value = Vec<T>;
    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<Vec<T>, D::Error> {
        d.deserialize_seq(self)
    }
}
fn read_list<'de, D: Deserializer<'de>, T: Entry>(
    d: D,
    meter: &mut Meter,
) -> Result<Vec<T>, D::Error> {
    ListSeed {
        meter,
        maximum: None,
        marker: PhantomData,
    }
    .deserialize(d)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RunFields {
    region: u64,
    z: i32,
    y: i32,
    first_x: i32,
    last_x: i32,
}
struct NamedRun(CellRun);
impl<'de> Deserialize<'de> for NamedRun {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let v: RunFields = crate::bounded::named_map(d)?;
        let run = CellRun {
            region: RegionId(v.region),
            z: v.z,
            y: v.y,
            first_x: v.first_x,
            last_x: v.last_x,
        };
        if run.expanded_len().is_none() {
            return Err(D::Error::custom("invalid spatial interval"));
        }
        Ok(Self(run))
    }
}
#[derive(Deserialize)]
#[serde(field_identifier, rename_all = "snake_case")]
enum PrimaryField {
    Kind,
    Id,
}
enum SpaceKind {
    Room,
    Passage,
}
impl<'de> Deserialize<'de> for SpaceKind {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Kind;
        impl Visitor<'_> for Kind {
            type Value = SpaceKind;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("room or passage string")
            }
            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<SpaceKind, E> {
                match v {
                    "room" => Ok(SpaceKind::Room),
                    "passage" => Ok(SpaceKind::Passage),
                    _ => Err(E::custom("unknown primary kind")),
                }
            }
        }
        d.deserialize_str(Kind)
    }
}
struct NamedPrimary(PrimarySpace);
impl<'de> Deserialize<'de> for NamedPrimary {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Fields;
        impl<'de> Visitor<'de> for Fields {
            type Value = NamedPrimary;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("named primary space")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<NamedPrimary, A::Error> {
                let (mut kind, mut id) = (None, None);
                while let Some(field) = map.next_key::<PrimaryField>()? {
                    match field {
                        PrimaryField::Kind => {
                            if kind.is_some() {
                                return Err(A::Error::duplicate_field("kind"));
                            }
                            kind = Some(map.next_value::<SpaceKind>()?);
                        }
                        PrimaryField::Id => {
                            if id.is_some() {
                                return Err(A::Error::duplicate_field("id"));
                            }
                            id = Some(map.next_value::<u32>()?);
                        }
                    }
                }
                let id = id.ok_or_else(|| A::Error::missing_field("id"))?;
                let primary = match kind.ok_or_else(|| A::Error::missing_field("kind"))? {
                    SpaceKind::Room => PrimarySpace::Room(
                        RoomId::new(id).ok_or_else(|| A::Error::custom("zero room id"))?,
                    ),
                    SpaceKind::Passage => PrimarySpace::Passage(
                        PassageId::new(id).ok_or_else(|| A::Error::custom("zero passage id"))?,
                    ),
                };
                Ok(NamedPrimary(primary))
            }
        }
        d.deserialize_map(Fields)
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MembershipFields {
    primary: NamedPrimary,
    #[serde(deserialize_with = "crate::bounded::required_option")]
    doorway: Option<u32>,
}
struct NamedMembership(Membership);
impl<'de> Deserialize<'de> for NamedMembership {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let v: MembershipFields = crate::bounded::named_map(d)?;
        let doorway = v
            .doorway
            .map(|id| PassageId::new(id).ok_or_else(|| D::Error::custom("zero doorway passage")))
            .transpose()?;
        Ok(Self(Membership::new(v.primary.0, doorway).ok_or_else(
            || D::Error::custom("passage-owned doorway"),
        )?))
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PrimaryRunFields {
    cells: NamedRun,
    membership: NamedMembership,
}
impl Entry for CellRun {
    fn read<'de, D: Deserializer<'de>>(d: D, m: &mut Meter) -> Result<Self, D::Error> {
        m.charge::<D::Error>(107)?;
        Ok(NamedRun::deserialize(d)?.0)
    }
}
impl Entry for PrimaryRun {
    fn read<'de, D: Deserializer<'de>>(d: D, m: &mut Meter) -> Result<Self, D::Error> {
        m.charge::<D::Error>(195)?;
        let v: PrimaryRunFields = crate::bounded::named_map(d)?;
        Ok(Self {
            cells: v.cells.0,
            membership: v.membership.0,
        })
    }
}
impl Entry for RegionId {
    fn read<'de, D: Deserializer<'de>>(d: D, m: &mut Meter) -> Result<Self, D::Error> {
        m.charge::<D::Error>(21)?;
        u64::deserialize(d).map(Self)
    }
}
impl Entry for u32 {
    fn read<'de, D: Deserializer<'de>>(d: D, m: &mut Meter) -> Result<Self, D::Error> {
        m.charge::<D::Error>(11)?;
        Self::deserialize(d)
    }
}
impl Entry for Vec<u32> {
    fn read<'de, D: Deserializer<'de>>(d: D, m: &mut Meter) -> Result<Self, D::Error> {
        m.charge::<D::Error>(3)?;
        read_list(d, m)
    }
}

struct LabelSeed<'a>(&'a mut Meter);
impl<'de> DeserializeSeed<'de> for LabelSeed<'_> {
    type Value = String;
    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<String, D::Error> {
        d.deserialize_string(self)
    }
}
#[derive(Deserialize)]
#[serde(field_identifier, rename_all = "snake_case")]
enum RoomField {
    Room,
    Runs,
}
impl Entry for RoomSurfaces {
    fn read<'de, D: Deserializer<'de>>(d: D, m: &mut Meter) -> Result<Self, D::Error> {
        m.charge::<D::Error>(30)?;
        struct Fields<'a>(&'a mut Meter);
        impl<'de> Visitor<'de> for Fields<'_> {
            type Value = RoomSurfaces;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("named room surfaces")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let (mut room, mut runs) = (None, None);
                while let Some(field) = map.next_key::<RoomField>()? {
                    match field {
                        RoomField::Room => {
                            if room.is_some() {
                                return Err(A::Error::duplicate_field("room"));
                            }
                            room = Some(map.next_value::<u32>()?);
                        }
                        RoomField::Runs => {
                            if runs.is_some() {
                                return Err(A::Error::duplicate_field("runs"));
                            }
                            runs = Some(map.next_value_seed(ListSeed::<CellRun> {
                                meter: self.0,
                                maximum: None,
                                marker: PhantomData,
                            })?);
                        }
                    }
                }
                let room = RoomId::new(room.ok_or_else(|| A::Error::missing_field("room"))?)
                    .ok_or_else(|| A::Error::custom("zero room id"))?;
                Ok(RoomSurfaces {
                    room,
                    runs: runs.ok_or_else(|| A::Error::missing_field("runs"))?,
                })
            }
        }
        d.deserialize_map(Fields(m))
    }
}
// Includes the new scope-field name, complete largest tagged-map skeleton,
// and delimiter allowance. Per-record charges are prepaid before decoding.
const MAPPING_HEADER_CHARGE: u64 = 75;
const MAPPING_FRAME_CHARGE: u64 = 68;

struct MappingUp(Direction);
impl<'de> Deserialize<'de> for MappingUp {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Up;
        impl Visitor<'_> for Up {
            type Value = MappingUp;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("one lowercase cardinal construction direction")
            }
            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Self::Value, E> {
                Ok(MappingUp(match value {
                    "north" => Direction::North,
                    "east" => Direction::East,
                    "south" => Direction::South,
                    "west" => Direction::West,
                    "up" => Direction::Up,
                    "down" => Direction::Down,
                    _ => return Err(E::custom("invalid mapping construction direction")),
                }))
            }
        }
        d.deserialize_str(Up)
    }
}
impl Serialize for MappingUp {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let value = match self.0 {
            Direction::North => "north",
            Direction::East => "east",
            Direction::South => "south",
            Direction::West => "west",
            Direction::Up => "up",
            Direction::Down => "down",
            _ => {
                return Err(serde::ser::Error::custom(
                    "invalid mapping construction direction",
                ))
            }
        };
        s.serialize_str(value)
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MappingFrameFields {
    room: u32,
    region: u64,
    up: MappingUp,
}
impl Entry for MappingRoomFrame {
    fn read<'de, D: Deserializer<'de>>(d: D, m: &mut Meter) -> Result<Self, D::Error> {
        m.charge::<D::Error>(MAPPING_FRAME_CHARGE)?;
        let value: MappingFrameFields = crate::bounded::named_map(d)?;
        Ok(Self {
            room: RoomId::new(value.room).ok_or_else(|| D::Error::custom("zero mapping room"))?,
            region: RegionId(value.region),
            up: value.up.0,
        })
    }
}
enum MappingStatus {
    Uncertified,
    Certified,
    CapacityUnavailable,
}
impl<'de> Deserialize<'de> for MappingStatus {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Status;
        impl Visitor<'_> for Status {
            type Value = MappingStatus;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("an explicit mapping certification status string")
            }
            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Self::Value, E> {
                match value {
                    "uncertified" => Ok(MappingStatus::Uncertified),
                    "certified" => Ok(MappingStatus::Certified),
                    "capacity_unavailable" => Ok(MappingStatus::CapacityUnavailable),
                    _ => Err(E::custom("unknown mapping certification status")),
                }
            }
        }
        d.deserialize_str(Status)
    }
}
#[derive(Deserialize)]
#[serde(field_identifier, rename_all = "snake_case")]
enum MappingField {
    Status,
    RoomFrames,
    ExteriorDoorways,
}
impl Entry for MappingCertification {
    fn read<'de, D: Deserializer<'de>>(d: D, m: &mut Meter) -> Result<Self, D::Error> {
        m.charge::<D::Error>(MAPPING_HEADER_CHARGE)?;
        struct Fields<'a>(&'a mut Meter);
        impl<'de> Visitor<'de> for Fields<'_> {
            type Value = MappingCertification;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a strict tagged mapping certification record")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let (mut status, mut room_frames, mut exterior_doorways) = (None, None, None);
                while let Some(field) = map.next_key::<MappingField>()? {
                    match field {
                        MappingField::Status => {
                            if status.is_some() {
                                return Err(A::Error::duplicate_field("status"));
                            }
                            status = Some(map.next_value::<MappingStatus>()?);
                        }
                        MappingField::RoomFrames => {
                            if room_frames.is_some() {
                                return Err(A::Error::duplicate_field("room_frames"));
                            }
                            room_frames =
                                Some(map.next_value_seed(ListSeed::<MappingRoomFrame> {
                                    meter: self.0,
                                    maximum: None,
                                    marker: PhantomData,
                                })?);
                        }
                        MappingField::ExteriorDoorways => {
                            if exterior_doorways.is_some() {
                                return Err(A::Error::duplicate_field("exterior_doorways"));
                            }
                            exterior_doorways = Some(map.next_value_seed(ListSeed::<CellRun> {
                                meter: self.0,
                                maximum: None,
                                marker: PhantomData,
                            })?);
                        }
                    }
                }
                match status.ok_or_else(|| A::Error::missing_field("status"))? {
                    MappingStatus::Certified => {
                        Ok(MappingCertification::Certified(MappingFrames {
                            room_frames: room_frames
                                .ok_or_else(|| A::Error::missing_field("room_frames"))?,
                            exterior_doorways: exterior_doorways
                                .ok_or_else(|| A::Error::missing_field("exterior_doorways"))?,
                        }))
                    }
                    status => {
                        if room_frames.is_some() || exterior_doorways.is_some() {
                            return Err(A::Error::custom(
                                "unavailable mapping status has payload fields",
                            ));
                        }
                        Ok(match status {
                            MappingStatus::Uncertified => MappingCertification::Uncertified,
                            MappingStatus::CapacityUnavailable => {
                                MappingCertification::CapacityUnavailable
                            }
                            MappingStatus::Certified => unreachable!(),
                        })
                    }
                }
            }
        }
        d.deserialize_map(Fields(m))
    }
}

#[derive(Deserialize)]
#[serde(field_identifier, rename_all = "snake_case")]
enum ScopeField {
    Id,
    Members,
    Primary,
    Rooms,
    Mapping,
}
impl Entry for Shared<Scope> {
    fn read<'de, D: Deserializer<'de>>(d: D, m: &mut Meter) -> Result<Self, D::Error> {
        m.charge::<D::Error>(47)?;
        struct Fields<'a>(&'a mut Meter);
        impl<'de> Visitor<'de> for Fields<'_> {
            type Value = Scope;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("named spatial scope")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Scope, A::Error> {
                let (mut id, mut members, mut primary, mut rooms, mut mapping) =
                    (None, None, None, None, None);
                while let Some(field) = map.next_key::<ScopeField>()? {
                    match field {
                        ScopeField::Id => {
                            if id.is_some() {
                                return Err(A::Error::duplicate_field("id"));
                            }
                            id = Some(map.next_value_seed(LabelSeed(self.0))?);
                        }
                        ScopeField::Members => {
                            if members.is_some() {
                                return Err(A::Error::duplicate_field("members"));
                            }
                            members = Some(map.next_value_seed(ListSeed::<RegionId> {
                                meter: self.0,
                                maximum: None,
                                marker: PhantomData,
                            })?);
                        }
                        ScopeField::Primary => {
                            if primary.is_some() {
                                return Err(A::Error::duplicate_field("primary"));
                            }
                            primary = Some(map.next_value_seed(ListSeed::<PrimaryRun> {
                                meter: self.0,
                                maximum: None,
                                marker: PhantomData,
                            })?);
                        }
                        ScopeField::Rooms => {
                            if rooms.is_some() {
                                return Err(A::Error::duplicate_field("rooms"));
                            }
                            rooms = Some(map.next_value_seed(ListSeed::<RoomSurfaces> {
                                meter: self.0,
                                maximum: None,
                                marker: PhantomData,
                            })?);
                        }
                        ScopeField::Mapping => {
                            if mapping.is_some() {
                                return Err(A::Error::duplicate_field("mapping"));
                            }
                            mapping =
                                Some(map.next_value_seed(EntrySeed::<MappingCertification> {
                                    meter: self.0,
                                    marker: PhantomData,
                                })?);
                        }
                    }
                }
                Ok(Scope {
                    id: id.ok_or_else(|| A::Error::missing_field("id"))?,
                    members: members.ok_or_else(|| A::Error::missing_field("members"))?,
                    primary: primary.ok_or_else(|| A::Error::missing_field("primary"))?,
                    rooms: rooms.ok_or_else(|| A::Error::missing_field("rooms"))?,
                    mapping: mapping.ok_or_else(|| A::Error::missing_field("mapping"))?,
                })
            }
        }
        let scope = d.deserialize_map(Fields(m))?;
        scope
            .validate(&m.structural_budget::<D::Error>()?)
            .map_err(|_| D::Error::custom("invalid spatial scope"))?;
        Ok(Shared::new(scope))
    }
}
#[derive(Deserialize)]
#[serde(field_identifier, rename_all = "snake_case")]
enum PoolField {
    Scopes,
    Directories,
}
impl<'de> DeserializeSeed<'de> for PoolSeed {
    type Value = DecodedPool;
    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<DecodedPool, D::Error> {
        let mut meter = Meter {
            used: 0,
            limit: self.0.charge_bytes,
        };
        meter.charge::<D::Error>(
            30_u64
                .checked_add(21 * u64::from(self.0.root_references))
                .ok_or_else(|| D::Error::custom("root reserve overflow"))?,
        )?;
        struct Fields<'a> {
            meter: &'a mut Meter,
            limits: DecodeLimits,
        }
        impl<'de> Visitor<'de> for Fields<'_> {
            type Value = DecodedPool;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("named spatial pool")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<DecodedPool, A::Error> {
                let (mut scopes, mut directories) = (None, None);
                while let Some(field) = map.next_key::<PoolField>()? {
                    match field {
                        PoolField::Scopes => {
                            if scopes.is_some() {
                                return Err(A::Error::duplicate_field("scopes"));
                            }
                            scopes = Some(map.next_value_seed(ListSeed::<Shared<Scope>> {
                                meter: self.meter,
                                maximum: Some(u32::MAX),
                                marker: PhantomData,
                            })?);
                        }
                        PoolField::Directories => {
                            if directories.is_some() {
                                return Err(A::Error::duplicate_field("directories"));
                            }
                            directories = Some(map.next_value_seed(ListSeed::<Vec<u32>> {
                                meter: self.meter,
                                maximum: Some(self.limits.directories),
                                marker: PhantomData,
                            })?);
                        }
                    }
                }
                Ok(DecodedPool {
                    scopes: scopes.ok_or_else(|| A::Error::missing_field("scopes"))?,
                    directories: directories
                        .ok_or_else(|| A::Error::missing_field("directories"))?,
                    limits: self.limits,
                    consumed_charge: self.meter.used,
                })
            }
        }
        d.deserialize_map(Fields {
            meter: &mut meter,
            limits: self.0,
        })
    }
}

impl DecodedPool {
    pub fn validate_roots(self, roots: &[u32]) -> Result<ValidatedPool, PoolError> {
        if usize::try_from(self.limits.root_references).ok() != Some(roots.len()) {
            return Err(PoolError::Capacity);
        }
        // Immutable source scope ownership is global across retained roots. Check
        // once, avoiding repeated payload-member walks per directory reference.
        // Surface-only foreign-region dependencies do not grant membership and
        // deliberately do not participate in exclusive primary ownership.
        let mut ids = BTreeSet::new();
        let mut regions = BTreeSet::new();
        for scope in &self.scopes {
            if !ids.insert(scope.id.as_str()) || scope.members.iter().any(|r| !regions.insert(*r)) {
                return Err(PoolError::Structure);
            }
        }
        let mut unique = BTreeSet::new();
        let mut used_scopes = BTreeSet::new();
        for directory in &self.directories {
            if !unique.insert(directory.as_slice()) {
                return Err(PoolError::Structure);
            }
            let mut previous: Option<&str> = None;
            for &index in directory {
                let scope = self
                    .scopes
                    .get(index as usize)
                    .ok_or(PoolError::Structure)?;
                if previous.is_some_and(|id| id >= scope.id.as_str()) {
                    return Err(PoolError::Structure);
                }
                previous = Some(&scope.id);
                used_scopes.insert(index);
            }
        }
        let mut used_directories = BTreeSet::new();
        for &root in roots {
            if root as usize >= self.directories.len() {
                return Err(PoolError::Structure);
            }
            used_directories.insert(root);
        }
        if used_directories.len() != self.directories.len()
            || used_scopes.len() != self.scopes.len()
        {
            return Err(PoolError::Structure);
        }
        drop(ids);
        drop(unique);
        Ok(ValidatedPool {
            scopes: self.scopes,
            directories: self.directories,
            roots: roots.to_vec(),
        })
    }
}

#[derive(Serialize)]
struct RunWire {
    region: u64,
    z: i32,
    y: i32,
    first_x: i32,
    last_x: i32,
}
impl From<CellRun> for RunWire {
    fn from(r: CellRun) -> Self {
        Self {
            region: r.region.0,
            z: r.z,
            y: r.y,
            first_x: r.first_x,
            last_x: r.last_x,
        }
    }
}
#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum PrimaryWire {
    Room { id: u32 },
    Passage { id: u32 },
}
#[derive(Serialize)]
struct MembershipWire {
    primary: PrimaryWire,
    doorway: Option<u32>,
}
impl From<Membership> for MembershipWire {
    fn from(m: Membership) -> Self {
        Self {
            primary: match m.primary {
                PrimarySpace::Room(id) => PrimaryWire::Room { id: id.0.get() },
                PrimarySpace::Passage(id) => PrimaryWire::Passage { id: id.0.get() },
            },
            doorway: m.doorway.map(|id| id.0.get()),
        }
    }
}
#[derive(Serialize)]
struct PrimaryRunWire {
    cells: RunWire,
    membership: MembershipWire,
}
struct Runs<'a>(&'a [CellRun]);
impl Serialize for Runs<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_seq(self.0.iter().copied().map(RunWire::from))
    }
}
struct Primaries<'a>(&'a [PrimaryRun]);
impl Serialize for Primaries<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_seq(self.0.iter().map(|r| PrimaryRunWire {
            cells: r.cells.into(),
            membership: r.membership.into(),
        }))
    }
}
#[derive(Serialize)]
struct RoomWire<'a> {
    room: u32,
    runs: Runs<'a>,
}
struct Rooms<'a>(&'a [RoomSurfaces]);
impl Serialize for Rooms<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_seq(self.0.iter().map(|r| RoomWire {
            room: r.room.0.get(),
            runs: Runs(&r.runs),
        }))
    }
}
#[derive(Serialize)]
struct MappingFrameWire {
    room: u32,
    region: u64,
    up: MappingUp,
}
struct MappingFrameList<'a>(&'a [MappingRoomFrame]);
impl Serialize for MappingFrameList<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_seq(self.0.iter().map(|frame| MappingFrameWire {
            room: frame.room.0.get(),
            region: frame.region.0,
            up: MappingUp(frame.up),
        }))
    }
}
#[derive(Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
enum MappingRecord<'a> {
    Uncertified,
    Certified {
        room_frames: MappingFrameList<'a>,
        exterior_doorways: Runs<'a>,
    },
    CapacityUnavailable,
}
struct MappingWire<'a>(&'a MappingCertification);
impl Serialize for MappingWire<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self.0 {
            MappingCertification::Uncertified => MappingRecord::Uncertified,
            MappingCertification::CapacityUnavailable => MappingRecord::CapacityUnavailable,
            MappingCertification::Certified(frames) => MappingRecord::Certified {
                room_frames: MappingFrameList(&frames.room_frames),
                exterior_doorways: Runs(&frames.exterior_doorways),
            },
        }
        .serialize(s)
    }
}
#[derive(Serialize)]
struct ScopeWire<'a> {
    id: &'a str,
    members: &'a [RegionId],
    primary: Primaries<'a>,
    rooms: Rooms<'a>,
    mapping: MappingWire<'a>,
}
struct Scopes<'a>(&'a [Shared<Scope>]);
impl Serialize for Scopes<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_seq(self.0.iter().map(|r| ScopeWire {
            id: &r.id,
            members: &r.members,
            primary: Primaries(&r.primary),
            rooms: Rooms(&r.rooms),
            mapping: MappingWire(&r.mapping),
        }))
    }
}
#[derive(Serialize)]
struct PoolWire<'a> {
    scopes: Scopes<'a>,
    directories: &'a [Vec<u32>],
}
impl Serialize for EncodedPool<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let _ = &self.0.roots;
        PoolWire {
            scopes: Scopes(&self.0.scopes),
            directories: &self.0.directories,
        }
        .serialize(s)
    }
}

impl<'de, T: Entry> Visitor<'de> for ListSeed<'_, T> {
    type Value = Vec<T>;
    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a bounded spatial sequence")
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Vec<T>, A::Error> {
        let mut values = Vec::new();
        loop {
            if self.maximum.is_some_and(|max| values.len() >= max as usize) {
                if seq.next_element_seed(RejectEntry)?.is_some() {
                    unreachable!();
                }
                return Ok(values);
            }
            let Some(value) = seq.next_element_seed(EntrySeed::<T> {
                meter: self.meter,
                marker: PhantomData,
            })?
            else {
                return Ok(values);
            };
            values.push(value);
        }
    }
}

impl<'de> Visitor<'de> for LabelSeed<'_> {
    type Value = String;
    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a bounded nonempty scope label")
    }
    fn visit_str<E: serde::de::Error>(self, s: &str) -> Result<String, E> {
        if s.is_empty() || s.chars().any(char::is_control) {
            return Err(E::custom("invalid scope label"));
        }
        self.0.charge::<E>(
            u64::try_from(s.len())
                .ok()
                .and_then(|n| n.checked_mul(6))
                .ok_or_else(|| E::custom("label charge overflow"))?,
        )?;
        Ok(s.to_owned())
    }
    fn visit_string<E: serde::de::Error>(self, s: String) -> Result<String, E> {
        if s.is_empty() || s.chars().any(char::is_control) {
            return Err(E::custom("invalid scope label"));
        }
        self.0.charge::<E>(
            u64::try_from(s.len())
                .ok()
                .and_then(|n| n.checked_mul(6))
                .ok_or_else(|| E::custom("label charge overflow"))?,
        )?;
        Ok(s)
    }
}

use super::registry::Registry;
/// Every actual root reference is charged 21 bytes by the existing codec meter.
/// Historical acquisition roots join snapshot roots; this is archive-derived,
/// not a second fixed history count limit.
pub(crate) const MAX_ROOT_REFERENCES: usize = crate::checkpoint::ARCHIVE_BYTE_LIMIT / 21;

pub(crate) fn capture_pool(
    directories: &[Shared<Registry>],
    references: impl ExactSizeIterator<Item = usize>,
) -> Result<ValidatedPool, PoolError> {
    use std::collections::BTreeMap;
    let max = MAX_ROOT_REFERENCES;
    if directories.len() > max || references.len() > max {
        return Err(PoolError::Capacity);
    }
    let limits = DecodeLimits {
        charge_bytes: crate::checkpoint::ARCHIVE_BYTE_LIMIT as u64,
        root_references: u32::try_from(references.len()).map_err(|_| PoolError::Capacity)?,
        directories: u32::try_from(max).map_err(|_| PoolError::Capacity)?,
    };
    let mut meter = Meter {
        used: 0,
        limit: limits.charge_bytes,
    };
    let pay = |m: &mut Meter, n| {
        m.charge::<serde::de::value::Error>(n)
            .map_err(|_| PoolError::Capacity)
    };
    pay(&mut meter, 30 + 21 * u64::from(limits.root_references))?;
    let mut scopes = Vec::new();
    let mut saved_directories = Vec::new();
    let mut scope_ids = BTreeMap::<&str, u32>::new();
    for registry in directories {
        let count = registry.payloads().count();
        let charge = u64::try_from(count)
            .ok()
            .and_then(|n| n.checked_mul(11))
            .and_then(|n| n.checked_add(3))
            .ok_or(PoolError::Capacity)?;
        pay(&mut meter, charge)?;
        let mut directory = Vec::new();
        for scope in registry.payloads() {
            let index = if let Some(&index) = scope_ids.get(scope.id.as_str()) {
                if scopes.get(index as usize) != Some(scope) {
                    return Err(PoolError::Structure);
                }
                index
            } else {
                let charge = scope_charge(scope)?;
                pay(&mut meter, charge)?;
                scope
                    .validate(
                        &meter
                            .structural_budget::<serde::de::value::Error>()
                            .map_err(|_| PoolError::Capacity)?,
                    )
                    .map_err(|_| PoolError::Structure)?;
                let index = u32::try_from(scopes.len()).map_err(|_| PoolError::Capacity)?;
                scopes.push(scope.clone());
                scope_ids.insert(scope.id.as_str(), index);
                index
            };
            directory.push(index);
        }
        saved_directories.push(directory);
    }
    // Root memory is bounded/reserved before this copy. Internal usize never
    // silently truncates to the current u32 wire-index domain.
    let roots = references
        .map(|r| u32::try_from(r).map_err(|_| PoolError::Capacity))
        .collect::<Result<Vec<_>, _>>()?;
    drop(scope_ids);
    DecodedPool {
        scopes,
        directories: saved_directories,
        limits,
        consumed_charge: meter.used,
    }
    .validate_roots(&roots)
}
/// Complete conservative archive charge for one scope, including its mapping tag.
/// Source compilation uses this same meter when assigning optional cache space.
pub fn scope_charge(scope: &Scope) -> Result<u64, PoolError> {
    let mul = |n: usize, k: u64| {
        u64::try_from(n)
            .ok()
            .and_then(|n| n.checked_mul(k))
            .ok_or(PoolError::Capacity)
    };
    let mut charge = 47_u64;
    for cost in [
        mul(scope.id.len(), 6)?,
        mul(scope.members.len(), 21)?,
        mul(scope.primary.len(), 195)?,
    ] {
        charge = charge.checked_add(cost).ok_or(PoolError::Capacity)?;
    }
    for room in &scope.rooms {
        charge = charge
            .checked_add(30)
            .and_then(|c| c.checked_add(mul(room.runs.len(), 107).ok()?))
            .ok_or(PoolError::Capacity)?;
    }
    let mapping_charge = match &scope.mapping {
        MappingCertification::Certified(frames) => {
            mapping::cache_charge(frames).map_err(|_| PoolError::Capacity)?
        }
        MappingCertification::Uncertified | MappingCertification::CapacityUnavailable => {
            MAPPING_HEADER_CHARGE
        }
    };
    charge = charge
        .checked_add(mapping_charge)
        .ok_or(PoolError::Capacity)?;
    Ok(charge)
}

// Exact-reserve public PoolSeed/DecodedPool remain unchanged. Only private seed
// can construct DeferredPool, with reserve0 and consumed_charge retained.
pub(crate) struct DeferredPool {
    decoded: DecodedPool,
}
pub(crate) struct DeferredCandidate<'a> {
    pool: &'a DeferredPool,
}
impl DeferredPool {
    pub(crate) fn candidate(&self) -> DeferredCandidate<'_> {
        DeferredCandidate { pool: self }
    }
}
impl DeferredCandidate<'_> {
    pub(crate) fn finalize(
        self,
        roots: impl ExactSizeIterator<Item = usize>,
    ) -> Result<ValidatedPool, PoolError> {
        if roots.len() > MAX_ROOT_REFERENCES {
            return Err(PoolError::Capacity);
        }
        let count = u32::try_from(roots.len()).map_err(|_| PoolError::Capacity)?;
        let decoded = &self.pool.decoded;
        let charged = decoded
            .consumed_charge
            .checked_add(21 * u64::from(count))
            .ok_or(PoolError::Capacity)?;
        if charged > decoded.limits.charge_bytes {
            return Err(PoolError::Capacity);
        }
        let roots = roots
            .map(|r| u32::try_from(r).map_err(|_| PoolError::Capacity))
            .collect::<Result<Vec<_>, _>>()?;
        // Context owns one bounded directory copy; Scope payloads remain shared.
        // This consumed private handle supplies no zero-reserve install path.
        DecodedPool {
            scopes: decoded.scopes.clone(),
            directories: decoded.directories.clone(),
            limits: DecodeLimits {
                root_references: count,
                ..decoded.limits
            },
            consumed_charge: charged,
        }
        .validate_roots(&roots)
    }
}

pub(crate) struct DeferredSeed;
impl<'de> DeserializeSeed<'de> for DeferredSeed {
    type Value = DeferredPool;
    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<DeferredPool, D::Error> {
        let limits = DecodeLimits {
            charge_bytes: crate::checkpoint::ARCHIVE_BYTE_LIMIT as u64,
            root_references: 0,
            directories: MAX_ROOT_REFERENCES as u32,
        };
        PoolSeed(limits)
            .deserialize(d)
            .map(|decoded| DeferredPool { decoded })
    }
}
impl ValidatedPool {
    pub(crate) fn registry(&self, index: usize) -> Option<Shared<Registry>> {
        let refs = self.directories.get(index)?;
        Some(Shared::new(Registry::from_validated_payloads(
            refs.iter().map(|i| self.scopes[*i as usize].clone()),
        )))
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod mapping_codec_tests {
    use super::*;
    use serde_json::{json, Value};

    fn decode_mapping(
        raw: &str,
        limit: u64,
    ) -> Result<(MappingCertification, u64), serde_json::Error> {
        let mut meter = Meter { used: 0, limit };
        let mut de = serde_json::Deserializer::from_str(raw);
        let mapping = MappingCertification::read(&mut de, &mut meter)?;
        de.end()?;
        Ok((mapping, meter.used))
    }
    fn positive() -> Value {
        json!({"status":"certified","room_frames":[{"room":1,"region":1,"up":"up"}],
            "exterior_doorways":[]})
    }
    fn max_frame() -> MappingRoomFrame {
        MappingRoomFrame {
            room: RoomId::new(u32::MAX).unwrap(),
            region: RegionId(u64::MAX),
            up: Direction::North,
        }
    }
    fn max_run() -> CellRun {
        CellRun {
            region: RegionId(u64::MAX),
            z: i32::MIN,
            y: i32::MIN,
            first_x: i32::MIN,
            last_x: i32::MIN,
        }
    }
    #[test]
    fn mapping_maximum_operand_shapes_and_real_writer_fit_the_independent_precharge() {
        let frame = json!({"room":u32::MAX,"region":u64::MAX,"up":"north"});
        let run = json!({"region":u64::MAX,"z":i32::MIN,"y":i32::MIN,
            "first_x":i32::MIN,"last_x":i32::MIN});
        let empty = json!({"status":"certified","room_frames":[],"exterior_doorways":[]});
        assert_eq!(serde_json::to_vec(&frame).unwrap().len(), 62);
        assert_eq!(serde_json::to_vec(&run).unwrap().len(), 106);
        assert_eq!(serde_json::to_vec(&empty).unwrap().len(), 62);
        assert_eq!(",\"mapping\":".len(), 11);
        assert_eq!(
            serde_json::to_vec(&json!({"status":"uncertified"}))
                .unwrap()
                .len(),
            24
        );
        assert_eq!(
            serde_json::to_vec(&json!({"status":"capacity_unavailable"}))
                .unwrap()
                .len(),
            33
        );
        let frames = MappingFrames {
            room_frames: vec![max_frame()],
            exterior_doorways: vec![max_run()],
        };
        let charged = mapping::cache_charge(&frames).unwrap();
        assert_eq!(charged, 75 + 68 + 107);
        let source = MappingCertification::Certified(frames);
        let encoded = serde_json::to_vec(&MappingWire(&source)).unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&encoded).unwrap(),
            json!({
            "status":"certified","room_frames":[frame],"exterior_doorways":[run]})
        );
        assert!(encoded.len() as u64 + 11 <= charged);
        let raw = std::str::from_utf8(&encoded).unwrap();
        let (decoded, used) = decode_mapping(raw, charged).unwrap();
        assert_eq!(decoded, source);
        assert_eq!(used, charged);
        assert!(decode_mapping(raw, charged - 1).is_err());
    }
    #[test]
    fn explicit_mapping_tags_roundtrip_without_absence_or_null_defaults() {
        for (value, expected) in [
            (
                json!({"status":"uncertified"}),
                MappingCertification::Uncertified,
            ),
            (
                json!({"status":"capacity_unavailable"}),
                MappingCertification::CapacityUnavailable,
            ),
            (
                json!({"status":"certified","room_frames":[],"exterior_doorways":[]}),
                MappingCertification::Certified(MappingFrames {
                    room_frames: vec![],
                    exterior_doorways: vec![],
                }),
            ),
        ] {
            let (decoded, used) = decode_mapping(&value.to_string(), 75).unwrap();
            assert_eq!(decoded, expected);
            assert_eq!(used, 75);
            assert_eq!(serde_json::to_value(MappingWire(&decoded)).unwrap(), value);
        }
        for malformed in [
            Value::Null,
            json!([]),
            json!("uncertified"),
            json!({}),
            json!({"status":null}),
            json!({"status":{"uncertified":null}}),
            json!({"status":"unknown"}),
            json!({"status":"certified"}),
            json!({"status":"uncertified","room_frames":[]}),
            json!({"status":"capacity_unavailable","exterior_doorways":[]}),
            json!({"status":"uncertified","extra":false}),
        ] {
            assert!(
                decode_mapping(&malformed.to_string(), 1024).is_err(),
                "{malformed}"
            );
        }
    }
    #[test]
    fn certified_records_reject_missing_positional_unknown_duplicate_and_noncardinal_fields() {
        let good = positive();
        decode_mapping(&good.to_string(), 1024).expect("valid positive before mutations");
        for pointer in ["", "/room_frames/0"] {
            let fields = good.pointer(pointer).unwrap().as_object().unwrap();
            for key in fields.keys() {
                let mut bad = good.clone();
                bad.pointer_mut(pointer)
                    .unwrap()
                    .as_object_mut()
                    .unwrap()
                    .remove(key);
                assert!(
                    decode_mapping(&bad.to_string(), 1024).is_err(),
                    "missing {pointer}/{key}"
                );
            }
            let mut bad = good.clone();
            *bad.pointer_mut(pointer).unwrap() = Value::Array(fields.values().cloned().collect());
            assert!(decode_mapping(&bad.to_string(), 1024).is_err());
            let mut bad = good.clone();
            bad.pointer_mut(pointer).unwrap()["unknown"] = json!(true);
            assert!(decode_mapping(&bad.to_string(), 1024).is_err());
        }
        for value in [
            json!("north_east"),
            json!("North"),
            json!(0),
            json!({"up":null}),
            Value::Null,
        ] {
            let mut bad = good.clone();
            bad["room_frames"][0]["up"] = value;
            assert!(decode_mapping(&bad.to_string(), 1024).is_err());
        }
        let raw = good.to_string();
        for (needle, duplicate) in [
            (
                "\"status\":\"certified\"",
                "\"status\":\"certified\",\"status\":\"certified\"",
            ),
            ("\"room\":1", "\"room\":1,\"room\":1"),
            ("\"region\":1", "\"region\":1,\"region\":1"),
            ("\"up\":\"up\"", "\"up\":\"up\",\"up\":\"up\""),
            ("\"room_frames\":", "\"room_frames\":[],\"room_frames\":"),
            (
                "\"exterior_doorways\":[]",
                "\"exterior_doorways\":[],\"exterior_doorways\":[]",
            ),
        ] {
            assert!(raw.contains(needle));
            assert!(decode_mapping(&raw.replacen(needle, duplicate, 1), 1024).is_err());
        }
    }
    #[test]
    fn mapping_sequence_precharge_rejects_next_record_before_reading_its_invalid_body() {
        let good = positive();
        let (decoded, used) = decode_mapping(&good.to_string(), 143).unwrap();
        assert!(matches!(decoded, MappingCertification::Certified(_)));
        assert_eq!(used, 143);
        // The second frame body is deliberately not valid MappingRoomFrame.
        // With no entry allowance the charge error must win before its decode.
        let raw = r#"{"status":"certified","room_frames":[{"room":1,"region":1,"up":"up"},"invalid frame"],"exterior_doorways":[]}"#;
        let error = decode_mapping(raw, 143).unwrap_err();
        assert!(
            error.to_string().contains("spatial charge exceeds limit"),
            "{error}"
        );
    }
    #[test]
    fn real_scope_writer_and_required_mapping_field_share_the_capture_decoder_charge() {
        let cells = max_run();
        let scope = Scope {
            id: "scope".into(),
            members: vec![cells.region],
            primary: vec![PrimaryRun {
                cells,
                membership: Membership::new(
                    PrimarySpace::Room(RoomId::new(u32::MAX).unwrap()),
                    None,
                )
                .unwrap(),
            }],
            rooms: vec![RoomSurfaces {
                room: RoomId::new(u32::MAX).unwrap(),
                runs: vec![cells],
            }],
            mapping: MappingCertification::Certified(MappingFrames {
                room_frames: vec![max_frame()],
                exterior_doorways: vec![],
            }),
        };
        let encoded = serde_json::to_value(ScopeWire {
            id: &scope.id,
            members: &scope.members,
            primary: Primaries(&scope.primary),
            rooms: Rooms(&scope.rooms),
            mapping: MappingWire(&scope.mapping),
        })
        .unwrap();
        let charge = scope_charge(&scope).unwrap();
        assert_eq!(charge, 47 + 6 * 5 + 21 + 195 + 30 + 107 + 75 + 68);
        let read = |value: &Value, limit| {
            let raw = value.to_string();
            let mut meter = Meter { used: 0, limit };
            let mut de = serde_json::Deserializer::from_str(&raw);
            Shared::<Scope>::read(&mut de, &mut meter).map(|scope| (scope, meter.used))
        };
        let (restored, used) = read(&encoded, charge).unwrap();
        assert_eq!(&*restored, &scope);
        assert_eq!(used, charge);
        assert!(read(&encoded, charge - 1).is_err());
        let mut missing = encoded.clone();
        missing.as_object_mut().unwrap().remove("mapping").unwrap();
        assert!(read(&missing, charge).is_err());
        let mut null = encoded;
        null["mapping"] = Value::Null;
        assert!(read(&null, charge).is_err());
    }
    #[test]
    fn certified_maximum_width_intervals_validate_as_runs_without_cell_expansion() {
        let mut cells = max_run();
        cells.last_x = i32::MAX;
        assert_eq!(cells.expanded_len(), Some(1_u64 << 32));
        let room = RoomId::new(u32::MAX).unwrap();
        let scope = Scope {
            id: "wide".into(),
            members: vec![cells.region],
            primary: vec![PrimaryRun {
                cells,
                membership: Membership::new(PrimarySpace::Room(room), None).unwrap(),
            }],
            rooms: vec![RoomSurfaces {
                room,
                runs: vec![cells],
            }],
            mapping: MappingCertification::Certified(MappingFrames {
                room_frames: vec![max_frame()],
                exterior_doorways: vec![],
            }),
        };
        let raw = serde_json::to_string(&ScopeWire {
            id: &scope.id,
            members: &scope.members,
            primary: Primaries(&scope.primary),
            rooms: Rooms(&scope.rooms),
            mapping: MappingWire(&scope.mapping),
        })
        .unwrap();
        let charge = scope_charge(&scope).unwrap();
        let mut meter = Meter {
            used: 0,
            limit: charge,
        };
        let mut de = serde_json::Deserializer::from_str(&raw);
        let restored = Shared::<Scope>::read(&mut de, &mut meter).unwrap();
        assert_eq!(&*restored, &scope);
        assert_eq!(meter.used, charge);
    }
}
