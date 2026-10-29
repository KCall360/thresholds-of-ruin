//! Dedicated, non-actionable chart transport and strict complete-stream validation.
//!
//! Components are a canonical declaration prefix, followed by sorted occurrences.
//! Equal occurrences are meaningful and are never deduplicated. Decoding a page
//! does not prove declarations on previous pages; `ChartStreamValidator` does.
use crate::{BranchId, ObservationBase, StreamContext};
use serde::{Deserialize, Serialize};

#[path = "chart_pages/read.rs"]
mod read;
#[path = "chart_pages/stream.rs"]
mod stream;
pub use stream::{ChartAssemblyBudget, ChartStreamValidator};

pub const MAX_CHART_PAGE_RECORDS: usize = 512;
pub const MAX_CHART_PAGE_BYTES: usize = 128 * 1024;
/// Full correlated envelope with an empty record list; includes every header.
/// Keep the independent JSON-envelope witness in tests when extending this DTO.
pub const CHART_ENVELOPE_BYTES: usize = 8192;
pub const CHART_COMPONENT_BYTES: usize = 96;
pub const CHART_ROW_BYTES: usize = 256;
pub const CHART_MONSTER_MARK_BYTES: usize = 96;
pub const CHART_PRESENCE_MARK_BYTES: usize = 32;
pub const CHART_MAPPING_MARK_BYTES: usize = 96;
pub const MAX_CHART_NAME_BYTES: usize = 256;
pub const MAX_CHART_ASSET_BYTES: usize = 80;
pub const MAX_CHART_TOKEN_BYTES: usize = 128;
pub const MAX_CHART_SUMMARIES: usize = 4;
/// Canonical presentation admission only; never a source geometry admission cap.
pub const MAX_CHART_TRANSFER_BYTES: u64 = 64 * 1024 * 1024;
/// Every canonical record costs at least one component plus its comma.
pub const MAX_CHART_TRANSFER_RECORDS: u64 =
    MAX_CHART_TRANSFER_BYTES / (CHART_COMPONENT_BYTES as u64 + 1);

macro_rules! string_enum {
    ($name:ident { $($variant:ident => $text:literal),+ $(,)? }) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
        pub enum $name { $(#[serde(rename = $text)] $variant),+ }
        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                let value = read::text::<D, 16>(d)?;
                match value.as_str() {
                    $($text => Ok(Self::$variant),)+
                    _ => Err(serde::de::Error::custom(concat!("invalid ", stringify!($name)))),
                }
            }
        }
    };
}
string_enum!(ChartKind { Monsters => "monsters", Food => "food", Magic => "magic", Mapping => "mapping" });
string_enum!(ChartMode { Live => "live", Historical => "historical" });
string_enum!(ChartAlignment { Aligned => "aligned", Inconsistent => "inconsistent" });
string_enum!(ChartReplyType { ChartPage => "chart_page" });
string_enum!(ChartStatus { Inactive => "inactive", Empty => "empty", Available => "available", Unavailable => "unavailable" });
string_enum!(ChartAttemptStatus { Empty => "empty", Positive => "positive", Unavailable => "unavailable" });

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

record!(ChartAttempt {
    #[serde(with = "crate::integers::unsigned")]
    attempted_at: u64,
    status: ChartAttemptStatus,
});
impl ChartAttempt {
    pub fn validate(&self) -> Result<(), &'static str> {
        Ok(())
    }
}
record!(ChartSummary {
    kind: ChartKind,
    supported: bool,
    mode: ChartMode,
    status: ChartStatus,
    #[serde(with = "crate::integers::optional_unsigned")]
    acquired_at: Option<u64>,
    #[serde(deserialize_with = "read::optional_token")]
    chart: Option<String>,
    #[serde(deserialize_with = "read::required_option")]
    latest_attempt: Option<ChartAttempt>,
});
impl ChartSummary {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self
            .chart
            .as_deref()
            .is_some_and(|token| !read::valid_token(token))
        {
            return Err("invalid chart summary token");
        }
        let valid = match self.status {
            ChartStatus::Inactive => self.acquired_at.is_none() && self.chart.is_none(),
            ChartStatus::Empty | ChartStatus::Available => {
                self.acquired_at.is_some() && self.chart.is_some()
            }
            ChartStatus::Unavailable => self.chart.is_none(),
        };
        if valid {
            Ok(())
        } else {
            Err("chart status disagrees with sample identity")
        }
    }
}
pub fn validate_summaries(values: &[ChartSummary]) -> Result<(), &'static str> {
    if values.len() > MAX_CHART_SUMMARIES || values.windows(2).any(|w| w[0].kind >= w[1].kind) {
        return Err("chart summaries exceed bound or repeat/descend in kind");
    }
    for value in values {
        value.validate()?;
    }
    Ok(())
}
pub(crate) fn read_summaries<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Vec<ChartSummary>, D::Error> {
    let values = crate::bounded::bounded::<D, _, MAX_CHART_SUMMARIES>(d)?;
    validate_summaries(&values).map_err(serde::de::Error::custom)?;
    Ok(values)
}

/// Sparse Mapping facts, never full map appearance or navigation authority.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MappingMark {
    WallFace { normal: Option<[i8; 3]> },
    Passage,
    Doorway,
    Stairs,
    SecretDoor,
    TrapPresence,
}
impl<'de> Deserialize<'de> for MappingMark {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            WallFace {
                #[serde(deserialize_with = "read::required_option")]
                normal: Option<[i8; 3]>,
            },
            Passage {},
            Doorway {},
            Stairs {},
            SecretDoor {},
            TrapPresence {},
        }
        let value = match crate::bounded::named_map(d)? {
            Fields::WallFace { normal } => Self::WallFace { normal },
            Fields::Passage {} => Self::Passage,
            Fields::Doorway {} => Self::Doorway,
            Fields::Stairs {} => Self::Stairs,
            Fields::SecretDoor {} => Self::SecretDoor,
            Fields::TrapPresence {} => Self::TrapPresence,
        };
        value.validate().map_err(serde::de::Error::custom)?;
        Ok(value)
    }
}
impl MappingMark {
    pub fn validate(&self) -> Result<(), &'static str> {
        if let Self::WallFace {
            normal: Some(normal),
        } = self
        {
            if normal.iter().filter(|axis| **axis != 0).count() != 1
                || normal.iter().any(|axis| !matches!(axis, -1..=1))
            {
                return Err("mapping wall normal must be cardinal");
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ChartMark {
    Monster { name: String, asset: Option<String> },
    Food,
    Magic,
    Mapping { fact: MappingMark },
}
impl<'de> Deserialize<'de> for ChartMark {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            Monster {
                #[serde(deserialize_with = "read::text::<_,MAX_CHART_NAME_BYTES>")]
                name: String,
                #[serde(deserialize_with = "read::asset")]
                asset: Option<String>,
            },
            Food {},
            Magic {},
            Mapping {
                fact: MappingMark,
            },
        }
        let value = match crate::bounded::named_map(d)? {
            Fields::Monster { name, asset } => Self::Monster { name, asset },
            Fields::Food {} => Self::Food,
            Fields::Magic {} => Self::Magic,
            Fields::Mapping { fact } => Self::Mapping { fact },
        };
        value.validate().map_err(serde::de::Error::custom)?;
        Ok(value)
    }
}
impl ChartMark {
    pub fn validate(&self) -> Result<(), &'static str> {
        if let Self::Monster { name, asset } = self {
            if !read::valid_text(name, MAX_CHART_NAME_BYTES)
                || asset.as_deref().is_some_and(|s| !read::valid_asset(s))
            {
                return Err("invalid chart monster appearance");
            }
        }
        if let Self::Mapping { fact } = self {
            fact.validate()?;
        }
        Ok(())
    }
    pub fn kind(&self) -> ChartKind {
        match self {
            Self::Monster { .. } => ChartKind::Monsters,
            Self::Food => ChartKind::Food,
            Self::Magic => ChartKind::Magic,
            Self::Mapping { .. } => ChartKind::Mapping,
        }
    }
    fn encoded_size_bound(&self) -> Result<usize, &'static str> {
        self.validate()?;
        match self {
            Self::Monster { name, asset } => {
                let name = read::escaped(name)?;
                let asset = asset.as_deref().map_or(Ok(4), read::escaped)?;
                CHART_MONSTER_MARK_BYTES
                    .checked_add(name)
                    .and_then(|n| n.checked_add(asset))
                    .ok_or("chart size arithmetic overflow")
            }
            Self::Food | Self::Magic => Ok(CHART_PRESENCE_MARK_BYTES),
            Self::Mapping { .. } => Ok(CHART_MAPPING_MARK_BYTES),
        }
    }
}

record!(ChartComponent {
    #[serde(with = "crate::integers::unsigned")]
    component: u64,
    alignment: ChartAlignment,
});
impl ChartComponent {
    pub fn validate(&self) -> Result<(), &'static str> {
        Ok(())
    }
}

record!(ChartRow {
    #[serde(with = "crate::integers::unsigned")]
    component: u64,
    #[serde(with = "read::optional_position")]
    position: Option<[i64; 3]>,
    mark: ChartMark,
});
impl ChartRow {
    pub fn validate(&self) -> Result<(), &'static str> {
        self.mark.validate()?;
        if let ChartMark::Mapping {
            fact: MappingMark::WallFace { normal },
        } = &self.mark
        {
            if normal.is_some() != self.position.is_some() {
                return Err("mapping wall normal and position must share alignment");
            }
        }
        Ok(())
    }
    fn order_key(&self) -> (u64, Option<[i64; 3]>, &ChartMark) {
        (self.component, self.position, &self.mark)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "record", rename_all = "snake_case")]
pub enum ChartRecord {
    Component(ChartComponent),
    Row(ChartRow),
}
impl<'de> Deserialize<'de> for ChartRecord {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "record", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            Component(ChartComponent),
            Row(ChartRow),
        }
        Ok(match crate::bounded::named_map(d)? {
            Fields::Component(value) => Self::Component(value),
            Fields::Row(value) => Self::Row(value),
        })
    }
}
impl ChartRecord {
    /// Includes the record discriminator and all values; excludes its list comma.
    pub fn encoded_size_bound(&self) -> Result<usize, &'static str> {
        match self {
            Self::Component(_) => Ok(CHART_COMPONENT_BYTES),
            Self::Row(row) => CHART_ROW_BYTES
                .checked_add(row.mark.encoded_size_bound()?)
                .ok_or("chart size arithmetic overflow"),
        }
    }
}

record!(ChartQuery {
    #[serde(deserialize_with = "read::context")]
    context: StreamContext,
    #[serde(deserialize_with = "read::branch")]
    branch: BranchId,
    #[serde(deserialize_with = "read::base")]
    base: ObservationBase,
    #[serde(deserialize_with = "read::token")]
    chart: String,
    #[serde(with = "crate::integers::optional_unsigned")]
    cursor: Option<u64>,
});
impl ChartQuery {
    pub fn validate(&self) -> Result<(), &'static str> {
        if !self.context.is_valid()
            || !self.base.snapshot.is_valid()
            || !read::valid_label(&self.branch.0)
            || !read::valid_token(&self.chart)
            || self.cursor == Some(0)
        {
            return Err("invalid chart query identity or cursor");
        }
        Ok(())
    }
}

record!(ChartPage {
    #[serde(deserialize_with = "read::context")]
    context: StreamContext,
    #[serde(deserialize_with = "read::branch")]
    branch: BranchId,
    #[serde(deserialize_with = "read::base")]
    base: ObservationBase,
    #[serde(deserialize_with = "read::token")]
    chart: String,
    kind: ChartKind,
    mode: ChartMode,
    #[serde(with = "crate::integers::unsigned")]
    acquired_at: u64,
    #[serde(with = "crate::integers::unsigned")]
    total_components: u64,
    /// Components PLUS rows. Empty charts still have their component prefix.
    #[serde(with = "crate::integers::unsigned")]
    total_records: u64,
    #[serde(with = "crate::integers::unsigned")]
    start: u64,
    #[serde(deserialize_with = "read::records")]
    records: Vec<ChartRecord>,
    #[serde(with = "crate::integers::optional_unsigned")]
    next: Option<u64>,
});
impl ChartPage {
    pub fn validate(&self) -> Result<(), &'static str> {
        if !self.context.is_valid()
            || !self.base.snapshot.is_valid()
            || !read::valid_label(&self.branch.0)
            || !read::valid_token(&self.chart)
            || self.total_components == 0
            || self.total_components > self.total_records
            || self.records.is_empty()
            || self.records.len() > MAX_CHART_PAGE_RECORDS
        {
            return Err("invalid chart page identity or bounds");
        }
        let end = self
            .start
            .checked_add(
                u64::try_from(self.records.len()).map_err(|_| "chart record count overflow")?,
            )
            .ok_or("chart page ordinal overflow")?;
        if end > self.total_records || self.next != (end < self.total_records).then_some(end) {
            return Err("invalid chart continuation");
        }
        let mut previous: Option<&ChartRow> = None;
        for (index, record) in self.records.iter().enumerate() {
            let ordinal = self.start + index as u64;
            match record {
                ChartRecord::Component(component)
                    if ordinal < self.total_components && component.component == ordinal => {}
                ChartRecord::Row(row)
                    if ordinal >= self.total_components
                        && row.component < self.total_components
                        && row.mark.kind() == self.kind =>
                {
                    row.validate()?;
                    if previous.is_some_and(|last| last.order_key() > row.order_key()) {
                        return Err("chart occurrences are not in canonical order");
                    }
                    previous = Some(row);
                }
                _ => return Err("chart declaration prefix or row kind mismatch"),
            }
        }
        // Presence of row coordinates depends on declarations that may precede
        // this page. ChartStreamValidator must run before any publication.
        Ok(())
    }
    /// Conservative bound for the ENTIRE reply, including its correlation fields.
    pub fn encoded_reply_size_bound(&self) -> Result<usize, &'static str> {
        self.records
            .iter()
            .try_fold(CHART_ENVELOPE_BYTES, |sum, record| {
                sum.checked_add(record.encoded_size_bound()?)
                    .and_then(|n| n.checked_add(1)) // conservative comma even for first
                    .ok_or("chart page byte arithmetic overflow")
            })
    }
}

record!(ChartPageReply {
    #[serde(rename = "type")]
    message_type: ChartReplyType,
    #[serde(deserialize_with = "read::reply_context")]
    context: crate::ReplyContext,
    #[serde(deserialize_with = "read::request_id")]
    request_id: String,
    page: ChartPage,
});
impl ChartPageReply {
    pub fn validate(&self) -> Result<(), &'static str> {
        validate_reply(&self.context, &self.request_id, &self.page)
    }
    /// Equality is exact, not tick-only or a monotone approximation. Authorization
    /// and selected current/history root remain the attachment owner's job.
    pub fn validate_for(&self, query: &ChartQuery, request_id: &str) -> Result<(), &'static str> {
        query.validate()?;
        self.validate()?;
        if self.request_id != request_id
            || self.page.context != query.context
            || self.page.branch != query.branch
            || self.page.base != query.base
            || self.page.chart != query.chart
            || self.page.start != query.cursor.unwrap_or(0)
        {
            return Err("chart reply does not match exact outstanding query");
        }
        Ok(())
    }
}

fn validate_reply(
    context: &crate::ReplyContext,
    request_id: &str,
    page: &ChartPage,
) -> Result<(), &'static str> {
    page.validate()?;
    if !read::valid_request_id(request_id) {
        return Err("invalid chart request identity");
    }
    if context.input.stream != page.context
        || context.branch != page.branch
        || context.revision < page.base.revision
        || context.cursor.tick < page.base.cursor.tick
        || context.cursor.sequence < page.base.cursor.sequence
    {
        return Err("chart reply context disagrees with its observation base");
    }
    #[derive(Serialize)]
    struct Envelope<'a> {
        #[serde(rename = "type")]
        kind: &'static str,
        context: &'a crate::ReplyContext,
        request_id: &'a str,
        page: &'a ChartPage,
    }
    let envelope = Envelope {
        kind: "chart_page",
        context,
        request_id,
        page,
    };
    if crate::encoded_length(&envelope, MAX_CHART_PAGE_BYTES)
        .map_err(|_| "chart encoding failed")?
        .is_none()
    {
        return Err("chart reply exceeds complete envelope byte bound");
    }
    Ok(())
}

/// Existing ServerMessage tagged-variant hook; validates the full outer envelope.
pub(crate) fn read_page_reply<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<(crate::ReplyContext, String, Box<ChartPage>), D::Error> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Fields {
        #[serde(deserialize_with = "read::reply_context")]
        context: crate::ReplyContext,
        #[serde(deserialize_with = "read::request_id")]
        request_id: String,
        page: Box<ChartPage>,
    }
    let f: Fields = crate::bounded::named_map(d)?;
    validate_reply(&f.context, &f.request_id, &f.page).map_err(serde::de::Error::custom)?;
    Ok((f.context, f.request_id, f.page))
}

/// Standalone bounded codec until the separate live Message integration exists.
pub fn decode_chart_page_reply(text: &str) -> Result<ChartPageReply, crate::DecodeError> {
    crate::codec::decode(text, MAX_CHART_PAGE_BYTES)
}
pub fn decode_chart_query(text: &str) -> Result<ChartQuery, crate::DecodeError> {
    crate::codec::decode(text, crate::MAX_REQUEST_BYTES)
}

#[cfg(test)]
#[path = "chart_pages/tests.rs"]
mod tests;
