//! Context-bound, descriptive fragments of private authored strike sources.
//! Page limits bound projection and retention independently from authoring limits.
use crate::{ActorId, AttackView, CombatTraceStep, ContentRef, DiagnosticField, DiagnosticValue};
use serde::{Deserialize, Serialize};

pub const MAX_STRIKE_PAGE_ENTRIES: usize = 16;
pub const MAX_STRIKE_PAGE_FIELDS: usize = 128;
pub const MAX_STRIKE_PAGE_FIELD_BYTES: usize = 1024;
pub const MAX_STRIKE_PAGE_BYTES: usize = 192 * 1024;
pub const MAX_STRIKE_SOURCE_ENTRIES: usize = 656;

use crate::bounded::bounded;

fn string<'de, D: serde::Deserializer<'de>, const N: usize>(d: D) -> Result<String, D::Error> {
    struct Visitor<const N: usize>;
    impl<const N: usize> serde::de::Visitor<'_> for Visitor<N> {
        type Value = String;
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            write!(f, "at most {N} UTF-8 bytes")
        }
        fn visit_str<E: serde::de::Error>(self, s: &str) -> Result<String, E> {
            if s.len() > N || s.chars().any(char::is_control) {
                return Err(E::custom("invalid strike page string"));
            }
            Ok(s.to_owned())
        }
    }
    d.deserialize_str(Visitor::<N>)
}
fn identity<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    let id = string::<D, 60>(d)?;
    if !crate::valid_content_id(&id) {
        return Err(serde::de::Error::custom("invalid content identity"));
    }
    Ok(id)
}
fn handle<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    let id = string::<D, 128>(d)?;
    if id.is_empty() {
        return Err(serde::de::Error::custom("empty inspection identity"));
    }
    Ok(id)
}
fn ordinal<'de, D: serde::Deserializer<'de>, const N: u16>(d: D) -> Result<u16, D::Error> {
    let n = u16::deserialize(d)?;
    if n >= N {
        return Err(serde::de::Error::custom(
            "strike page ordinal exceeds bound",
        ));
    }
    Ok(n)
}
fn strike<'de, D: serde::Deserializer<'de>>(d: D) -> Result<u16, D::Error> {
    ordinal::<D, 16>(d)
}
fn rider<'de, D: serde::Deserializer<'de>>(d: D) -> Result<u16, D::Error> {
    ordinal::<D, 8>(d)
}
fn index<'de, D: serde::Deserializer<'de>>(d: D) -> Result<u16, D::Error> {
    ordinal::<D, 128>(d)
}

/// Ephemeral connection-local identity, never a saved credential or access grant.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct StrikeInspectionId(#[serde(deserialize_with = "handle")] pub String);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum StrikeSourceSelector {
    Natural {},
    SelectedMelee {},
    Ability {
        #[serde(deserialize_with = "identity")]
        id: String,
    },
}
impl StrikeSourceSelector {
    pub fn valid(&self) -> bool {
        match self {
            Self::Ability { id } => crate::valid_content_id(id),
            _ => true,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum StrikePagePosition {
    StrikeHeader {
        #[serde(deserialize_with = "strike")]
        strike: u16,
    },
    SuccessEffect {
        #[serde(deserialize_with = "strike")]
        strike: u16,
        #[serde(deserialize_with = "index")]
        index: u16,
    },
    FailureEffect {
        #[serde(deserialize_with = "strike")]
        strike: u16,
        #[serde(deserialize_with = "index")]
        index: u16,
    },
    StrikeDescriptor {
        #[serde(deserialize_with = "strike")]
        strike: u16,
        #[serde(deserialize_with = "index")]
        index: u16,
    },
    RiderHeader {
        #[serde(deserialize_with = "strike")]
        strike: u16,
        #[serde(deserialize_with = "rider")]
        rider: u16,
    },
    RiderEffect {
        #[serde(deserialize_with = "strike")]
        strike: u16,
        #[serde(deserialize_with = "rider")]
        rider: u16,
        #[serde(deserialize_with = "index")]
        index: u16,
    },
    RiderDescriptor {
        #[serde(deserialize_with = "strike")]
        strike: u16,
        #[serde(deserialize_with = "rider")]
        rider: u16,
        #[serde(deserialize_with = "index")]
        index: u16,
    },
    Weapon {},
    SelectedMelee {},
}
impl StrikePagePosition {
    fn key(&self) -> (u16, u16, u16, u16, u16) {
        match *self {
            Self::StrikeHeader { strike } => (strike, 0, 0, 0, 0),
            Self::SuccessEffect { strike, index } => (strike, 1, 0, 0, index),
            Self::FailureEffect { strike, index } => (strike, 2, 0, 0, index),
            Self::StrikeDescriptor { strike, index } => (strike, 3, 0, 0, index),
            Self::RiderHeader { strike, rider } => (strike, 4, rider, 0, 0),
            Self::RiderEffect {
                strike,
                rider,
                index,
            } => (strike, 4, rider, 1, index),
            Self::RiderDescriptor {
                strike,
                rider,
                index,
            } => (strike, 4, rider, 2, index),
            Self::Weapon {} => (0, 5, 0, 0, 0),
            Self::SelectedMelee {} => (0, 6, 0, 0, 0),
        }
    }
    pub fn valid(&self) -> bool {
        let (s, _, r, _, i) = self.key();
        s < 16 && r < 8 && i < 128
    }
    pub fn strike(&self) -> Option<u16> {
        match self {
            Self::Weapon {} | Self::SelectedMelee {} => None,
            _ => Some(self.key().0),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StrikePageCursor {
    pub inspection: StrikeInspectionId,
    pub position: StrikePagePosition,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Reference {
    #[serde(deserialize_with = "identity")]
    id: String,
    #[serde(deserialize_with = "string::<_,256>")]
    name: String,
}
impl From<Reference> for ContentRef {
    fn from(v: Reference) -> Self {
        Self {
            id: v.id,
            name: v.name,
        }
    }
}
fn reference<'de, D: serde::Deserializer<'de>>(d: D) -> Result<ContentRef, D::Error> {
    Ok(Reference::deserialize(d)?.into())
}
fn optional_reference<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Option<ContentRef>, D::Error> {
    Ok(Option::<Reference>::deserialize(d)?.map(Into::into))
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum StrikeSourceAttribution {
    Ability {
        #[serde(deserialize_with = "reference")]
        ability: ContentRef,
    },
    Natural {
        #[serde(deserialize_with = "reference")]
        species: ContentRef,
    },
    /// Species-authored strikes followed by this ability's riders on every strike.
    NaturalMelee {
        #[serde(deserialize_with = "reference")]
        species: ContentRef,
        #[serde(deserialize_with = "reference")]
        ability: ContentRef,
        additional_riders: u16,
        #[serde(deserialize_with = "bounded::<_,_,16>")]
        natural_riders: Vec<u16>,
    },
    Weapon {
        #[serde(deserialize_with = "reference")]
        source: ContentRef,
        socket: u16,
    },
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum FieldValue {
    Text {
        #[serde(deserialize_with = "string::<_,1024>")]
        value: String,
    },
    Signed {
        #[serde(with = "crate::integers::signed")]
        value: i64,
    },
    Unsigned {
        #[serde(with = "crate::integers::unsigned")]
        value: u64,
    },
    Bool {
        value: bool,
    },
    Dice {
        #[serde(deserialize_with = "bounded::<_,_,128>")]
        values: Vec<u16>,
    },
    Content {
        #[serde(deserialize_with = "reference")]
        value: ContentRef,
    },
}
impl From<FieldValue> for DiagnosticValue {
    fn from(v: FieldValue) -> Self {
        match v {
            FieldValue::Text { value } => Self::Text { value },
            FieldValue::Signed { value } => Self::Signed { value },
            FieldValue::Unsigned { value } => Self::Unsigned { value },
            FieldValue::Bool { value } => Self::Bool { value },
            FieldValue::Dice { values } => Self::Dice { values },
            FieldValue::Content { value } => Self::Content { value },
        }
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Field {
    #[serde(deserialize_with = "identity")]
    id: String,
    #[serde(deserialize_with = "string::<_,64>")]
    name: String,
    value: FieldValue,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Row {
    #[serde(deserialize_with = "identity")]
    id: String,
    #[serde(deserialize_with = "string::<_,64>")]
    name: String,
    #[serde(deserialize_with = "bounded::<_,_,32>")]
    fields: Vec<Field>,
}
impl From<Row> for CombatTraceStep {
    fn from(v: Row) -> Self {
        Self {
            id: v.id,
            name: v.name,
            fields: v
                .fields
                .into_iter()
                .map(|v| DiagnosticField {
                    id: v.id,
                    name: v.name,
                    value: v.value.into(),
                })
                .collect(),
        }
    }
}
fn row<'de, D: serde::Deserializer<'de>>(d: D) -> Result<CombatTraceStep, D::Error> {
    Ok(Row::deserialize(d)?.into())
}
fn optional_row<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Option<CombatTraceStep>, D::Error> {
    Ok(Option::<Row>::deserialize(d)?.map(Into::into))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WeaponComponent {
    category: crate::DamageType,
    #[serde(deserialize_with = "optional_reference")]
    descriptor: Option<ContentRef>,
    amount: crate::DamageAmountView,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WeaponKey {
    category: crate::DamageType,
    #[serde(deserialize_with = "optional_reference")]
    descriptor: Option<ContentRef>,
    #[serde(deserialize_with = "Option::deserialize")]
    sides: Option<u16>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WeaponDamage {
    primary: WeaponKey,
    #[serde(deserialize_with = "bounded::<_,_,32>")]
    components: Vec<WeaponComponent>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Weapon {
    #[serde(deserialize_with = "reference")]
    skill: ContentRef,
    bonus: i32,
    wind_up: u32,
    recovery: u32,
    damage: WeaponDamage,
}
fn weapon<'de, D: serde::Deserializer<'de>>(d: D) -> Result<AttackView, D::Error> {
    let v = Weapon::deserialize(d)?;
    Ok(AttackView {
        skill: v.skill,
        bonus: v.bonus,
        wind_up: v.wind_up,
        recovery: v.recovery,
        damage: crate::DamageView {
            primary: crate::DamageKeyView {
                category: v.damage.primary.category,
                descriptor: v.damage.primary.descriptor,
                sides: v.damage.primary.sides,
            },
            components: v
                .damage
                .components
                .into_iter()
                .map(|c| crate::DamageComponentView {
                    category: c.category,
                    descriptor: c.descriptor,
                    amount: c.amount,
                })
                .collect(),
        },
    })
}

/// Authored rider branch, relative to the primary check's roller.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiderTrigger {
    Success,
    Failure,
}

/// Disclosed source condition only; no live sequence history or eligibility.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum InspectionRiderCondition {
    SequenceSuccessfulHits {
        #[serde(deserialize_with = "hit_minimum")]
        minimum: u16,
    },
}
impl InspectionRiderCondition {
    pub fn valid(&self) -> bool {
        match self {
            Self::SequenceSuccessfulHits { minimum } => (1..=16).contains(minimum),
        }
    }
}
fn hit_minimum<'de, D: serde::Deserializer<'de>>(d: D) -> Result<u16, D::Error> {
    let minimum = u16::deserialize(d)?;
    if !(1..=16).contains(&minimum) {
        return Err(serde::de::Error::custom("invalid sequence hit minimum"));
    }
    Ok(minimum)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum StrikePageEntry {
    StrikeHeader {
        #[serde(deserialize_with = "strike")]
        strike: u16,
        #[serde(with = "crate::integers::unsigned")]
        offset: u64,
        #[serde(deserialize_with = "optional_reference")]
        unlock: Option<ContentRef>,
        #[serde(deserialize_with = "optional_row")]
        check: Option<CombatTraceStep>,
        success_count: u16,
        failure_count: u16,
        descriptor_count: u16,
        rider_count: u16,
    },
    SuccessEffect {
        #[serde(deserialize_with = "strike")]
        strike: u16,
        #[serde(deserialize_with = "index")]
        index: u16,
        #[serde(deserialize_with = "row")]
        row: CombatTraceStep,
    },
    FailureEffect {
        #[serde(deserialize_with = "strike")]
        strike: u16,
        #[serde(deserialize_with = "index")]
        index: u16,
        #[serde(deserialize_with = "row")]
        row: CombatTraceStep,
    },
    StrikeDescriptor {
        #[serde(deserialize_with = "strike")]
        strike: u16,
        #[serde(deserialize_with = "index")]
        index: u16,
        #[serde(deserialize_with = "reference")]
        descriptor: ContentRef,
    },
    RiderHeader {
        #[serde(deserialize_with = "strike")]
        strike: u16,
        #[serde(deserialize_with = "rider")]
        rider: u16,
        trigger: RiderTrigger,
        #[serde(deserialize_with = "Option::deserialize")]
        condition: Option<InspectionRiderCondition>,
        #[serde(deserialize_with = "optional_row")]
        check: Option<CombatTraceStep>,
        effect_count: u16,
        descriptor_count: u16,
    },
    RiderEffect {
        #[serde(deserialize_with = "strike")]
        strike: u16,
        #[serde(deserialize_with = "rider")]
        rider: u16,
        #[serde(deserialize_with = "index")]
        index: u16,
        #[serde(deserialize_with = "row")]
        row: CombatTraceStep,
    },
    RiderDescriptor {
        #[serde(deserialize_with = "strike")]
        strike: u16,
        #[serde(deserialize_with = "rider")]
        rider: u16,
        #[serde(deserialize_with = "index")]
        index: u16,
        #[serde(deserialize_with = "reference")]
        descriptor: ContentRef,
    },
    Weapon {
        #[serde(deserialize_with = "weapon")]
        attack: AttackView,
    },
    SelectedMelee {},
}
fn valid_row(row: &CombatTraceStep) -> bool {
    row.valid()
        && row.name.len() <= 64
        && row.fields.iter().all(|field| {
            field.name.len() <= 64
                && matches!(
                    crate::codec::encoded_length(field, MAX_STRIKE_PAGE_FIELD_BYTES),
                    Ok(Some(_))
                )
        })
}
impl StrikePageEntry {
    pub fn position(&self) -> StrikePagePosition {
        match self {
            Self::StrikeHeader { strike, .. } => {
                StrikePagePosition::StrikeHeader { strike: *strike }
            }
            Self::SuccessEffect { strike, index, .. } => StrikePagePosition::SuccessEffect {
                strike: *strike,
                index: *index,
            },
            Self::FailureEffect { strike, index, .. } => StrikePagePosition::FailureEffect {
                strike: *strike,
                index: *index,
            },
            Self::StrikeDescriptor { strike, index, .. } => StrikePagePosition::StrikeDescriptor {
                strike: *strike,
                index: *index,
            },
            Self::RiderHeader { strike, rider, .. } => StrikePagePosition::RiderHeader {
                strike: *strike,
                rider: *rider,
            },
            Self::RiderEffect {
                strike,
                rider,
                index,
                ..
            } => StrikePagePosition::RiderEffect {
                strike: *strike,
                rider: *rider,
                index: *index,
            },
            Self::RiderDescriptor {
                strike,
                rider,
                index,
                ..
            } => StrikePagePosition::RiderDescriptor {
                strike: *strike,
                rider: *rider,
                index: *index,
            },
            Self::Weapon { .. } => StrikePagePosition::Weapon {},
            Self::SelectedMelee {} => StrikePagePosition::SelectedMelee {},
        }
    }
    pub fn fields(&self) -> usize {
        match self {
            Self::StrikeHeader { check, .. } | Self::RiderHeader { check, .. } => {
                check.as_ref().map_or(0, |c| c.fields.len())
            }
            Self::SuccessEffect { row, .. }
            | Self::FailureEffect { row, .. }
            | Self::RiderEffect { row, .. } => row.fields.len(),
            Self::Weapon { attack } => attack.damage.components.len(),
            _ => 0,
        }
    }
    pub fn valid(&self) -> bool {
        self.position().valid()
            && match self {
                Self::StrikeHeader {
                    strike,
                    offset,
                    unlock,
                    check,
                    success_count,
                    failure_count,
                    descriptor_count,
                    rider_count,
                } => {
                    *offset <= 1_000_000
                        && (*strike != 0 || (*offset == 0 && unlock.is_none()))
                        && unlock.as_ref().is_none_or(ContentRef::valid)
                        && check
                            .as_ref()
                            .is_none_or(|row| row.fields.len() <= 5 && valid_row(row))
                        && *success_count <= 128
                        && *failure_count <= 128
                        && *descriptor_count <= 128
                        && *rider_count <= 8
                }
                Self::RiderHeader {
                    trigger,
                    condition,
                    check,
                    effect_count,
                    descriptor_count,
                    ..
                } => {
                    condition.as_ref().is_none_or(|condition| {
                        *trigger == RiderTrigger::Success && condition.valid()
                    }) && *effect_count <= 128
                        && *descriptor_count <= 128
                        && check
                            .as_ref()
                            .is_none_or(|row| row.fields.len() <= 3 && valid_row(row))
                }
                Self::SuccessEffect { row, .. }
                | Self::FailureEffect { row, .. }
                | Self::RiderEffect { row, .. } => valid_row(row),
                Self::StrikeDescriptor { descriptor, .. }
                | Self::RiderDescriptor { descriptor, .. } => descriptor.valid(),
                Self::Weapon { attack } => {
                    attack.valid()
                        && attack.damage.components.len() <= 32
                        && attack.damage.components.iter().all(|c| {
                            matches!(
                                crate::codec::encoded_length(c, MAX_STRIKE_PAGE_FIELD_BYTES),
                                Ok(Some(_))
                            )
                        })
                }
                Self::SelectedMelee {} => true,
            }
            && matches!(
                crate::codec::encoded_length(
                    self,
                    2048 + self.fields() * MAX_STRIKE_PAGE_FIELD_BYTES
                ),
                Ok(Some(_))
            )
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "Page")]
pub struct CreatureStrikePage {
    pub inspection: StrikeInspectionId,
    pub actor: ActorId,
    #[serde(with = "crate::integers::unsigned")]
    pub target_revision: u64,
    pub source: StrikeSourceSelector,
    pub resolved: StrikeSourceAttribution,
    pub strike_count: u16,
    pub start: StrikePagePosition,
    pub entries: Vec<StrikePageEntry>,
    pub next: Option<StrikePageCursor>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Page {
    inspection: StrikeInspectionId,
    actor: ActorId,
    #[serde(with = "crate::integers::unsigned")]
    target_revision: u64,
    source: StrikeSourceSelector,
    resolved: StrikeSourceAttribution,
    strike_count: u16,
    start: StrikePagePosition,
    #[serde(deserialize_with = "bounded::<_,_,16>")]
    entries: Vec<StrikePageEntry>,
    #[serde(deserialize_with = "Option::deserialize")]
    next: Option<StrikePageCursor>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidStrikePage;
impl std::fmt::Display for InvalidStrikePage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Invalid private strike page")
    }
}
impl std::error::Error for InvalidStrikePage {}
impl TryFrom<Page> for CreatureStrikePage {
    type Error = InvalidStrikePage;
    fn try_from(v: Page) -> Result<Self, Self::Error> {
        let page = Self {
            inspection: v.inspection,
            actor: v.actor,
            target_revision: v.target_revision,
            source: v.source,
            resolved: v.resolved,
            strike_count: v.strike_count,
            start: v.start,
            entries: v.entries,
            next: v.next,
        };
        page.validate()?;
        Ok(page)
    }
}
// A fragment may start inside a list. Validate every local boundary whose header
// is present, leaving cross-page continuity to the context-bound client navigator.
fn valid_local_boundaries(entries: &[StrikePageEntry]) -> bool {
    let mut strike_header = None;
    let mut rider_header = None;
    let mut last_offset = None;
    let mut primary_check = None;
    for entry in entries {
        match entry {
            StrikePageEntry::StrikeHeader {
                strike,
                offset,
                check,
                success_count,
                failure_count,
                descriptor_count,
                rider_count,
                ..
            } => {
                if last_offset.is_some_and(|prior| prior > *offset) {
                    return false;
                }
                last_offset = Some(*offset);
                primary_check = Some(check.is_some());
                strike_header = Some((
                    *strike,
                    *success_count,
                    *failure_count,
                    *descriptor_count,
                    *rider_count,
                ));
                rider_header = None;
            }
            StrikePageEntry::RiderHeader {
                strike,
                rider,
                trigger,
                effect_count,
                descriptor_count,
                ..
            } => {
                if strike_header.is_some_and(|(s, _, _, _, count)| s != *strike || *rider >= count)
                {
                    return false;
                }
                if *trigger == RiderTrigger::Failure && primary_check == Some(false) {
                    return false;
                }
                rider_header = Some((*strike, *rider, *effect_count, *descriptor_count));
            }
            StrikePageEntry::SuccessEffect { strike, index, .. }
            | StrikePageEntry::FailureEffect { strike, index, .. }
            | StrikePageEntry::StrikeDescriptor { strike, index, .. } => {
                if let Some((s, success, failure, descriptors, _)) = strike_header {
                    let count = match entry {
                        StrikePageEntry::SuccessEffect { .. } => success,
                        StrikePageEntry::FailureEffect { .. } => failure,
                        _ => descriptors,
                    };
                    if s != *strike || *index >= count {
                        return false;
                    }
                }
            }
            StrikePageEntry::RiderEffect {
                strike,
                rider,
                index,
                ..
            }
            | StrikePageEntry::RiderDescriptor {
                strike,
                rider,
                index,
                ..
            } => {
                if strike_header.is_some_and(|(s, _, _, _, count)| s != *strike || *rider >= count)
                {
                    return false;
                }
                if let Some((s, r, effects, descriptors)) = rider_header {
                    let count = if matches!(entry, StrikePageEntry::RiderEffect { .. }) {
                        effects
                    } else {
                        descriptors
                    };
                    if s != *strike || r != *rider || *index >= count {
                        return false;
                    }
                }
            }
            StrikePageEntry::Weapon { .. } | StrikePageEntry::SelectedMelee {} => {}
        }
    }
    true
}
impl CreatureStrikePage {
    pub fn validate(&self) -> Result<(), InvalidStrikePage> {
        let source = match (&self.source, &self.resolved) {
            (
                StrikeSourceSelector::Ability { id },
                StrikeSourceAttribution::Ability { ability },
            ) => id == &ability.id && ability.valid(),
            (
                StrikeSourceSelector::Ability { id },
                StrikeSourceAttribution::NaturalMelee {
                    species,
                    ability,
                    additional_riders,
                    natural_riders,
                },
            ) => {
                id == &ability.id
                    && ability.valid()
                    && species.valid()
                    && (1..=16).contains(&self.strike_count)
                    && natural_riders.len() == usize::from(self.strike_count)
                    && *additional_riders <= 8
                    && natural_riders
                        .iter()
                        .all(|count| *count <= 8 - *additional_riders)
            }
            (
                StrikeSourceSelector::Natural {} | StrikeSourceSelector::SelectedMelee {},
                StrikeSourceAttribution::Natural { species },
            ) => species.valid(),
            (
                StrikeSourceSelector::SelectedMelee {},
                StrikeSourceAttribution::Weapon { source, socket },
            ) => source.valid() && usize::from(*socket) < crate::MAX_EQUIPMENT_SLOTS,
            _ => false,
        };
        let natural_position_valid = |position: &StrikePagePosition| {
            let StrikeSourceAttribution::NaturalMelee {
                additional_riders,
                natural_riders,
                ..
            } = &self.resolved
            else {
                return true;
            };
            let (strike, rider) = match position {
                StrikePagePosition::RiderHeader { strike, rider }
                | StrikePagePosition::RiderEffect { strike, rider, .. }
                | StrikePagePosition::RiderDescriptor { strike, rider, .. } => (*strike, *rider),
                _ => return true,
            };
            natural_riders
                .get(usize::from(strike))
                .and_then(|count| count.checked_add(*additional_riders))
                .is_some_and(|count| rider < count)
        };
        let natural_headers_valid = self.entries.iter().all(|entry| {
            natural_position_valid(&entry.position())
                && match (&self.resolved, entry) {
                    (
                        StrikeSourceAttribution::NaturalMelee {
                            additional_riders,
                            natural_riders,
                            ..
                        },
                        StrikePageEntry::StrikeHeader {
                            strike,
                            rider_count,
                            ..
                        },
                    ) => {
                        natural_riders
                            .get(usize::from(*strike))
                            .and_then(|count| count.checked_add(*additional_riders))
                            == Some(*rider_count)
                    }
                    _ => true,
                }
        });
        let marker = matches!(self.start, StrikePagePosition::SelectedMelee {});
        let weapon = matches!(self.start, StrikePagePosition::Weapon {});
        let special = if marker {
            self.strike_count == 0
                && matches!(self.resolved, StrikeSourceAttribution::Ability { .. })
                && self.entries == [StrikePageEntry::SelectedMelee {}]
                && self.next.is_none()
        } else if weapon {
            self.strike_count == 1
                && matches!(self.resolved, StrikeSourceAttribution::Weapon { .. })
                && self.entries.len() == 1
                && matches!(self.entries[0], StrikePageEntry::Weapon { .. })
                && self.next.is_none()
        } else {
            (1..=16).contains(&self.strike_count)
                && !matches!(self.resolved, StrikeSourceAttribution::Weapon { .. })
                && self
                    .entries
                    .iter()
                    .all(|e| e.position().strike().is_some_and(|s| s < self.strike_count))
        };
        if !source
            || !special
            || !natural_headers_valid
            || self.actor.0 == 0
            || !crate::valid_label(&self.inspection.0, 128)
            || !self.source.valid()
            || self.entries.is_empty()
            || self.entries.len() > MAX_STRIKE_PAGE_ENTRIES
            || self.start != self.entries[0].position()
            || self.entries.iter().any(|e| !e.valid())
            || !valid_local_boundaries(&self.entries)
            || self
                .entries
                .iter()
                .map(StrikePageEntry::fields)
                .sum::<usize>()
                > MAX_STRIKE_PAGE_FIELDS
            || !self
                .entries
                .windows(2)
                .all(|p| p[0].position().key() < p[1].position().key())
            || self.next.as_ref().is_some_and(|c| {
                c.inspection != self.inspection
                    || !c.position.valid()
                    || !natural_position_valid(&c.position)
                    || !c.position.strike().is_some_and(|s| s < self.strike_count)
                    || c.position.key() <= self.entries.last().unwrap().position().key()
            })
            || !matches!(
                crate::codec::encoded_length(self, MAX_STRIKE_PAGE_BYTES - 8192),
                Ok(Some(_))
            )
        {
            Err(InvalidStrikePage)
        } else {
            Ok(())
        }
    }
}
