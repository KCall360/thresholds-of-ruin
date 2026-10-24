//! Standalone strict spatial codec; no Game/save field or canonical source authority.
use super::registry::{Budget, CellRun, PrimaryRun, RoomSurfaces, Scope};
use super::{Membership, PassageId, PrimarySpace, RoomId};
use serde::de::{DeserializeSeed, Error as _, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::BTreeSet;
use std::{fmt, marker::PhantomData};
use tor_world::{RegionId, Shared};

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
#[derive(Deserialize)]
#[serde(field_identifier, rename_all = "snake_case")]
enum ScopeField {
    Id,
    Members,
    Primary,
    Rooms,
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
                let (mut id, mut members, mut primary, mut rooms) = (None, None, None, None);
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
                    }
                }
                Ok(Scope {
                    id: id.ok_or_else(|| A::Error::missing_field("id"))?,
                    members: members.ok_or_else(|| A::Error::missing_field("members"))?,
                    primary: primary.ok_or_else(|| A::Error::missing_field("primary"))?,
                    rooms: rooms.ok_or_else(|| A::Error::missing_field("rooms"))?,
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
struct ScopeWire<'a> {
    id: &'a str,
    members: &'a [RegionId],
    primary: Primaries<'a>,
    rooms: Rooms<'a>,
}
struct Scopes<'a>(&'a [Shared<Scope>]);
impl Serialize for Scopes<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_seq(self.0.iter().map(|r| ScopeWire {
            id: &r.id,
            members: &r.members,
            primary: Primaries(&r.primary),
            rooms: Rooms(&r.rooms),
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

#[cfg(test)]
mod tests;
