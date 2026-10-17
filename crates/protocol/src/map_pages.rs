//! Bounded actor-owned last-seen appearances, without live action capabilities.
use crate::{ActorId, BranchId, ItemClass, ObservationBase, Position, StreamContext, StreamCursor};
use serde::{Deserialize, Serialize};

pub const MAX_MAP_PAGE_CELLS: usize = 512;
pub const MAX_MAP_PAGE_BYTES: usize = 128 * 1024;
pub const MAX_MAP_TRANSFER_CELLS: usize = 12_474;
pub const MAX_MAP_TRANSFER_PAGES: usize = 128;
pub const MAX_MAP_TRANSFER_BYTES: usize = 8 * 1024 * 1024;
// Every encoded item has more than 64 bytes of required field names alone.
// This eager allocation bound cannot exclude an item list fitting a whole page.
const MAX_MAP_CELL_ITEMS: usize = MAX_MAP_PAGE_BYTES / 64;

macro_rules! record {
    ($name:ident { $($(#[$attribute:meta])* $field:ident: $ty:ty),* $(,)? }) => {
        #[derive(Clone, Debug, PartialEq, Eq, Serialize)]
        pub struct $name { $($(#[$attribute])* pub $field: $ty,)* }
        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Fields { $($(#[$attribute])* $field: $ty,)* }
                let f: Fields = crate::bounded::named_map(d)?;
                let value = Self { $($field: f.$field,)* };
                value.validate().map_err(serde::de::Error::custom)?;
                Ok(value)
            }
        }
    };
}
fn text<'de, D: serde::Deserializer<'de>, const N: usize>(d: D) -> Result<String, D::Error> {
    struct Visitor<const N: usize>;
    impl<const N: usize> serde::de::Visitor<'_> for Visitor<N> {
        type Value = String;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "at most {N} UTF-8 bytes")
        }
        fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<String, E> {
            if value.len() > N {
                return Err(E::custom("map text exceeds byte bound"));
            }
            Ok(value.to_owned())
        }
    }
    d.deserialize_str(Visitor::<N>)
}
fn required_option<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> Result<Option<T>, D::Error> {
    Option::deserialize(d)
}
fn asset<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    #[derive(Deserialize)]
    struct Text(#[serde(deserialize_with = "text::<_,MAX_MAP_PAGE_BYTES>")] String);
    Option::<Text>::deserialize(d).map(|value| value.map(|value| value.0))
}
fn position<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Position, D::Error> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Fields {
        x: i32,
        y: i32,
        z: i32,
    }
    let f: Fields = crate::bounded::named_map(d)?;
    Ok(Position {
        x: f.x,
        y: f.y,
        z: f.z,
    })
}
fn cursor<'de, D: serde::Deserializer<'de>>(d: D) -> Result<StreamCursor, D::Error> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Fields {
        #[serde(with = "crate::integers::unsigned")]
        sequence: u64,
        #[serde(with = "crate::integers::unsigned")]
        tick: u64,
    }
    let f: Fields = crate::bounded::named_map(d)?;
    Ok(StreamCursor {
        sequence: f.sequence,
        tick: f.tick,
    })
}
fn base<'de, D: serde::Deserializer<'de>>(d: D) -> Result<ObservationBase, D::Error> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Fields {
        #[serde(deserialize_with = "cursor")]
        cursor: StreamCursor,
        #[serde(with = "crate::integers::unsigned")]
        revision: u64,
    }
    let f: Fields = crate::bounded::named_map(d)?;
    Ok(ObservationBase {
        cursor: f.cursor,
        revision: f.revision,
    })
}
fn stream<'de, D: serde::Deserializer<'de>>(d: D) -> Result<StreamContext, D::Error> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Fields {
        #[serde(deserialize_with = "text::<_,128>")]
        stream: String,
        #[serde(with = "crate::integers::unsigned")]
        epoch: u64,
    }
    let f: Fields = crate::bounded::named_map(d)?;
    let value = StreamContext {
        stream: crate::StreamId(f.stream),
        epoch: f.epoch,
    };
    if !value.is_valid() {
        return Err(serde::de::Error::custom("invalid map stream context"));
    }
    Ok(value)
}
fn branch<'de, D: serde::Deserializer<'de>>(d: D) -> Result<BranchId, D::Error> {
    let value = text::<D, 128>(d)?;
    if !label(&value, 128) {
        return Err(serde::de::Error::custom("invalid map branch"));
    }
    Ok(BranchId(value))
}
fn item_class<'de, D: serde::Deserializer<'de>>(d: D) -> Result<ItemClass, D::Error> {
    let value = text::<D, 16>(d)?;
    ItemClass::deserialize(serde::de::value::StrDeserializer::<D::Error>::new(&value))
}
fn label(value: &str, limit: usize) -> bool {
    !value.is_empty() && value.len() <= limit && !value.chars().any(char::is_control)
}
fn valid_asset(value: &Option<String>) -> bool {
    value
        .as_ref()
        .is_none_or(|value| value.len() <= MAX_MAP_PAGE_BYTES)
}
fn cells<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<RememberedMapCell>, D::Error> {
    crate::bounded::bounded::<D, _, MAX_MAP_PAGE_CELLS>(d)
}
fn items<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<MapItemAppearance>, D::Error> {
    crate::bounded::bounded::<D, _, MAX_MAP_CELL_ITEMS>(d)
}

record!(MapWindow {
    #[serde(deserialize_with = "position")]
    min: Position,
    #[serde(deserialize_with = "position")]
    max: Position,
});
impl Copy for MapWindow {}
impl MapWindow {
    /// The complete ASCII footprint, including every ordinary height plane.
    pub const fn normal() -> Self {
        Self {
            min: Position {
                x: -31,
                y: -16,
                z: -2,
            },
            max: Position { x: 31, y: 16, z: 3 },
        }
    }
    pub fn volume(&self) -> Option<usize> {
        let mut volume = 1usize;
        for (min, max) in [
            (self.min.x, self.max.x),
            (self.min.y, self.max.y),
            (self.min.z, self.max.z),
        ] {
            let width = i64::from(max).checked_sub(i64::from(min))?.checked_add(1)?;
            if width <= 0 {
                return None;
            }
            volume = volume.checked_mul(usize::try_from(width).ok()?)?;
        }
        Some(volume)
    }
    pub fn validate(&self) -> Result<(), &'static str> {
        if self
            .volume()
            .is_some_and(|volume| volume <= MAX_MAP_TRANSFER_CELLS)
        {
            Ok(())
        } else {
            Err("invalid or oversized map window")
        }
    }
    pub fn contains(&self, position: Position) -> bool {
        position.x >= self.min.x
            && position.x <= self.max.x
            && position.y >= self.min.y
            && position.y <= self.max.y
            && position.z >= self.min.z
            && position.z <= self.max.z
    }
    /// Translate relative bounds without wrapping near the coordinate endpoints.
    pub fn checked_translate(&self, by: Position) -> Option<Self> {
        let translate = |position: Position| {
            Some(Position {
                x: position.x.checked_add(by.x)?,
                y: position.y.checked_add(by.y)?,
                z: position.z.checked_add(by.z)?,
            })
        };
        let value = Self {
            min: translate(self.min)?,
            max: translate(self.max)?,
        };
        value.validate().ok()?;
        Some(value)
    }
}

record!(MapDoorAppearance {
    #[serde(deserialize_with = "text::<_,MAX_MAP_PAGE_BYTES>")]
    name: String,
    #[serde(deserialize_with = "text::<_,MAX_MAP_PAGE_BYTES>")]
    description: String,
    open: bool,
    #[serde(deserialize_with = "asset")]
    asset: Option<String>,
});
impl MapDoorAppearance {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.name.len() <= MAX_MAP_PAGE_BYTES
            && self.description.len() <= MAX_MAP_PAGE_BYTES
            && valid_asset(&self.asset)
        {
            Ok(())
        } else {
            Err("map door appearance exceeds byte bound")
        }
    }
}
record!(MapItemAppearance {
    #[serde(deserialize_with = "item_class")]
    class: ItemClass,
    #[serde(deserialize_with = "text::<_,MAX_MAP_PAGE_BYTES>")]
    description: String,
    #[serde(deserialize_with = "text::<_,80>")]
    name: String,
    #[serde(deserialize_with = "text::<_,80>")]
    appearance: String,
    identified: bool,
    #[serde(with = "crate::integers::unsigned")]
    quantity: u64,
    #[serde(deserialize_with = "asset")]
    asset: Option<String>,
});
impl MapItemAppearance {
    pub fn validate(&self) -> Result<(), &'static str> {
        if label(&self.name, 80)
            && label(&self.appearance, 80)
            && self.quantity > 0
            && self.description.len() <= MAX_MAP_PAGE_BYTES
            && valid_asset(&self.asset)
        {
            Ok(())
        } else {
            Err("invalid map item appearance")
        }
    }
}
record!(RememberedMapCell {
    #[serde(deserialize_with = "text::<_,128>")]
    key: String,
    #[serde(deserialize_with = "position")]
    position: Position,
    wall: bool,
    #[serde(deserialize_with = "text::<_,MAX_MAP_PAGE_BYTES>")]
    material: String,
    #[serde(deserialize_with = "asset")]
    asset: Option<String>,
    place_hint: bool,
    stairs_up: bool,
    stairs_down: bool,
    #[serde(deserialize_with = "required_option")]
    door: Option<MapDoorAppearance>,
    #[serde(deserialize_with = "items")]
    ground_items: Vec<MapItemAppearance>,
});
impl RememberedMapCell {
    pub fn validate(&self) -> Result<(), &'static str> {
        if !label(&self.key, 128)
            || self.material.len() > MAX_MAP_PAGE_BYTES
            || !valid_asset(&self.asset)
            || self.ground_items.len() > MAX_MAP_CELL_ITEMS
        {
            return Err("invalid remembered map cell");
        }
        if let Some(door) = &self.door {
            door.validate()?;
        }
        for item in &self.ground_items {
            item.validate()?;
        }
        Ok(())
    }
}
record!(MapPageCursor {
    #[serde(deserialize_with = "text::<_,128>")]
    transfer: String,
    next: u32,
    page: u16,
});
impl MapPageCursor {
    pub fn validate(&self) -> Result<(), &'static str> {
        let next = self.next as usize;
        let page = usize::from(self.page);
        if label(&self.transfer, 128)
            && page > 0
            && page < MAX_MAP_TRANSFER_PAGES
            && next >= page
            && next <= page * MAX_MAP_PAGE_CELLS
            && next < MAX_MAP_TRANSFER_CELLS
        {
            Ok(())
        } else {
            Err("invalid map continuation")
        }
    }
}
record!(MapQuery {
    #[serde(deserialize_with = "stream")]
    context: StreamContext,
    #[serde(deserialize_with = "branch")]
    branch: BranchId,
    #[serde(deserialize_with = "base")]
    base: ObservationBase,
    window: MapWindow,
    #[serde(deserialize_with = "required_option")]
    cursor: Option<MapPageCursor>,
});
impl MapQuery {
    pub fn validate(&self) -> Result<(), &'static str> {
        if !self.context.is_valid() || !label(&self.branch.0, 128) {
            return Err("invalid map query context");
        }
        self.window.validate()?;
        if let Some(cursor) = &self.cursor {
            cursor.validate()?;
            if cursor.next as usize >= self.window.volume().unwrap_or(0) {
                return Err("map continuation exceeds window");
            }
        }
        Ok(())
    }
}
record!(MapPage {
    #[serde(deserialize_with = "text::<_,128>")]
    transfer: String,
    actor: ActorId,
    #[serde(deserialize_with = "branch")]
    branch: BranchId,
    #[serde(deserialize_with = "base")]
    base: ObservationBase,
    window: MapWindow,
    start: u32,
    page: u16,
    total_cells: u32,
    complete: bool,
    ambiguous: bool,
    limited: bool,
    #[serde(deserialize_with = "cells")]
    cells: Vec<RememberedMapCell>,
    #[serde(deserialize_with = "required_option")]
    next: Option<MapPageCursor>,
});
impl MapPage {
    pub fn validate(&self) -> Result<(), &'static str> {
        self.window.validate()?;
        let start = self.start as usize;
        let page = usize::from(self.page);
        let total = self.total_cells as usize;
        if self.limited
            && (self.total_cells != 0
                || !self.cells.is_empty()
                || self.start != 0
                || self.page != 0
                || self.next.is_some())
        {
            return Err("limited map projection cannot publish an uncertified prefix");
        }
        if !label(&self.transfer, 128)
            || !label(&self.branch.0, 128)
            || page >= MAX_MAP_TRANSFER_PAGES
            || self.cells.len() > MAX_MAP_PAGE_CELLS
            || total > MAX_MAP_TRANSFER_CELLS
            || total > self.window.volume().unwrap_or(0)
            || self.complete == self.limited
            || start < page
            || start > page * MAX_MAP_PAGE_CELLS
        {
            return Err("invalid map page bounds or completeness");
        }
        let end = start
            .checked_add(self.cells.len())
            .ok_or("map page ordinal overflow")?;
        if end > total || (self.cells.is_empty() && (total != 0 || page != 0 || start != 0)) {
            return Err("invalid map page range");
        }
        if let Some(next) = &self.next {
            next.validate()?;
            if end >= total
                || next.transfer != self.transfer
                || next.next as usize != end
                || usize::from(next.page) != page + 1
            {
                return Err("map continuation does not follow page");
            }
        } else if end != total {
            return Err("terminal map page has missing cells");
        }
        let mut positions = std::collections::BTreeSet::new();
        for cell in &self.cells {
            cell.validate()?;
            if !self.window.contains(cell.position) || !positions.insert(cell.position) {
                return Err("duplicate or out-of-window map appearance");
            }
        }
        if crate::encoded_length(self, MAX_MAP_PAGE_BYTES)
            .map_err(|_| "map page encoding failed")?
            .is_none()
        {
            return Err("map page exceeds byte bound");
        }
        Ok(())
    }
}

fn reply_context<'de, D: serde::Deserializer<'de>>(d: D) -> Result<crate::ReplyContext, D::Error> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Input {
        #[serde(deserialize_with = "stream")]
        stream: StreamContext,
        #[serde(with = "crate::integers::unsigned")]
        readiness_revision: u64,
    }
    fn input<'de, D: serde::Deserializer<'de>>(d: D) -> Result<crate::InputContext, D::Error> {
        let f: Input = crate::bounded::named_map(d)?;
        Ok(crate::InputContext {
            stream: f.stream,
            readiness_revision: f.readiness_revision,
        })
    }
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Fields {
        #[serde(deserialize_with = "input")]
        input: crate::InputContext,
        actor: ActorId,
        #[serde(deserialize_with = "branch")]
        branch: BranchId,
        #[serde(deserialize_with = "cursor")]
        cursor: StreamCursor,
        #[serde(with = "crate::integers::unsigned")]
        revision: u64,
    }
    let f: Fields = crate::bounded::named_map(d)?;
    Ok(crate::ReplyContext {
        input: f.input,
        actor: f.actor,
        branch: f.branch,
        cursor: f.cursor,
        revision: f.revision,
    })
}

/// Strict correlated envelope. Stream/branch/base equality against the outstanding
/// query is enforced by the attachment owner, not inferred from this response.
pub(crate) fn read_page_reply<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<(crate::ReplyContext, String, Box<MapPage>), D::Error> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Fields {
        #[serde(deserialize_with = "reply_context")]
        context: crate::ReplyContext,
        #[serde(deserialize_with = "text::<_,128>")]
        request_id: String,
        page: Box<MapPage>,
    }
    let f: Fields = crate::bounded::named_map(d)?;
    if !label(&f.request_id, 128)
        || f.context.actor != f.page.actor
        || f.context.branch != f.page.branch
        || f.context.revision != f.page.base.revision
        || f.context.cursor.sequence < f.page.base.cursor.sequence
        || f.context.cursor.tick != f.page.base.cursor.tick
    {
        return Err(serde::de::Error::custom("map reply context mismatch"));
    }
    #[derive(Serialize)]
    struct Envelope<'a> {
        #[serde(rename = "type")]
        kind: &'static str,
        context: &'a crate::ReplyContext,
        request_id: &'a str,
        page: &'a MapPage,
    }
    let envelope = Envelope {
        kind: "map_page",
        context: &f.context,
        request_id: &f.request_id,
        page: &f.page,
    };
    if crate::encoded_length(&envelope, MAX_MAP_PAGE_BYTES)
        .map_err(serde::de::Error::custom)?
        .is_none()
    {
        return Err(serde::de::Error::custom(
            "map reply exceeds complete envelope byte bound",
        ));
    }
    Ok((f.context, f.request_id, f.page))
}
