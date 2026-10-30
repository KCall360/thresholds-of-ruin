use super::*;

pub(super) fn required_option<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> Result<Option<T>, D::Error> {
    Option::deserialize(d)
}

pub(super) fn valid_text(value: &str, max: usize) -> bool {
    value.len() <= max && !value.chars().any(char::is_control)
}
pub(super) fn valid_label(value: &str) -> bool {
    !value.is_empty() && valid_text(value, 128)
}
pub(super) fn valid_request_id(value: &str) -> bool {
    !value.trim().is_empty() && valid_text(value, 128)
}
pub(super) fn valid_token(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_CHART_TOKEN_BYTES
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}
pub(super) fn valid_asset(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_CHART_ASSET_BYTES
        && value.split('.').all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        })
}
pub(super) fn escaped(value: &str) -> Result<usize, &'static str> {
    value
        .len()
        .checked_mul(6)
        .and_then(|n| n.checked_add(2))
        .ok_or("chart escaped string size overflow")
}
pub(super) fn text<'de, D: serde::Deserializer<'de>, const N: usize>(
    d: D,
) -> Result<String, D::Error> {
    struct Text<const N: usize>;
    impl<const N: usize> serde::de::Visitor<'_> for Text<N> {
        type Value = String;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "at most {N} UTF-8 bytes without control characters")
        }
        fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<String, E> {
            if valid_text(value, N) {
                Ok(value.to_owned())
            } else {
                Err(E::custom("invalid chart string"))
            }
        }
    }
    d.deserialize_str(Text::<N>)
}
pub(super) fn token<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    let value = text::<D, MAX_CHART_TOKEN_BYTES>(d)?;
    if valid_token(&value) {
        Ok(value)
    } else {
        Err(serde::de::Error::custom("invalid chart token"))
    }
}
pub(super) fn optional_token<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Option<String>, D::Error> {
    #[derive(Deserialize)]
    struct Token(#[serde(deserialize_with = "token")] String);
    Option::<Token>::deserialize(d).map(|value| value.map(|value| value.0))
}
pub(super) fn request_id<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    let value = text::<D, 128>(d)?;
    if valid_request_id(&value) {
        Ok(value)
    } else {
        Err(serde::de::Error::custom("invalid request ID"))
    }
}
pub(super) fn asset<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    #[derive(Deserialize)]
    struct Text(#[serde(deserialize_with = "text::<_,MAX_CHART_ASSET_BYTES>")] String);
    Option::<Text>::deserialize(d).map(|value| value.map(|value| value.0))
}
pub(super) fn records<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Vec<ChartRecord>, D::Error> {
    crate::bounded::bounded::<D, _, MAX_CHART_PAGE_RECORDS>(d)
}
pub(super) fn context<'de, D: serde::Deserializer<'de>>(d: D) -> Result<StreamContext, D::Error> {
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
    if value.is_valid() {
        Ok(value)
    } else {
        Err(serde::de::Error::custom("invalid chart context"))
    }
}
pub(super) fn branch<'de, D: serde::Deserializer<'de>>(d: D) -> Result<BranchId, D::Error> {
    let value = text::<D, 128>(d)?;
    if valid_label(&value) {
        Ok(BranchId(value))
    } else {
        Err(serde::de::Error::custom("invalid branch"))
    }
}
pub(super) fn base<'de, D: serde::Deserializer<'de>>(d: D) -> Result<ObservationBase, D::Error> {
    fn cursor<'de, D: serde::Deserializer<'de>>(d: D) -> Result<crate::StreamCursor, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            #[serde(with = "crate::integers::unsigned")]
            sequence: u64,
            #[serde(with = "crate::integers::unsigned")]
            tick: u64,
        }
        let f: Fields = crate::bounded::named_map(d)?;
        Ok(crate::StreamCursor {
            sequence: f.sequence,
            tick: f.tick,
        })
    }
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Fields {
        snapshot: crate::SnapshotId,
        #[serde(deserialize_with = "cursor")]
        cursor: crate::StreamCursor,
        #[serde(with = "crate::integers::unsigned")]
        revision: u64,
    }
    let f: Fields = crate::bounded::named_map(d)?;
    Ok(ObservationBase {
        snapshot: f.snapshot,
        cursor: f.cursor,
        revision: f.revision,
    })
}
pub(super) fn reply_context<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<crate::ReplyContext, D::Error> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Input {
        #[serde(deserialize_with = "context")]
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
    fn cursor<'de, D: serde::Deserializer<'de>>(d: D) -> Result<crate::StreamCursor, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            #[serde(with = "crate::integers::unsigned")]
            sequence: u64,
            #[serde(with = "crate::integers::unsigned")]
            tick: u64,
        }
        let f: Fields = crate::bounded::named_map(d)?;
        Ok(crate::StreamCursor {
            sequence: f.sequence,
            tick: f.tick,
        })
    }
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Fields {
        snapshot: crate::SnapshotId,
        #[serde(deserialize_with = "input")]
        input: crate::InputContext,
        actor: crate::ActorId,
        #[serde(deserialize_with = "branch")]
        branch: BranchId,
        #[serde(deserialize_with = "cursor")]
        cursor: crate::StreamCursor,
        #[serde(with = "crate::integers::unsigned")]
        revision: u64,
    }
    let f: Fields = crate::bounded::named_map(d)?;
    Ok(crate::ReplyContext {
        snapshot: f.snapshot,
        input: f.input,
        actor: f.actor,
        branch: f.branch,
        cursor: f.cursor,
        revision: f.revision,
    })
}
pub(super) mod optional_position {
    use super::*;
    #[derive(Serialize, Deserialize)]
    #[serde(transparent)]
    struct Position(#[serde(with = "crate::integers::signed_triple")] [i64; 3]);
    pub fn serialize<S: serde::Serializer>(
        value: &Option<[i64; 3]>,
        s: S,
    ) -> Result<S::Ok, S::Error> {
        value.map(Position).serialize(s)
    }
    pub fn deserialize<'de, D: serde::Deserializer<'de>>(
        d: D,
    ) -> Result<Option<[i64; 3]>, D::Error> {
        Option::<Position>::deserialize(d).map(|value| value.map(|value| value.0))
    }
}
