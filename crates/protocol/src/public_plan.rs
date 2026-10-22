//! One paid observation, frozen in the observer's historical frame.
//! This wire shape has no world location, actor identifier or selecting handle.
use crate::{ContentRef, Position};
use serde::{Deserialize, Serialize};

/// Body-origin physical envelope [-8,8]^3 + the radius-16 Manhattan ball,
/// plus two abstract stair occurrences. Mirrors the simulation structural
/// bound without importing simulation types into the wire schema.
pub const MAX_PUBLIC_AIM_OCCURRENCES: usize = 61_619;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ObservedAimView {
    Unavailable,
    /// Unit ray vector in the observer's historical frame, never a destination.
    Direction {
        direction: Position,
    },
    Disclosed {
        cell: String,
        occurrences: Vec<Position>,
    },
}
impl<'de> Deserialize<'de> for ObservedAimView {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            Unavailable {},
            Direction {
                #[serde(deserialize_with = "read_direction")]
                direction: Position,
            },
            Disclosed {
                #[serde(deserialize_with = "read_cell")]
                cell: String,
                #[serde(deserialize_with = "read_occurrences")]
                occurrences: Vec<Position>,
            },
        }
        Ok(match crate::bounded::named_map(d)? {
            Fields::Unavailable {} => Self::Unavailable,
            Fields::Direction { direction } => Self::Direction { direction },
            Fields::Disclosed { cell, occurrences } => Self::Disclosed { cell, occurrences },
        })
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PublicPlanStateView {
    NoPublicPlan,
    Preparing {
        ability: Option<ContentRef>,
        aim: ObservedAimView,
        #[serde(with = "crate::integers::unsigned")]
        remaining_work: u64,
    },
}
impl<'de> Deserialize<'de> for PublicPlanStateView {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            NoPublicPlan {},
            Preparing {
                #[serde(deserialize_with = "read_ability")]
                ability: Option<ContentRef>,
                aim: ObservedAimView,
                #[serde(with = "crate::integers::unsigned")]
                remaining_work: u64,
            },
        }
        Ok(match crate::bounded::named_map(d)? {
            Fields::NoPublicPlan {} => Self::NoPublicPlan,
            Fields::Preparing {
                ability,
                aim,
                remaining_work,
            } => Self::Preparing {
                ability,
                aim,
                remaining_work,
            },
        })
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PublicPlanObservationView {
    #[serde(with = "crate::integers::unsigned")]
    pub observed_at: u64,
    pub subject_name: String,
    pub state: PublicPlanStateView,
}
impl<'de> Deserialize<'de> for PublicPlanObservationView {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            #[serde(with = "crate::integers::unsigned")]
            observed_at: u64,
            #[serde(deserialize_with = "read_name")]
            subject_name: String,
            state: PublicPlanStateView,
        }
        let f: Fields = crate::bounded::named_map(d)?;
        Ok(Self {
            observed_at: f.observed_at,
            subject_name: f.subject_name,
            state: f.state,
        })
    }
}
impl PublicPlanObservationView {
    pub fn valid_at(&self, tick: u64) -> bool {
        self.observed_at <= tick
            && crate::valid_label(&self.subject_name, 256)
            && match &self.state {
                PublicPlanStateView::NoPublicPlan => true,
                PublicPlanStateView::Preparing {
                    ability,
                    aim,
                    remaining_work,
                } => {
                    *remaining_work <= 1_000_000
                        && ability.as_ref().is_none_or(ContentRef::valid)
                        && match aim {
                            ObservedAimView::Unavailable => true,
                            ObservedAimView::Direction { direction } => valid_direction(*direction),
                            ObservedAimView::Disclosed { cell, occurrences } => {
                                crate::valid_label(cell, 128)
                                    && !occurrences.is_empty()
                                    && occurrences.len() <= MAX_PUBLIC_AIM_OCCURRENCES
                                    && occurrences.windows(2).all(|pair| pair[0] < pair[1])
                            }
                        }
                }
            }
    }
}
fn read_ability<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<ContentRef>, D::Error> {
    struct NamedContent(ContentRef);
    impl<'de> Deserialize<'de> for NamedContent {
        fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
            crate::bounded::named_map(d).map(Self)
        }
    }
    Option::<NamedContent>::deserialize(d).map(|value| value.map(|named| named.0))
}
fn read_label<'de, D: serde::Deserializer<'de>, const N: usize>(d: D) -> Result<String, D::Error> {
    struct Label<const N: usize>;
    impl<const N: usize> serde::de::Visitor<'_> for Label<N> {
        type Value = String;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "a label of at most {N} bytes")
        }
        fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<String, E> {
            if crate::valid_label(value, N) {
                Ok(value.to_owned())
            } else {
                Err(E::custom("invalid public observation label"))
            }
        }
    }
    d.deserialize_str(Label::<N>)
}
fn read_name<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    read_label::<D, 256>(d)
}
fn read_cell<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    read_label::<D, 128>(d)
}
fn read_occurrences<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<Position>, D::Error> {
    struct NamedPosition(Position);
    impl<'de> Deserialize<'de> for NamedPosition {
        fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Fields {
                x: i32,
                y: i32,
                z: i32,
            }
            let f: Fields = crate::bounded::named_map(d)?;
            Ok(Self(Position {
                x: f.x,
                y: f.y,
                z: f.z,
            }))
        }
    }
    crate::bounded::bounded::<D, NamedPosition, MAX_PUBLIC_AIM_OCCURRENCES>(d)
        .map(|rows| rows.into_iter().map(|row| row.0).collect())
}

fn valid_direction(direction: Position) -> bool {
    let components = [direction.x, direction.y, direction.z];
    components.iter().all(|value| (-1..=1).contains(value))
        && (1..=2).contains(&components.iter().filter(|value| **value != 0).count())
}
fn read_direction<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Position, D::Error> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Fields {
        x: i32,
        y: i32,
        z: i32,
    }
    let fields: Fields = crate::bounded::named_map(d)?;
    let direction = Position {
        x: fields.x,
        y: fields.y,
        z: fields.z,
    };
    if valid_direction(direction) {
        Ok(direction)
    } else {
        Err(serde::de::Error::custom("invalid public ray direction"))
    }
}
